#![cfg_attr(target_arch = "wasm32", no_std)]

//! Quantum-Forward Key Hopping (QFKH)
//! ---------------------------------
//!
//! This crate provides a dynamic rotation mechanism that hops ML-KEM keys on a
//! deterministic schedule. Each hop derives a fresh symmetric secret, ensuring
//! that intercepted ciphertext remains indecipherable even if an adversary later
//! gains quantum capabilities.

extern crate alloc;

use alloc::collections::btree_map::{BTreeMap, Entry};
use alloc::vec::Vec;
use autheo_pqc_core::error::{PqcError, PqcResult};
use autheo_pqc_core::kem::{MlKemEngine, MlKemKeyPair};
use autheo_pqc_core::types::{Bytes, KeyId, SecurityLevel, TimestampMs};
use blake2::Blake2s256;
use digest::Digest;

const DOMAIN_KEY_ID: &[u8] = b"QFKH:KEY:ID";
const DOMAIN_FORWARD_KEY: &[u8] = b"QFKH:FORWARD";
const DOMAIN_COMMITMENT: &[u8] = b"QFKH:CIPH";

/// Configuration for the Quantum-Forward Key Hopper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QfkhConfig {
    /// Duration of a hop epoch.
    pub rotation_interval_ms: TimestampMs,
    /// Number of future epochs to pre-materialize for deterministic behavior.
    pub lookahead_epochs: u32,
}

impl QfkhConfig {
    /// Validate parameters and instantiate a config.
    pub fn new(rotation_interval_ms: TimestampMs, lookahead_epochs: u32) -> PqcResult<Self> {
        if rotation_interval_ms == 0 {
            return Err(PqcError::InvalidInput("rotation interval must be > 0"));
        }
        if lookahead_epochs == 0 {
            return Err(PqcError::InvalidInput("lookahead must be > 0"));
        }
        Ok(Self {
            rotation_interval_ms,
            lookahead_epochs,
        })
    }

    /// Convert a timestamp into an epoch identifier.
    pub fn epoch_for(&self, timestamp_ms: TimestampMs) -> u64 {
        timestamp_ms / self.rotation_interval_ms
    }

    /// Return the inclusive/exclusive window for a given epoch.
    pub fn window_bounds(&self, epoch: u64) -> (TimestampMs, TimestampMs) {
        let start = epoch * self.rotation_interval_ms;
        let end = start + self.rotation_interval_ms;
        (start, end)
    }
}

/// Announcement sent to peers so they can encapsulate into an upcoming epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QfkhEpochTicket {
    pub epoch: u64,
    pub key_id: KeyId,
    pub window_start_ms: TimestampMs,
    pub window_end_ms: TimestampMs,
    pub level: SecurityLevel,
    pub public_key: Bytes,
}

/// Ciphertext produced by encapsulating against a ticket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QfkhHopCiphertext {
    pub epoch: u64,
    pub key_id: KeyId,
    pub ciphertext: Bytes,
    pub commitment: [u8; 32],
}

/// Symmetric key material derived for a specific hop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QfkhSessionKey {
    pub epoch: u64,
    pub key_id: KeyId,
    pub derived_key: [u8; 32],
    pub activated_at: TimestampMs,
}

impl QfkhSessionKey {
    /// Borrow the derived key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.derived_key
    }
}

struct PreparedEpoch {
    keypair: MlKemKeyPair,
    key_id: KeyId,
    window_start: TimestampMs,
    window_end: TimestampMs,
}

/// Stateful controller that hops lattice-backed session keys.
pub struct QuantumForwardKeyHopper {
    engine: MlKemEngine,
    config: QfkhConfig,
    prepared: BTreeMap<u64, PreparedEpoch>,
    active: Option<QfkhSessionKey>,
}

impl QuantumForwardKeyHopper {
    /// Instantiate a hopper with the provided ML-KEM engine and policy.
    pub fn new(engine: MlKemEngine, config: QfkhConfig) -> Self {
        Self {
            engine,
            config,
            prepared: BTreeMap::new(),
            active: None,
        }
    }

    /// Current configuration.
    pub fn config(&self) -> &QfkhConfig {
        &self.config
    }

    /// Currently active session (if any).
    pub fn active_session(&self) -> Option<&QfkhSessionKey> {
        self.active.as_ref()
    }

    /// Materialize a ticket for the epoch implied by `timestamp_ms`.
    pub fn announce_epoch(&mut self, timestamp_ms: TimestampMs) -> PqcResult<QfkhEpochTicket> {
        let epoch = self.config.epoch_for(timestamp_ms);
        self.materialize_epoch(epoch)
    }

    /// Explicitly materialize a ticket for a future epoch.
    pub fn announce_specific_epoch(&mut self, epoch: u64) -> PqcResult<QfkhEpochTicket> {
        self.materialize_epoch(epoch)
    }

    /// Ensure that `lookahead_epochs` future hops are prepared.
    pub fn ensure_lookahead(&mut self, now_ms: TimestampMs) -> PqcResult<Vec<QfkhEpochTicket>> {
        let mut minted = Vec::new();
        let start_epoch = self.config.epoch_for(now_ms);
        for offset in 0..self.config.lookahead_epochs {
            let epoch = start_epoch + offset as u64;
            let existed = self.prepared.contains_key(&epoch);
            let ticket = self.materialize_epoch(epoch)?;
            if !existed {
                minted.push(ticket);
            }
        }
        Ok(minted)
    }

    /// Encapsulate to a peer that announced `ticket`, returning the capsule and derived key.
    pub fn encapsulate_for(
        &mut self,
        ticket: &QfkhEpochTicket,
        activated_at: TimestampMs,
    ) -> PqcResult<(QfkhHopCiphertext, QfkhSessionKey)> {
        ensure_within_window(ticket.window_start_ms, ticket.window_end_ms, activated_at)?;
        let enc = self.engine.encapsulate(&ticket.public_key)?;
        let derived_key = derive_forward_key(&enc.shared_secret, ticket.epoch, activated_at);
        let commitment = derive_commitment(&ticket.key_id, &enc.ciphertext);
        let session = QfkhSessionKey {
            epoch: ticket.epoch,
            key_id: ticket.key_id.clone(),
            derived_key,
            activated_at,
        };
        self.active = Some(session.clone());
        Ok((
            QfkhHopCiphertext {
                epoch: ticket.epoch,
                key_id: ticket.key_id.clone(),
                ciphertext: enc.ciphertext,
                commitment,
            },
            session,
        ))
    }

    /// Decapsulate and activate a hop produced by a remote peer.
    pub fn activate_from(
        &mut self,
        capsule: &QfkhHopCiphertext,
        received_at: TimestampMs,
    ) -> PqcResult<QfkhSessionKey> {
        let slot = self
            .prepared
            .get(&capsule.epoch)
            .ok_or(PqcError::InvalidInput("epoch not prepared"))?;
        ensure_within_window(slot.window_start, slot.window_end, received_at)?;
        if slot.key_id != capsule.key_id {
            return Err(PqcError::InvalidInput("key id mismatch"));
        }
        let expected = derive_commitment(&slot.key_id, &capsule.ciphertext);
        if expected != capsule.commitment {
            return Err(PqcError::VerifyFailed);
        }
        let shared = self
            .engine
            .decapsulate(&slot.keypair.secret_key, &capsule.ciphertext)?;
        let derived_key = derive_forward_key(&shared, capsule.epoch, received_at);
        let session = QfkhSessionKey {
            epoch: capsule.epoch,
            key_id: capsule.key_id.clone(),
            derived_key,
            activated_at: received_at,
        };
        self.active = Some(session.clone());
        Ok(session)
    }

    /// Determine whether a new hop is required for the provided timestamp.
    pub fn needs_rotation(&self, now_ms: TimestampMs) -> bool {
        match &self.active {
            None => true,
            Some(session) => {
                let current_epoch = self.config.epoch_for(now_ms);
                current_epoch > session.epoch
            }
        }
    }

    /// Drop prepared epochs that are strictly older than `retain_from_epoch`.
    pub fn prune(&mut self, retain_from_epoch: u64) {
        let mut stale = Vec::new();
        for epoch in self.prepared.keys().copied() {
            if epoch + 1 < retain_from_epoch {
                stale.push(epoch);
            } else {
                break;
            }
        }
        for epoch in stale {
            self.prepared.remove(&epoch);
        }
    }

    fn materialize_epoch(&mut self, epoch: u64) -> PqcResult<QfkhEpochTicket> {
        match self.prepared.entry(epoch) {
            Entry::Occupied(entry) => Ok(ticket_from_slot(epoch, entry.get())),
            Entry::Vacant(entry) => {
                let pair = self.engine.keygen()?;
                let (start, end) = self.config.window_bounds(epoch);
                let key_id = derive_key_id(epoch, &pair.public_key);
                let slot = PreparedEpoch {
                    keypair: pair,
                    key_id: key_id.clone(),
                    window_start: start,
                    window_end: end,
                };
                let ticket = ticket_from_slot(epoch, &slot);
                entry.insert(slot);
                Ok(ticket)
            }
        }
    }
}

fn ticket_from_slot(epoch: u64, slot: &PreparedEpoch) -> QfkhEpochTicket {
    QfkhEpochTicket {
        epoch,
        key_id: slot.key_id.clone(),
        window_start_ms: slot.window_start,
        window_end_ms: slot.window_end,
        level: slot.keypair.level,
        public_key: slot.keypair.public_key.clone(),
    }
}

fn ensure_within_window(
    start: TimestampMs,
    end: TimestampMs,
    timestamp: TimestampMs,
) -> PqcResult<()> {
    if timestamp < start || timestamp >= end {
        return Err(PqcError::InvalidInput("timestamp outside epoch window"));
    }
    Ok(())
}

fn derive_key_id(epoch: u64, public_key: &[u8]) -> KeyId {
    let mut hasher = Blake2s256::new();
    hasher.update(DOMAIN_KEY_ID);
    hasher.update(epoch.to_le_bytes());
    hasher.update(public_key);
    let digest = hasher.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(&digest);
    KeyId(id)
}

fn derive_forward_key(shared_secret: &[u8], epoch: u64, activated_at: TimestampMs) -> [u8; 32] {
    let mut hasher = Blake2s256::new();
    hasher.update(DOMAIN_FORWARD_KEY);
    hasher.update(epoch.to_le_bytes());
    hasher.update(activated_at.to_le_bytes());
    hasher.update(shared_secret);
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn derive_commitment(key_id: &KeyId, ciphertext: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2s256::new();
    hasher.update(DOMAIN_COMMITMENT);
    hasher.update(&key_id.0);
    hasher.update(ciphertext);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use autheo_pqc_core::adapters::DemoMlKem;
    use autheo_pqc_core::runtime;

    fn make_engine() -> MlKemEngine {
        MlKemEngine::new(Box::new(DemoMlKem::new()))
    }

    #[test]
    fn session_keys_match_after_hop() {
        runtime::reset_state_for_tests();
        let config = QfkhConfig::new(5_000, 3).unwrap();
        let mut responder = QuantumForwardKeyHopper::new(make_engine(), config);
        let mut initiator = QuantumForwardKeyHopper::new(make_engine(), config);

        let ticket = responder.announce_epoch(6_500).unwrap();
        let (capsule, initiator_key) = initiator
            .encapsulate_for(&ticket, 7_000)
            .expect("encapsulate");
        let responder_key = responder.activate_from(&capsule, 7_000).expect("activate");

        assert_eq!(initiator_key.derived_key, responder_key.derived_key);
        assert!(!responder.needs_rotation(7_000));
        assert!(responder.needs_rotation(12_500));
    }

    #[test]
    fn lookahead_materializes_future_epochs() {
        runtime::reset_state_for_tests();
        let mut hopper =
            QuantumForwardKeyHopper::new(make_engine(), QfkhConfig::new(10_000, 2).unwrap());
        let minted = hopper.ensure_lookahead(0).expect("lookahead");
        assert_eq!(minted.len(), 2);
        let minted_again = hopper.ensure_lookahead(0).expect("lookahead");
        assert_eq!(minted_again.len(), 0, "no duplicate tickets");
    }

    #[test]
    fn window_violation_is_rejected() {
        runtime::reset_state_for_tests();
        let mut hopper =
            QuantumForwardKeyHopper::new(make_engine(), QfkhConfig::new(4_000, 1).unwrap());
        let ticket = hopper.announce_epoch(0).unwrap();
        let err = hopper
            .encapsulate_for(&ticket, ticket.window_end_ms + 1)
            .expect_err("window violation");
        assert!(matches!(err, PqcError::InvalidInput(_)));
    }
}
