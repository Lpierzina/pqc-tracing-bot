use std::{
    env,
    fs::{self, create_dir_all, File},
    io::{BufReader, BufWriter, Read},
    path::Path,
    sync::{Arc, Once},
    time::Instant,
};

use blake3::Hasher;
use halo2_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{
        create_proof, keygen_pk, keygen_vk, verify_proof, Advice, Circuit, Column,
        ConstraintSystem, Error as Halo2Error, Instance, ProvingKey, SingleVerifier,
    },
    poly::{commitment::Params, Rotation},
    transcript::{Blake2bRead, Blake2bWrite, Challenge255},
};
use halo2curves::bn256::{Fr, G1Affine};
use rand_core::OsRng;
use rayon::ThreadPoolBuilder;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::ZkConfig;
use serde_json::json;

const HALO2_PARAMS_BITS_ENV: &str = "AUTHEO_HALO2_PARAMS_BITS";
const HALO2_MIN_K: u32 = 8;
const HALO2_MAX_K: u32 = 28;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkStatement {
    pub circuit_id: String,
    pub claim: String,
    pub public_inputs: Vec<String>,
}

impl ZkStatement {
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(self.circuit_id.as_bytes());
        hasher.update(self.claim.as_bytes());
        for input in &self.public_inputs {
            hasher.update(input.as_bytes());
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkProof {
    pub proof_system: String,
    pub curve: String,
    pub proof_bytes: Vec<u8>,
    pub statement_hash: [u8; 32],
    pub soundness_error: f64,
}

#[derive(Debug, Error, Clone)]
#[error("{0}")]
pub struct ZkError(pub String);

impl ZkError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

pub trait ZkProver: Send + Sync {
    fn prove(&self, statement: &ZkStatement) -> Result<ZkProof, ZkError>;

    fn verify(&self, proof: &ZkProof, statement: &ZkStatement) -> Result<(), ZkError> {
        if proof.statement_hash != statement.hash() {
            Err(ZkError::new("statement hash mismatch"))
        } else {
            Ok(())
        }
    }
}

/// Generates the Halo2 parameter/proving artifacts on disk without booting the
/// entire EZPH pipeline. Call this during deployment so subsequent runs can skip
/// the expensive keygen path.
pub fn warm_halo2_key_cache(config: &ZkConfig) -> Result<(), ZkError> {
    println!(
        "[halo2-cache] priming circuit '{}' (curve={}, soundness={:.2e})",
        config.circuit_id, config.curve, config.soundness
    );
    println!(
        "[halo2-cache] params={}, pk={}, vk={}",
        config.params_path.display(),
        config.proving_key_path.display(),
        config.verifying_key_path.display()
    );
    if let Ok(raw) = env::var("AUTHEO_RAYON_THREADS") {
        println!("[halo2-cache] AUTHEO_RAYON_THREADS={raw}");
    }
    if let Ok(raw) = env::var("RAYON_NUM_THREADS") {
        println!("[halo2-cache] RAYON_NUM_THREADS={raw}");
    }
    Halo2ZkProver::limit_rayon_threads();
    let k = Halo2ZkProver::derive_k(config);
    println!("[halo2-cache] derived k={k} from target soundness.");
    let params = log_phase("load/create Halo2 params", || {
        Halo2ZkProver::load_or_create_params(config, k)
    })?;
    let pk = log_phase("build Halo2 proving key", || {
        Halo2ZkProver::build_pk(config, &params)
    })?;
    log_phase("persist Halo2 key metadata", || {
        Halo2ZkProver::persist_key_material(config, k, &params, &pk)
    })?;
    println!("[halo2-cache] warmup complete (params_bits=k={k}); artifacts persisted.");
    Ok(())
}

fn log_phase<T, F>(label: &str, action: F) -> Result<T, ZkError>
where
    F: FnOnce() -> Result<T, ZkError>,
{
    println!("[halo2-cache] >> {label}");
    let start = Instant::now();
    match action() {
        Ok(value) => {
            println!(
                "[halo2-cache] << {label} complete in {:.2?}",
                start.elapsed()
            );
            Ok(value)
        }
        Err(err) => {
            eprintln!(
                "[halo2-cache] !! {label} failed after {:.2?}: {err}",
                start.elapsed()
            );
            Err(err)
        }
    }
}

pub struct MockCircomProver {
    config: ZkConfig,
}

impl MockCircomProver {
    pub fn new(config: ZkConfig) -> Self {
        Self { config }
    }
}

impl ZkProver for MockCircomProver {
    fn prove(&self, statement: &ZkStatement) -> Result<ZkProof, ZkError> {
        if statement.public_inputs.is_empty() {
            return Err(ZkError::new("public inputs missing"));
        }
        let mut hasher = Hasher::new();
        hasher.update(&statement.hash());
        hasher.update(self.config.proof_system.as_bytes());
        hasher.update(self.config.curve.as_bytes());
        let mut proof_bytes = vec![0u8; 96];
        proof_bytes.copy_from_slice(&hasher.finalize().as_bytes().repeat(3)[..96]);
        Ok(ZkProof {
            proof_system: self.config.proof_system.clone(),
            curve: self.config.curve.clone(),
            proof_bytes,
            statement_hash: statement.hash(),
            soundness_error: self.config.soundness,
        })
    }

    fn verify(&self, proof: &ZkProof, statement: &ZkStatement) -> Result<(), ZkError> {
        if proof.statement_hash != statement.hash() {
            Err(ZkError::new("mock proof mismatch"))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
pub struct Halo2ZkProver {
    config: ZkConfig,
    params: Arc<Params<G1Affine>>,
    pk: Arc<ProvingKey<G1Affine>>,
}

impl Halo2ZkProver {
    pub fn new(config: ZkConfig) -> Result<Self, ZkError> {
        Self::limit_rayon_threads();
        let k = Self::derive_k(&config);
        let params = Arc::new(Self::load_or_create_params(&config, k)?);
        let pk = Arc::new(Self::build_pk(&config, params.as_ref())?);
        Self::persist_key_material(&config, k, params.as_ref(), pk.as_ref())?;
        Ok(Self { config, params, pk })
    }

    fn derive_k(config: &ZkConfig) -> u32 {
        if let Some(k) = Self::env_forced_params_bits() {
            return k;
        }
        let security_bits = (-config.soundness.log2()).ceil().max(1.0) as u32;
        let extra = (security_bits / 64).min(6);
        18 + extra
    }

    fn scalar_for(statement: &ZkStatement) -> Fr {
        let hash = statement.hash();
        Fr::from_bytes(&hash).unwrap_or_else(|| Fr::zero())
    }
}

impl Halo2ZkProver {
    fn limit_rayon_threads() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            if env::var_os("RAYON_NUM_THREADS").is_some() {
                return;
            }
            let threads = env::var("AUTHEO_RAYON_THREADS")
                .ok()
                .and_then(|raw| raw.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(1);
            if let Err(err) = ThreadPoolBuilder::new().num_threads(threads).build_global() {
                eprintln!(
                    "halo2 prover: failed to configure Rayon thread pool \
                     (requested {threads} threads): {err}"
                );
            }
        });
    }

    fn env_forced_params_bits() -> Option<u32> {
        match env::var(HALO2_PARAMS_BITS_ENV) {
            Ok(raw) => match raw.parse::<u32>() {
                Ok(value) if (HALO2_MIN_K..=HALO2_MAX_K).contains(&value) => {
                    println!(
                        "[halo2-cache] using k override {value} from {HALO2_PARAMS_BITS_ENV}"
                    );
                    Some(value)
                }
                Ok(value) => {
                    eprintln!(
                        "[halo2-cache] ignoring {HALO2_PARAMS_BITS_ENV}={value}: expected {}..={}",
                        HALO2_MIN_K, HALO2_MAX_K
                    );
                    None
                }
                Err(err) => {
                    eprintln!(
                        "[halo2-cache] ignoring {HALO2_PARAMS_BITS_ENV}={raw}: {err}"
                    );
                    None
                }
            },
            Err(_) => None,
        }
    }

    fn ensure_parent(path: &Path) -> Result<(), ZkError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent).map_err(|err| {
                    ZkError::new(format!(
                        "failed to create directory {}: {err}",
                        parent.display()
                    ))
                })?;
            }
        }
        Ok(())
    }

    fn load_or_create_params(config: &ZkConfig, k: u32) -> Result<Params<G1Affine>, ZkError> {
        let path = config.params_path.as_path();
        if path.exists() {
            let mut encoded_k = [0u8; 4];
            {
                let mut header = BufReader::new(File::open(path).map_err(|err| {
                    ZkError::new(format!(
                        "failed to open Halo2 params {}: {err}",
                        path.display()
                    ))
                })?);
                header.read_exact(&mut encoded_k).map_err(|err| {
                    ZkError::new(format!(
                        "failed to read Halo2 params header {}: {err}",
                        path.display()
                    ))
                })?;
            }
            let existing_k = u32::from_le_bytes(encoded_k);
            if existing_k != k {
                return Err(ZkError::new(format!(
                    "Halo2 params {} were generated with k={} but config requires k={}",
                    path.display(),
                    existing_k,
                    k
                )));
            }
            let mut reader = BufReader::new(File::open(path).map_err(|err| {
                ZkError::new(format!(
                    "failed to open Halo2 params {}: {err}",
                    path.display()
                ))
            })?);
            Params::<G1Affine>::read(&mut reader).map_err(|err| {
                ZkError::new(format!(
                    "failed to deserialize Halo2 params {}: {err}",
                    path.display()
                ))
            })
        } else {
            Self::ensure_parent(path)?;
            let params = Params::<G1Affine>::new(k);
            let mut writer = BufWriter::new(File::create(path).map_err(|err| {
                ZkError::new(format!(
                    "failed to create Halo2 params {}: {err}",
                    path.display()
                ))
            })?);
            params
                .write(&mut writer)
                .map_err(|err| ZkError::new(format!("failed to write Halo2 params: {err}")))?;
            Ok(params)
        }
    }

    fn build_pk(
        config: &ZkConfig,
        params: &Params<G1Affine>,
    ) -> Result<ProvingKey<G1Affine>, ZkError> {
        let circuit = StatementEqualityCircuit::default();
        let vk = keygen_vk(params, &circuit).map_err(|err| {
            ZkError::new(format!(
                "failed building Halo2 VK for {}: {err}",
                config.circuit_id
            ))
        })?;
        keygen_pk(params, vk, &circuit).map_err(|err| {
            ZkError::new(format!(
                "failed building Halo2 PK for {}: {err}",
                config.circuit_id
            ))
        })
    }

    fn persist_key_material(
        config: &ZkConfig,
        k: u32,
        _params: &Params<G1Affine>,
        pk: &ProvingKey<G1Affine>,
    ) -> Result<(), ZkError> {
        Self::ensure_parent(&config.verifying_key_path)?;
        Self::ensure_parent(&config.proving_key_path)?;
        let pinned = format!("{:?}", pk.get_vk().pinned());
        let vk_payload = json!({
            "circuit_id": config.circuit_id,
            "proof_system": config.proof_system,
            "curve": config.curve,
            "soundness": config.soundness,
            "params_bits": k,
            "pinned": pinned,
        });
        let vk_bytes = serde_json::to_vec_pretty(&vk_payload).map_err(|err| {
            ZkError::new(format!(
                "failed to serialize verifying key metadata for {}: {err}",
                config.circuit_id
            ))
        })?;
        fs::write(&config.verifying_key_path, vk_bytes).map_err(|err| {
            ZkError::new(format!(
                "failed to write verifying key metadata {}: {err}",
                config.verifying_key_path.display()
            ))
        })?;

        let digest = blake3::hash(
            serde_json::to_string(&vk_payload)
                .unwrap_or_default()
                .as_bytes(),
        );
        let pk_payload = json!({
            "circuit_id": config.circuit_id,
            "proof_system": config.proof_system,
            "curve": config.curve,
            "vk_digest": digest.to_hex().to_string(),
            "params_path": config.params_path,
        });
        let pk_bytes = serde_json::to_vec_pretty(&pk_payload).map_err(|err| {
            ZkError::new(format!(
                "failed to serialize proving key metadata for {}: {err}",
                config.circuit_id
            ))
        })?;
        fs::write(&config.proving_key_path, pk_bytes).map_err(|err| {
            ZkError::new(format!(
                "failed to write proving key metadata {}: {err}",
                config.proving_key_path.display()
            ))
        })
    }
}

impl ZkProver for Halo2ZkProver {
    fn prove(&self, statement: &ZkStatement) -> Result<ZkProof, ZkError> {
        if statement.public_inputs.is_empty() {
            return Err(ZkError::new("public inputs missing"));
        }
        let public_storage = vec![Self::scalar_for(statement)];
        let instance_column = vec![public_storage.as_slice()];
        let instance_refs = vec![instance_column.as_slice()];
        let circuit = StatementEqualityCircuit {
            statement_hash: Some(public_storage[0]),
        };
        let mut transcript = Blake2bWrite::<_, G1Affine, Challenge255<_>>::init(vec![]);
        create_proof(
            self.params.as_ref(),
            self.pk.as_ref(),
            &[circuit],
            instance_refs.as_slice(),
            OsRng,
            &mut transcript,
        )
        .map_err(|err| ZkError::new(format!("Halo2 prove failure: {err}")))?;
        let proof_bytes = transcript.finalize();
        Ok(ZkProof {
            proof_system: self.config.proof_system.clone(),
            curve: self.config.curve.clone(),
            proof_bytes,
            statement_hash: statement.hash(),
            soundness_error: self.config.soundness,
        })
    }

    fn verify(&self, proof: &ZkProof, statement: &ZkStatement) -> Result<(), ZkError> {
        if proof.statement_hash != statement.hash() {
            return Err(ZkError::new("halo2 statement hash mismatch"));
        }
        let public_storage = vec![Self::scalar_for(statement)];
        let instance_column = vec![public_storage.as_slice()];
        let instance_refs = vec![instance_column.as_slice()];
        let mut transcript =
            Blake2bRead::<_, G1Affine, Challenge255<_>>::init(&proof.proof_bytes[..]);
        let strategy = SingleVerifier::new(self.params.as_ref());
        verify_proof(
            self.params.as_ref(),
            self.pk.get_vk(),
            strategy,
            instance_refs.as_slice(),
            &mut transcript,
        )
        .map_err(|err| ZkError::new(format!("Halo2 verify failure: {err}")))
    }
}

#[derive(Clone, Default)]
struct StatementEqualityCircuit {
    statement_hash: Option<Fr>,
}

#[derive(Clone, Debug)]
struct StatementEqualityConfig {
    advice: Column<Advice>,
    instance: Column<Instance>,
}

impl Circuit<Fr> for StatementEqualityCircuit {
    type Config = StatementEqualityConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self {
            statement_hash: None,
        }
    }

    fn configure(meta: &mut ConstraintSystem<Fr>) -> Self::Config {
        let advice = meta.advice_column();
        let instance = meta.instance_column();
        meta.enable_equality(advice);
        meta.enable_equality(instance);
        meta.create_gate("hash equals instance", |meta| {
            let witness = meta.query_advice(advice, Rotation::cur());
            let instance_val = meta.query_instance(instance, Rotation::cur());
            vec![witness - instance_val]
        });
        StatementEqualityConfig { advice, instance }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<Fr>,
    ) -> Result<(), Halo2Error> {
        let assigned = layouter.assign_region(
            || "load statement hash",
            |mut region| {
                region.assign_advice(
                    || "statement hash",
                    config.advice,
                    0,
                    || {
                        self.statement_hash
                            .map(Value::known)
                            .unwrap_or_else(Value::unknown)
                    },
                )
            },
        )?;
        layouter.constrain_instance(assigned.cell(), config.instance, 0)?;
        Ok(())
    }
}
