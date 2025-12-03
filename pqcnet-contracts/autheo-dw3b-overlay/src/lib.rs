//! Autheo DW3B Overlay – wraps the DW3B mesh engine with JSON-RPC 2.0,
//! Grapplang, networking, telemetry, and QSTP transports so privacy workloads
//! can run as a standalone overlay facade.

pub mod config;
pub mod error;
pub mod grapplang;
pub mod overlay;
pub mod rpc;
pub mod transport;

pub use config::Dw3bOverlayConfig;
pub use error::{OverlayError, OverlayResult};
pub use grapplang::parse_statement;
pub use overlay::Dw3bOverlayNode;
pub use rpc::{
    AnonymizeQueryParams, Dw3bOverlayRpc, EntropyRequestParams, ObfuscateRouteParams,
    OverlayRpcEnvelope, PolicyConfigureParams, QtaidProveParams, SyncStateParams,
};
pub use transport::{loopback_gateways, Dw3bGateway, OverlayFrame};
