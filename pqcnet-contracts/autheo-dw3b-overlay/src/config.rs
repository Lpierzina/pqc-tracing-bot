use autheo_dw3b_mesh::Dw3bMeshConfig;
use pqcnet_crypto::{KemAdvertisement, SignatureRedundancy, SignatureScheme};
use pqcnet_networking::NetworkingConfig;
use pqcnet_qstp::{MeshPeerId, MeshQosClass, MeshRoutePlan};
use pqcnet_telemetry::TelemetryConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dw3bOverlayConfig {
    pub node_id: String,
    pub mesh: Dw3bMeshConfig,
    pub networking: NetworkingConfig,
    pub rpc: RpcSurfaceConfig,
    pub qstp: QstpConfig,
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub pqc: PqcOverlayConfig,
}

impl Dw3bOverlayConfig {
    pub fn demo() -> Self {
        let mut networking = NetworkingConfig::sample("127.0.0.1:8513");
        networking.peers.clear();
        Self {
            node_id: "dw3b-overlay-alpha".into(),
            mesh: Dw3bMeshConfig::production(),
            networking,
            rpc: RpcSurfaceConfig::default(),
            qstp: QstpConfig::default(),
            telemetry: TelemetryConfig::sample("http://127.0.0.1:4318"),
            pqc: PqcOverlayConfig::default(),
        }
    }
}

impl Default for Dw3bOverlayConfig {
    fn default() -> Self {
        Self::demo()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcSurfaceConfig {
    pub epsilon_cap: f64,
    pub bloom_fp_max: f64,
    pub max_layers: u32,
    pub stake_threshold: u64,
    pub chsh_target: f32,
    pub qtaid_bits_per_snp: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PqcOverlayConfig {
    #[serde(default = "default_advertised_kems")]
    pub advertised_kems: Vec<KemAdvertisement>,
    #[serde(default)]
    pub signature_redundancy: Option<SignatureRedundancy>,
}

impl Default for PqcOverlayConfig {
    fn default() -> Self {
        Self {
            advertised_kems: default_advertised_kems(),
            signature_redundancy: Some(SignatureRedundancy {
                primary: SignatureScheme::Dilithium,
                backup: SignatureScheme::Sphincs,
                require_dual: true,
            }),
        }
    }
}

fn default_advertised_kems() -> Vec<KemAdvertisement> {
    vec![
        KemAdvertisement::sample_primary(),
        KemAdvertisement::sample_backup(),
    ]
}

impl Default for RpcSurfaceConfig {
    fn default() -> Self {
        Self {
            epsilon_cap: 1e-5,
            bloom_fp_max: 0.01,
            max_layers: 7,
            stake_threshold: 25_000,
            chsh_target: 2.87,
            qtaid_bits_per_snp: 4,
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
            topic: "dw3b/jsonrpc".into(),
            qos: OverlayQosClass::LowLatency,
            epoch: 1,
            local_peer_label: "dw3b-shell".into(),
            remote_peer_label: "dw3b-engine".into(),
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
