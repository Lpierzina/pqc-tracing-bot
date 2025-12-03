use autheo_pqcnet_5dezph::{
    pipeline::{DefaultEzphPipeline, EzphRequest},
    EzphPrivacyReport,
};
use autheo_pqcnet_5dqeh::{HypergraphModule, PqcBinding, TemporalWeightModel, VertexId};
use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::{
    budget::{BudgetClaim, PrivacyBudgetLedger},
    chaos::{ChaosOracle, ChaosSample},
    config::{ApiConfig, PrivacyNetConfig},
    dp::{Blake3Hash, DifferentialPrivacyEngine, DpQuery, DpSample},
    errors::{PrivacyNetError, PrivacyNetResult},
    fhe::{FheLayer, HomomorphicJob},
    icosuple::PrivacyEnhancedIcosuple,
};

pub struct PrivacyNetEngine {
    config: PrivacyNetConfig,
    pipeline: DefaultEzphPipeline,
    module: HypergraphModule,
    budgets: PrivacyBudgetLedger,
    chaos: ChaosOracle,
    dp_engine: DifferentialPrivacyEngine,
    fhe_layer: FheLayer,
}

impl PrivacyNetEngine {
    pub fn new(config: PrivacyNetConfig) -> Self {
        let dp_seed = blake3::hash(b"privacynet/dp");
        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(dp_seed.as_bytes());
        let pipeline = DefaultEzphPipeline::new(config.ezph.clone());
        let module = HypergraphModule::new(config.ezph.qeh.clone(), TemporalWeightModel::default());
        Self {
            dp_engine: DifferentialPrivacyEngine::new(config.dp.clone(), seed_bytes),
            fhe_layer: FheLayer::new(config.fhe.clone()),
            budgets: PrivacyBudgetLedger::new(config.budget.clone()),
            chaos: ChaosOracle::new(config.chaos.clone()),
            pipeline,
            module,
            config,
        }
    }

    pub fn handle_request(
        &mut self,
        request: PrivacyNetRequest,
    ) -> PrivacyNetResult<PrivacyNetResponse> {
        self.validate(&request)?;
        let ezph_request = request.to_ezph_request();
        let seed = derive_seed(&ezph_request);
        let chaos_sample = self.chaos.sample(&seed);
        let budget_claim = self.budgets.claim(request.session_id, &request.dp_query)?;
        let dp_sample = self.dp_engine.execute(&request.dp_query, &chaos_sample)?;
        let fhe_ct = self
            .fhe_layer
            .execute(HomomorphicJob::Slots(request.fhe_slots.clone()))?;
        let outcome = self
            .pipeline
            .entangle_and_anchor(&mut self.module, ezph_request)?;
        let enhanced = PrivacyEnhancedIcosuple::assemble(
            &outcome,
            &dp_sample,
            &chaos_sample,
            &budget_claim,
            self.remaining_ops(),
        );
        let dp_result = DpQueryResult {
            sample: dp_sample,
            encrypted_response: fhe_ct,
            zk_proof_digest: outcome.zk_proof.statement_hash,
            budget_claim: budget_claim.clone(),
        };
        self.budgets.settle(request.session_id);
        Ok(PrivacyNetResponse {
            request_id: request.dp_query.query_id,
            tenant_id: request.tenant_id,
            chaos_sample,
            dp_result,
            enhanced_icosuple: enhanced,
            privacy_report: outcome.privacy,
        })
    }

    fn validate(&self, request: &PrivacyNetRequest) -> PrivacyNetResult<()> {
        let ApiConfig {
            max_payload_bytes,
            max_public_inputs,
        } = self.config.api;
        if request.payload_bytes > max_payload_bytes {
            return Err(PrivacyNetError::PayloadTooLarge {
                bytes: request.payload_bytes,
                limit: max_payload_bytes,
            });
        }
        if request.public_inputs.len() > max_public_inputs {
            return Err(PrivacyNetError::InvalidRpc("too many public inputs"));
        }
        Ok(())
    }

    fn remaining_ops(&self) -> u32 {
        self.config
            .fhe
            .max_multiplications
            .saturating_sub(self.config.fhe.bootstrap_period)
    }
}

#[derive(Clone, Debug)]
pub struct PrivacyNetRequest {
    pub session_id: u64,
    pub tenant_id: String,
    pub label: String,
    pub chain_epoch: u64,
    pub dp_query: DpQuery,
    pub fhe_slots: Vec<f64>,
    pub parents: Vec<VertexId>,
    pub payload_bytes: usize,
    pub lamport: u64,
    pub contribution_score: f64,
    pub ann_similarity: f32,
    pub qrng_entropy_bits: u16,
    pub zk_claim: String,
    pub public_inputs: Vec<String>,
}

impl PrivacyNetRequest {
    fn tuple_commitment(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(self.label.as_bytes());
        hasher.update(&self.lamport.to_le_bytes());
        hasher.update(&self.session_id.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }

    fn pqc_binding(&self) -> PqcBinding {
        PqcBinding::simulated(&self.tenant_id)
    }

    fn autheo_identity(&self) -> String {
        format!("did:autheo:{}:{}", self.tenant_id, self.session_id)
    }

    pub fn to_ezph_request(&self) -> EzphRequest {
        EzphRequest {
            label: self.label.clone(),
            chain_epoch: self.chain_epoch,
            parents: self.parents.clone(),
            payload_bytes: self.payload_bytes,
            lamport: self.lamport,
            contribution_score: self.contribution_score,
            ann_similarity: self.ann_similarity,
            qrng_entropy_bits: self.qrng_entropy_bits,
            pqc_binding: self.pqc_binding(),
            autheo_identity: self.autheo_identity(),
            tuple_commitment: self.tuple_commitment(),
            zk_claim: self.zk_claim.clone(),
            public_inputs: self.public_inputs.clone(),
            fhe_slots: self.fhe_slots.clone(),
            parent_coherence_hint: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DpQueryResult {
    pub sample: DpSample,
    pub encrypted_response: autheo_pqcnet_5dezph::fhe::FheCiphertext,
    pub zk_proof_digest: [u8; 32],
    pub budget_claim: BudgetClaim,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivacyNetResponse {
    pub request_id: Blake3Hash,
    pub tenant_id: String,
    pub chaos_sample: ChaosSample,
    pub dp_result: DpQueryResult,
    pub enhanced_icosuple: PrivacyEnhancedIcosuple,
    pub privacy_report: EzphPrivacyReport,
}

fn derive_seed(request: &EzphRequest) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(request.label.as_bytes());
    hasher.update(&request.lamport.to_le_bytes());
    hasher.update(&request.chain_epoch.to_le_bytes());
    hasher.update(&request.tuple_commitment);
    for slot in &request.fhe_slots {
        hasher.update(&slot.to_le_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    out
}
