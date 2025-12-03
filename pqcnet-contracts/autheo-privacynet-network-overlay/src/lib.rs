//! PrivacyNet Network Overlay – JSON-RPC + Grapplang shell for the
//! Autheo PrivacyNet engine. This crate wraps the Autheo core pipeline with
//! production-oriented networking, QSTP transport sealing, and Zer0veil shell
//! bindings so control planes can embed PrivacyNet as a self-contained overlay.

pub mod config;
pub mod error;
pub mod grapplang;
pub mod overlay;
pub mod rpc;
pub mod transport;

pub use config::OverlayNodeConfig;
pub use error::{OverlayError, OverlayResult};
pub use grapplang::parse_statement;
pub use overlay::OverlayNode;
pub use rpc::{
    CreateVertexParams, CreateVertexResult, EntangledProofResult, OverlayRpc, ProofTelemetry,
    ProveAttributeParams, QtaidTokenizeParams, QtaidTokenizeResult, RevokeCredentialParams,
    RevokeCredentialResult, VerifyProofParams, VerifyProofResult,
};
pub use transport::{loopback_gateways, OverlayFrame, QstpGateway};
