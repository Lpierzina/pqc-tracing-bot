//! Autheo DW3B Mesh – production-grade engine that mirrors the Autheo
//! PrivacyNet stack while adding DW3B-specific anonymity overlays, Bloom
//! filters, chaos perturbations, and mixnet route planning. The crate pairs the
//! `autheo-privacynet` pipeline with deterministic stubs for the privacy
//! primitives listed in the PrivacyNet + DW3B Mesh specification so hosts can
//! begin integration before the audited OpenFHE/Halo2/RISC Zero bindings land.

pub mod bloom;
pub mod chaos;
pub mod compression;
pub mod config;
pub mod engine;
pub mod entropy;
pub mod mesh;
pub mod noise;
pub mod types;

pub use config::{Dw3bMeshConfig, MeshNodeWeights, PrivacyPrimitiveConfig, QuantumEntropyConfig};
pub use engine::{
    Dw3bMeshEngine, MeshAnonymizeRequest, MeshAnonymizeResponse, QtaidProof, QtaidProveRequest,
};
pub use mesh::{AnonymityMetrics, AnonymityProof, MeshNodeKind, MeshRoutePlan, MeshRouteStats};
pub use types::{MeshError, MeshResult};
