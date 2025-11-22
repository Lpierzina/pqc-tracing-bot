//! Deterministic, in-memory networking façade that mimics message passing
//! semantics required by the pqcnet binaries. The implementation focuses on
//! debuggability and deterministic tests rather than real sockets.
//!
//! # Quickstart
//! ```
//! use pqcnet_networking::{NetworkClient, NetworkingConfig};
//!
//! let config = NetworkingConfig::sample("127.0.0.1:7100");
//! let client = NetworkClient::from_config("node-a", config);
//! let receipts = client.broadcast("ping");
//! assert_eq!(receipts.len(), 2);
//! assert_eq!(client.drain_inflight().len(), 2);
//! ```

pub mod control_plane;
pub mod pubsub;
pub mod rpcnet;
pub use pqcnet_qs_dag as qs_dag;
pub use pqcnet_qs_dag::{DagError, QsDag, StateDiff, StateOp, StateSnapshot};

pub use control_plane::{
    ControlCommand, ControlEvent, ControlPlane, ControlPlaneConfig, ControlPlaneError,
    NodeAnnouncement,
};
pub use pubsub::{
    ContentTopic, PubSubEnvelope, PubSubMessage, PubSubRouter, PublishReceipt, Subscription, Topic,
};
pub use rpcnet::{
    AnchorEdgeEndpoint, MsgOpenTunnel, MsgOpenTunnelResponse, RpcNetError, RpcNetRouter,
    SessionKeyMaterial,
};

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::Instant,
};
use thiserror::Error;

#[cfg(any(
    all(feature = "dev", feature = "test"),
    all(feature = "dev", feature = "prod"),
    all(feature = "test", feature = "prod")
))]
compile_error!(
    "Only one of the `dev`, `test`, or `prod` features may be enabled for pqcnet-networking."
);

#[cfg(feature = "dev")]
const DEFAULT_RETRY_ATTEMPTS: u8 = 1;
#[cfg(feature = "test")]
const DEFAULT_RETRY_ATTEMPTS: u8 = 3;
#[cfg(feature = "prod")]
const DEFAULT_RETRY_ATTEMPTS: u8 = 5;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PeerConfig {
    pub id: String,
    pub address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct NetworkingConfig {
    /// Local bind address (host:port) for diagnostics.
    pub listen: String,
    /// Static peer set.
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    /// Maximum in-flight messages.
    #[serde(default = "default_max_inflight")]
    pub max_inflight: u16,
    /// Retry attempts automatically derived from feature flags.
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u8,
    /// Jitter range used to model latency (ms).
    #[serde(default = "default_jitter_ms")]
    pub jitter_ms: u64,
}

fn default_max_inflight() -> u16 {
    64
}

fn default_retry_attempts() -> u8 {
    DEFAULT_RETRY_ATTEMPTS
}

fn default_jitter_ms() -> u64 {
    50
}

impl NetworkingConfig {
    pub fn sample(listen: &str) -> Self {
        Self {
            listen: listen.to_owned(),
            peers: vec![
                PeerConfig {
                    id: "peer-a".into(),
                    address: "127.0.0.1:7001".into(),
                },
                PeerConfig {
                    id: "peer-b".into(),
                    address: "127.0.0.1:7002".into(),
                },
            ],
            max_inflight: default_max_inflight(),
            retry_attempts: default_retry_attempts(),
            jitter_ms: default_jitter_ms(),
        }
    }
}

#[derive(Debug, Error)]
pub enum NetworkingError {
    #[error("unknown peer: {0}")]
    UnknownPeer(String),
    #[error("in-flight limit {limit} reached (currently {current})")]
    InFlightLimit { limit: u16, current: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub payload: Vec<u8>,
    pub sent_at: Instant,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} -> {} ({} bytes)",
            self.from,
            self.to,
            self.payload.len()
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryReceipt {
    pub peer_id: String,
    pub latency_ms: u64,
}

#[derive(Clone)]
pub struct NetworkClient {
    node_id: String,
    config: NetworkingConfig,
    peers: HashMap<String, PeerConfig>,
    inflight: Arc<Mutex<Vec<Message>>>,
}

impl NetworkClient {
    pub fn from_config(node_id: &str, config: NetworkingConfig) -> Self {
        let peers = config
            .peers
            .iter()
            .map(|peer| (peer.id.clone(), peer.clone()))
            .collect();
        Self {
            node_id: node_id.to_owned(),
            config,
            peers,
            inflight: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn publish(
        &self,
        target_peer: &str,
        payload: impl Into<Vec<u8>>,
    ) -> Result<DeliveryReceipt, NetworkingError> {
        let mut inflight = self.inflight.lock().unwrap();
        if inflight.len() >= self.config.max_inflight as usize {
            return Err(NetworkingError::InFlightLimit {
                limit: self.config.max_inflight,
                current: inflight.len(),
            });
        }
        let peer = self
            .peers
            .get(target_peer)
            .ok_or_else(|| NetworkingError::UnknownPeer(target_peer.to_owned()))?;
        let latency_ms = simulate_latency(self.config.jitter_ms);
        inflight.push(Message {
            from: self.node_id.clone(),
            to: peer.id.clone(),
            payload: payload.into(),
            sent_at: Instant::now(),
        });
        Ok(DeliveryReceipt {
            peer_id: peer.id.clone(),
            latency_ms,
        })
    }

    pub fn broadcast(&self, payload: impl AsRef<[u8]>) -> Vec<DeliveryReceipt> {
        let payload = payload.as_ref().to_vec();
        self.peers
            .keys()
            .filter_map(|peer| self.publish(peer, payload.clone()).ok())
            .collect()
    }

    pub fn drain_inflight(&self) -> Vec<Message> {
        let mut guard = self.inflight.lock().unwrap();
        let drained = guard.clone();
        guard.clear();
        drained
    }
}

fn simulate_latency(max_ms: u64) -> u64 {
    if max_ms == 0 {
        return 0;
    }
    rand::thread_rng().gen_range(1..=max_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> NetworkClient {
        NetworkClient::from_config("node-a", NetworkingConfig::sample("127.0.0.1:7000"))
    }

    #[test]
    fn publishes_to_known_peer() {
        let client = client();
        let receipt = client.publish("peer-a", b"hello".as_slice()).unwrap();
        assert_eq!(receipt.peer_id, "peer-a");
        let inflight = client.drain_inflight();
        assert_eq!(inflight.len(), 1);
        assert_eq!(inflight[0].to, "peer-a");
    }

    #[test]
    fn errors_on_unknown_peer() {
        let client = client();
        let err = client.publish("missing", b"hello").unwrap_err();
        assert!(matches!(err, NetworkingError::UnknownPeer(_)));
    }

    #[test]
    fn enforce_inflight_limit() {
        let mut config = NetworkingConfig::sample("127.0.0.1:7000");
        config.max_inflight = 1;
        let client = NetworkClient::from_config("node-a", config);
        client.publish("peer-a", b"a").unwrap();
        let err = client.publish("peer-a", b"b").unwrap_err();
        assert!(matches!(err, NetworkingError::InFlightLimit { .. }));
    }

    #[test]
    fn broadcast_to_all_peers() {
        let client = client();
        let receipts = client.broadcast("ping");
        assert_eq!(receipts.len(), 2);
    }
}
