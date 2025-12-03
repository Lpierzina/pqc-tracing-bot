//! Autheo PrivacyNet – production-grade integration of differential privacy,
//! CKKS/BFV-style FHE, zk-SNARK/STARK proofs, and the five-dimensional EZPH
//! manifold. This crate exposes a cohesive orchestration engine that threads
//! every primitive through Autheo's PQCNet substrates so privacy guarantees
//! compose across dimensions, network layers, and tenant boundaries.

pub mod api;
pub mod budget;
pub mod chaos;
pub mod config;
pub mod dp;
pub mod errors;
pub mod fhe;
pub mod icosuple;
pub mod pipeline;

pub use api::{ChaosPerturbRequest, ChaosPerturbResponse, PrivacyNetRpc, RpcEnvelope};
pub use budget::{BudgetClaim, BudgetLedgerSnapshot, PrivacyBudgetLedger};
pub use chaos::{ChaosOracle, ChaosOracleConfig, ChaosSample};
pub use config::{ApiConfig, PrivacyNetConfig};
pub use dp::{
    Blake3Hash, DifferentialPrivacyEngine, DpEngineConfig, DpMechanism, DpQuery, DpSample,
};
pub use errors::{PrivacyNetError, PrivacyNetResult};
pub use fhe::{FheCircuitIntent, FheLayer, FheLayerConfig, HomomorphicJob};
pub use icosuple::{CkksContextMetadata, PrivacyEnhancedIcosuple};
pub use pipeline::{DpQueryResult, PrivacyNetEngine, PrivacyNetRequest, PrivacyNetResponse};
