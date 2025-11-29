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

const ROTATION_INTERVAL_MS: u64 = 6_000;
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

const RECORDED_SAMPLES: &[RecordedSample] = &[
    // Captured from qfkh_prod_trace.json epoch 0
    RecordedSample {
        offset_ms: 0,
        public_key: hex!("d7b0fb204399baf73a88cd54b634524e795b2e639c5445362645928b4f70c6a5"),
        secret_key: hex!("4aa98eb9ae7ea039cf03cb11479b778e408fa633d1c9702512aa5d221cb12df6"),
        ciphertext: hex!(
            "4a0479d7d28610ad18143ccd2626c99c64e38f8d2eed137b7269b1498174dd776fd37a037c21aec8ff974d855c61b818"
        ),
        shared_secret: hex!("803e7695fabc0807f54d777fdd01724c7d655d0d386503fbbb97714188273f58"),
    },
    // Captured from qfkh_prod_trace.json epoch 1
    RecordedSample {
        offset_ms: 6_000,
        public_key: hex!("5b1df29ec8c2221bf102bb337025128224f1866fa155534fb6805589d2d27d7e"),
        secret_key: hex!("a9c77a34740bd49213143be3b64374403492be16c6443dae7358d85d81665e2f"),
        ciphertext: hex!(
            "fda38b9b8b132af34fc5c93726d7103332fb1b25f84f0c92ba16969bf065a38188a62549522256678280156b2a77b31d"
        ),
        shared_secret: hex!("84ce0313049b370e4eb16d37a0c14c4708b91d23f5a6c9282261d7eb3c2b0a2d"),
    },
    // Captured from qfkh_prod_trace.json epoch 2
    RecordedSample {
        offset_ms: 12_000,
        public_key: hex!("a31955521cd796f73258fa062a23b9abc8e0809a6b09f7ab4843c6c440abe981"),
        secret_key: hex!("e09997ef1ed91cae79745b6fe90e3aae67a194057d09f18ef065c01950700980"),
        ciphertext: hex!(
            "9753ace7e4c1c2a27555eeff07503600b2df908d42cc05228fc449f6994a10887e357ab947155c1b5550f0026f040b48"
        ),
        shared_secret: hex!("739142abd8fc1a7badc0dc2e78271467bc01fffdb69e2660fba5912c74a9932b"),
    },
];

pub(super) fn build_contract_state() -> PqcResult<ContractState> {
    let kem_engine = MlKemEngine::new(Box::new(RecordedMlKem::new(RECORDED_SAMPLES)));
    let mut key_manager = KeyManager::new(kem_engine, THRESHOLD, ROTATION_INTERVAL_MS);

    let first_ts = BASE_TIMESTAMP_MS + RECORDED_SAMPLES[0].offset_ms;
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
