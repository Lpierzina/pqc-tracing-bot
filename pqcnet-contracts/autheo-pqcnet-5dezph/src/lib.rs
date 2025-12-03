//! 5D-EZPH (Five-Dimensional Entangled Zero-Knowledge Privacy Hypergraph)
//! orchestrator. This crate layers chaos-infused manifold synthesis, CKKS-style
//! homomorphic summaries, and ZK proof metadata on top of the 5D-QEH module so
//! PrivacyNet overlays can anchor entangled privacy states in Chronosync.

pub mod chaos;
pub mod config;
pub mod error;
pub mod fhe;
pub mod manifold;
pub mod pipeline;
pub mod privacy;
pub mod zk;

pub use chaos::{ChaosEngine, ChaosVector, LorenzChuaChaos};
pub use config::EzphConfig;
pub use error::{EzphError, EzphResult};
pub use fhe::{FheCiphertext, FheEvaluator, MockCkksEvaluator};
pub use manifold::{DimensionKind, DimensionProjection, EzphManifoldState};
pub use pipeline::{DefaultEzphPipeline, EzphOutcome, EzphPipeline, EzphRequest};
pub use privacy::EzphPrivacyReport;
pub use zk::{MockCircomProver, ZkProof, ZkProver, ZkStatement};
