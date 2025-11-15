use crate::adapters::{DemoMlDsa, DemoMlKem};
use crate::dsa::MlDsaEngine;
use crate::error::PqcResult;
use crate::kem::MlKemEngine;
use crate::key_manager::{KeyManager, ThresholdPolicy};
use crate::signatures::{DsaKeyState, SignatureManager};
use crate::types::{Bytes, TimestampMs};
use alloc::boxed::Box;
use spin::Mutex;

pub struct ContractState {
    pub key_manager: KeyManager,
    pub signature_manager: SignatureManager,
    pub signing_secret_key: Bytes,
    pub signing_key_state: DsaKeyState,
    pub monotonic_ms: TimestampMs,
}

static STATE: Mutex<Option<ContractState>> = Mutex::new(None);

const DEFAULT_ROTATION_INTERVAL_MS: u64 = 300_000;
const DEFAULT_THRESHOLD: ThresholdPolicy = ThresholdPolicy { t: 3, n: 5 };
const INITIAL_TIMESTAMP_MS: TimestampMs = 1_700_000_000_000;

impl ContractState {
    fn initialize() -> PqcResult<Self> {
        let kem_engine = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let mut key_manager =
            KeyManager::new(kem_engine, DEFAULT_THRESHOLD, DEFAULT_ROTATION_INTERVAL_MS);
        let _current = key_manager.keygen_and_install(INITIAL_TIMESTAMP_MS)?;

        let dsa_engine = MlDsaEngine::new(Box::new(DemoMlDsa::new()));
        let mut signature_manager = SignatureManager::new(dsa_engine);
        let (signing_state, signing_pair) =
            signature_manager.generate_signing_key(INITIAL_TIMESTAMP_MS)?;

        Ok(Self {
            key_manager,
            signature_manager,
            signing_secret_key: signing_pair.secret_key,
            signing_key_state: signing_state,
            monotonic_ms: INITIAL_TIMESTAMP_MS,
        })
    }

    /// Update the monotonic timestamp based on optional host input.
    pub fn advance_time(&mut self, candidate: Option<TimestampMs>) -> TimestampMs {
        match candidate {
            Some(ts) if ts > self.monotonic_ms => self.monotonic_ms = ts,
            _ => self.monotonic_ms = self.monotonic_ms.saturating_add(1),
        }
        self.monotonic_ms
    }
}

/// Execute a closure with mutable access to the singleton contract state.
pub fn with_contract_state<F, R>(mut f: F) -> R
where
    F: FnMut(&mut ContractState) -> R,
{
    let mut guard = STATE.lock();
    if guard.is_none() {
        *guard = Some(ContractState::initialize().expect("contract state initialization failed"));
    }

    // Safe unwrap: state is initialized above.
    f(guard.as_mut().unwrap())
}

#[cfg(test)]
pub fn reset_state_for_tests() {
    let mut guard = STATE.lock();
    *guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_time_prefers_hint_and_remains_monotonic() {
        reset_state_for_tests();
        with_contract_state(|state| {
            assert_eq!(state.monotonic_ms, INITIAL_TIMESTAMP_MS);
            let hinted = state.advance_time(Some(INITIAL_TIMESTAMP_MS + 500));
            assert_eq!(hinted, INITIAL_TIMESTAMP_MS + 500);
            let monotonic = state.advance_time(Some(INITIAL_TIMESTAMP_MS + 100));
            assert_eq!(monotonic, INITIAL_TIMESTAMP_MS + 501);
        });
    }

    #[test]
    fn reset_state_restores_initial_timestamp() {
        reset_state_for_tests();
        with_contract_state(|state| {
            state.advance_time(Some(INITIAL_TIMESTAMP_MS + 10));
        });
        reset_state_for_tests();
        with_contract_state(|state| {
            assert_eq!(state.monotonic_ms, INITIAL_TIMESTAMP_MS);
        });
    }
}
