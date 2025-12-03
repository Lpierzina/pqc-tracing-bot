use autheo_privacynet::config::PrivacyNetConfig;
use pqcnet_networking::NetworkingConfig;
use pqcnet_qstp::{MeshPeerId, MeshQosClass, MeshRoutePlan};
use pqcnet_telemetry::TelemetryConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayNodeConfig {
    pub node_id: String,
    pub privacynet: PrivacyNetConfig,
    pub networking: NetworkingConfig,
    pub rpc: RpcSurfaceConfig,
    pub qstp: QstpConfig,
    pub telemetry: TelemetryConfig,
}

impl Default for OverlayNodeConfig {
    fn default() -> Self {
        Self {
            node_id: "overlay-node-alpha".into(),
            privacynet: PrivacyNetConfig::default(),
            networking: NetworkingConfig::sample("127.0.0.1:7415"),
            rpc: RpcSurfaceConfig::default(),
            qstp: QstpConfig::default(),
            telemetry: TelemetryConfig::sample("http://127.0.0.1:4318"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcSurfaceConfig {
    pub manifold_hint: String,
    pub max_sessions: u32,
    pub max_pending: usize,
    pub chsh_target: f32,
    pub epsilon_cap: f64,
    pub qtaid_bits_per_snp: u8,
}

impl Default for RpcSurfaceConfig {
    fn default() -> Self {
        Self {
            manifold_hint: "5d-ezph".into(),
            max_sessions: 8192,
            max_pending: 4096,
            chsh_target: 2.87,
            epsilon_cap: 1e-4,
            qtaid_bits_per_snp: 3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QstpConfig {
    pub topic: String,
    pub qos: OverlayQosClass,
    pub epoch: u64,
    pub local_peer_label: String,
    pub remote_peer_label: String,
    #[serde(default)]
    pub hops: Vec<String>,
}

impl QstpConfig {
    pub fn route_plan(&self) -> MeshRoutePlan {
        MeshRoutePlan {
            topic: self.topic.clone(),
            hops: self
                .hops
                .iter()
                .map(|label| MeshPeerId::derive(label))
                .collect(),
            qos: self.qos.into(),
            epoch: self.epoch,
        }
    }

    pub fn local_peer(&self) -> MeshPeerId {
        MeshPeerId::derive(&self.local_peer_label)
    }

    pub fn remote_peer(&self) -> MeshPeerId {
        MeshPeerId::derive(&self.remote_peer_label)
    }
}

impl Default for QstpConfig {
    fn default() -> Self {
        Self {
            topic: "waku/privacynet/jsonrpc".into(),
            qos: OverlayQosClass::LowLatency,
            epoch: 1,
            local_peer_label: "overlay-shell".into(),
            remote_peer_label: "overlay-engine".into(),
            hops: vec!["mesh-alpha".into(), "mesh-beta".into()],
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayQosClass {
    Gossip,
    LowLatency,
    Control,
}

impl From<OverlayQosClass> for MeshQosClass {
    fn from(value: OverlayQosClass) -> Self {
        match value {
            OverlayQosClass::Gossip => MeshQosClass::Gossip,
            OverlayQosClass::LowLatency => MeshQosClass::LowLatency,
            OverlayQosClass::Control => MeshQosClass::Control,
        }
    }
}
