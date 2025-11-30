use super::ContractState;
use crate::adapters::DemoMlDsa;
use crate::dsa::{MlDsaEngine, MlDsaKeyPair};
use crate::error::{PqcError, PqcResult};
use crate::kem::{MlKem, MlKemEncapsulation, MlKemEngine, MlKemKeyPair};
use crate::key_manager::{KeyManager, ThresholdPolicy};
use crate::signatures::SignatureManager;
use crate::types::{Bytes, SecurityLevel, TimestampMs};
use alloc::boxed::Box;
use hex_literal::hex;
use spin::Mutex;

const BASE_TIMESTAMP_MS: TimestampMs = 1_700_000_000_000;
const THRESHOLD: ThresholdPolicy = ThresholdPolicy { t: 3, n: 5 };

const SIGNING_SECRET_KEY: [u8; 32] =
    hex!("4aa98eb9ae7ea039cf03cb11479b778e408fa633d1c9702512aa5d221cb12df6");
const SIGNING_PUBLIC_KEY: [u8; 32] =
    hex!("fc350732dfabcea27af2e14c67c340084095a062e851d3599861186106b1b6e2");

struct RecordedSample {
    offset_ms: u64,
    public_key: [u8; 32],
    secret_key: [u8; 32],
    ciphertext: [u8; 48],
    shared_secret: [u8; 32],
}

include!(concat!(env!("OUT_DIR"), "/recorded_trace.rs"));

pub(super) fn build_contract_state() -> PqcResult<ContractState> {
    let kem_engine = MlKemEngine::new(Box::new(RecordedMlKem::new(&RECORDED_SAMPLES)));
    let mut key_manager = KeyManager::new(kem_engine, THRESHOLD, TRACE_ROTATION_INTERVAL_MS);

    let first_sample = RECORDED_SAMPLES
        .first()
        .ok_or(PqcError::InternalError("recorded ml-kem samples missing"))?;
    let first_ts = BASE_TIMESTAMP_MS + first_sample.offset_ms;
    let _ = key_manager.keygen_and_install(first_ts)?;

    let dsa_engine = MlDsaEngine::new(Box::new(DemoMlDsa::new()));
    let mut signature_manager = SignatureManager::new(dsa_engine);
    let signing_pair = MlDsaKeyPair {
        public_key: SIGNING_PUBLIC_KEY.to_vec(),
        secret_key: SIGNING_SECRET_KEY.to_vec(),
        level: SecurityLevel::MlDsa192,
    };
    let signing_state = signature_manager.install_external_key(first_ts, signing_pair.clone());

    Ok(ContractState {
        key_manager,
        signature_manager,
        signing_secret_key: signing_pair.secret_key,
        signing_key_state: signing_state,
        monotonic_ms: first_ts,
    })
}

struct RecordedMlKem {
    samples: &'static [RecordedSample],
    cursor: Mutex<usize>,
}

impl RecordedMlKem {
    const fn new(samples: &'static [RecordedSample]) -> Self {
        Self {
            samples,
            cursor: Mutex::new(0),
        }
    }

    fn find_by_public_key(&self, public_key: &[u8]) -> PqcResult<&RecordedSample> {
        self.samples
            .iter()
            .find(|sample| &sample.public_key[..] == public_key)
            .ok_or(PqcError::InvalidInput("unknown recorded ml-kem key"))
    }

    fn find_by_ciphertext(&self, ciphertext: &[u8]) -> PqcResult<&RecordedSample> {
        self.samples
            .iter()
            .find(|sample| &sample.ciphertext[..] == ciphertext)
            .ok_or(PqcError::InvalidInput("unknown recorded ml-kem ciphertext"))
    }
}

impl MlKem for RecordedMlKem {
    fn level(&self) -> SecurityLevel {
        SecurityLevel::MlKem192
    }

    fn keygen(&self) -> PqcResult<MlKemKeyPair> {
        let mut guard = self.cursor.lock();
        let idx = *guard;
        if idx >= self.samples.len() {
            return Err(PqcError::InternalError("recorded ml-kem exhausted"));
        }
        let sample = &self.samples[idx];
        *guard = idx.saturating_add(1);
        Ok(MlKemKeyPair {
            public_key: sample.public_key.to_vec(),
            secret_key: sample.secret_key.to_vec(),
            level: SecurityLevel::MlKem192,
        })
    }

    fn encapsulate(&self, public_key: &[u8]) -> PqcResult<MlKemEncapsulation> {
        let sample = self.find_by_public_key(public_key)?;
        Ok(MlKemEncapsulation {
            ciphertext: sample.ciphertext.to_vec(),
            shared_secret: sample.shared_secret.to_vec(),
        })
    }

    fn decapsulate(&self, _secret_key: &[u8], ciphertext: &[u8]) -> PqcResult<Bytes> {
        let sample = self.find_by_ciphertext(ciphertext)?;
        Ok(sample.shared_secret.to_vec())
    }
}
