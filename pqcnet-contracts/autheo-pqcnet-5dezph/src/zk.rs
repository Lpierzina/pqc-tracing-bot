use std::sync::Arc;

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
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::ZkConfig;

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
        let k = Self::derive_k(&config);
        let params = Params::<G1Affine>::new(k);
        let circuit = StatementEqualityCircuit::default();
        let vk = keygen_vk(&params, &circuit).map_err(|err| {
            ZkError::new(format!(
                "failed building Halo2 VK for {}: {err}",
                config.circuit_id
            ))
        })?;
        let pk = keygen_pk(&params, vk, &circuit).map_err(|err| {
            ZkError::new(format!(
                "failed building Halo2 PK for {}: {err}",
                config.circuit_id
            ))
        })?;
        Ok(Self {
            config,
            params: Arc::new(params),
            pk: Arc::new(pk),
        })
    }

    fn derive_k(config: &ZkConfig) -> u32 {
        let security_bits = (-config.soundness.log2()).ceil().max(1.0) as u32;
        let extra = (security_bits / 64).min(6);
        18 + extra
    }

    fn scalar_for(statement: &ZkStatement) -> Fr {
        let hash = statement.hash();
        Fr::from_bytes(&hash).unwrap_or_else(|| Fr::zero())
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
        let value = self.statement_hash.ok_or(Halo2Error::Synthesis)?;
        let assigned = layouter.assign_region(
            || "load statement hash",
            |mut region| {
                region.assign_advice(
                    || "statement hash",
                    config.advice,
                    0,
                    || Value::known(value),
                )
            },
        )?;
        layouter.constrain_instance(assigned.cell(), config.instance, 0)?;
        Ok(())
    }
}
