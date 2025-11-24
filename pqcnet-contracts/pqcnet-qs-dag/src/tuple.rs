use alloc::string::String;
use alloc::vec::Vec;

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::icosuple::IcosupleLayer;
use crate::state::{StateDiff, StateOp};

/// Domain-aware tagging that lets tuplechains map to regional shards or industry
/// overlays (healthcare, DePIN, finance, etc.).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TupleDomain {
    CoreInfrastructure,
    Finance,
    Healthcare,
    Identity,
    Depin,
    Interop,
    Custom(String),
}

impl TupleDomain {
    pub fn label(&self) -> &str {
        match self {
            TupleDomain::CoreInfrastructure => "core",
            TupleDomain::Finance => "finance",
            TupleDomain::Healthcare => "healthcare",
            TupleDomain::Identity => "identity",
            TupleDomain::Depin => "depin",
            TupleDomain::Interop => "interop",
            TupleDomain::Custom(_) => "custom",
        }
    }
}

/// Payload profiling keeps tuple scheduling deterministic and allows execution
/// pipelines (e.g., WASM marketplace vs. GraphQL routes) to apply QoS policies.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PayloadProfile {
    SmartContract,
    AssetTransfer,
    MessageBus,
    Governance,
    Marketplace,
    Custom(String),
}

/// QIP tags identify the interoperability corridor a tuple belongs to.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum QIPTag {
    Native,
    Bridge(String),
    Relay(String),
    Sidechain(String),
}

/// Signature metadata for Dilithium/Falcon/etc. verification.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TupleValidation {
    pub scheme: String,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
}

impl TupleValidation {
    pub fn new(scheme: impl Into<String>, signature: Vec<u8>, public_key: Vec<u8>) -> Self {
        Self {
            scheme: scheme.into(),
            signature,
            public_key,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TupleProofKind {
    ZkSnark,
    ZkStark,
    Merkle,
    ArchiveReceipt,
    Custom(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TupleProof {
    pub kind: TupleProofKind,
    pub bytes: Vec<u8>,
}

impl TupleProof {
    pub fn new(kind: TupleProofKind, bytes: Vec<u8>) -> Self {
        Self { kind, bytes }
    }
}

/// TupleEnvelope describes the data carried by a DAG vertex.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TupleEnvelope {
    pub domain: TupleDomain,
    pub layer: IcosupleLayer,
    pub payload_profile: PayloadProfile,
    pub sender_did: String,
    pub receiver_did: String,
    pub amount: u128,
    pub payload_commitment: [u8; 32],
    pub payload_inline: Option<Vec<u8>>,
    pub qrng_seed: [u8; 32],
    pub timestamp_ns: u64,
    pub qip_tag: QIPTag,
    pub zk_proof: Option<TupleProof>,
    pub validation: TupleValidation,
}

impl TupleEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        domain: TupleDomain,
        layer: IcosupleLayer,
        payload_profile: PayloadProfile,
        sender_did: impl Into<String>,
        receiver_did: impl Into<String>,
        amount: u128,
        payload: &[u8],
        qrng_seed: [u8; 32],
        timestamp_ns: u64,
        qip_tag: QIPTag,
        zk_proof: Option<TupleProof>,
        validation: TupleValidation,
    ) -> Self {
        let payload_commitment = commit_payload(payload, &qrng_seed);
        Self {
            domain,
            layer,
            payload_profile,
            sender_did: sender_did.into(),
            receiver_did: receiver_did.into(),
            amount,
            payload_commitment,
            payload_inline: Some(payload.to_vec()),
            qrng_seed,
            timestamp_ns,
            qip_tag,
            zk_proof,
            validation,
        }
    }

    pub fn without_inline_payload(mut self) -> Self {
        self.payload_inline = None;
        self
    }

    pub fn verifies_payload(&self, payload: &[u8]) -> bool {
        commit_payload(payload, &self.qrng_seed) == self.payload_commitment
    }

    pub fn attach_to_diff(self, diff: &mut StateDiff) {
        diff.tuple = Some(self);
    }

    pub fn into_state_diff(
        self,
        id: impl Into<String>,
        author: impl Into<String>,
        parents: Vec<String>,
        lamport: u64,
        ops: Vec<StateOp>,
    ) -> StateDiff {
        StateDiff::with_tuple(id, author, parents, lamport, ops, self)
    }
}

/// Derive a deterministic commitment using BLAKE3 keyed by the QRNG seed so that
/// tuple deduplication can be verified independently inside pools.
pub fn commit_payload(payload: &[u8], qrng_seed: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(qrng_seed);
    hasher.update(payload);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icosuple::LayerClass;

    fn sample_envelope() -> TupleEnvelope {
        TupleEnvelope::new(
            TupleDomain::Finance,
            IcosupleLayer::CONSENSUS_TIER_9,
            PayloadProfile::AssetTransfer,
            "did:example:alice",
            "did:example:bob",
            42,
            b"transfer",
            [0xAA; 32],
            123,
            QIPTag::Native,
            None,
            TupleValidation::new("Dilithium5", vec![1, 2, 3], vec![4, 5, 6]),
        )
    }

    #[test]
    fn commitment_roundtrip() {
        let env = sample_envelope();
        assert!(env.verifies_payload(b"transfer"));
        assert!(!env.verifies_payload(b"tampered"));
    }

    #[test]
    fn attach_to_diff_sets_metadata() {
        let tuple = sample_envelope();
        let mut diff = StateDiff::new(
            "tuple-1",
            "node-a",
            vec!["genesis".into()],
            1,
            vec![StateOp::upsert("k", "v")],
        );
        tuple.attach_to_diff(&mut diff);
        let ref_tuple = diff.tuple.as_ref().expect("tuple metadata set");
        assert_eq!(ref_tuple.layer.class(), LayerClass::Consensus);
    }
}
