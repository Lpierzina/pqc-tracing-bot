use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum LayerClass {
    Infrastructure,
    Network,
    Consensus,
    Application,
    Security,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum IcosupleLayer {
    INFRASTRUCTURE_TIER_0,
    INFRASTRUCTURE_TIER_1,
    INFRASTRUCTURE_TIER_2,
    INFRASTRUCTURE_TIER_3,
    NETWORK_TIER_4,
    NETWORK_TIER_5,
    NETWORK_TIER_6,
    NETWORK_TIER_7,
    CONSENSUS_TIER_8,
    CONSENSUS_TIER_9,
    CONSENSUS_TIER_10,
    CONSENSUS_TIER_11,
    APPLICATION_TIER_12,
    APPLICATION_TIER_13,
    APPLICATION_TIER_14,
    APPLICATION_TIER_15,
    SECURITY_TIER_16,
    SECURITY_TIER_17,
    SECURITY_TIER_18,
    SECURITY_TIER_19,
}

impl IcosupleLayer {
    pub fn index(self) -> u8 {
        match self {
            IcosupleLayer::INFRASTRUCTURE_TIER_0 => 0,
            IcosupleLayer::INFRASTRUCTURE_TIER_1 => 1,
            IcosupleLayer::INFRASTRUCTURE_TIER_2 => 2,
            IcosupleLayer::INFRASTRUCTURE_TIER_3 => 3,
            IcosupleLayer::NETWORK_TIER_4 => 4,
            IcosupleLayer::NETWORK_TIER_5 => 5,
            IcosupleLayer::NETWORK_TIER_6 => 6,
            IcosupleLayer::NETWORK_TIER_7 => 7,
            IcosupleLayer::CONSENSUS_TIER_8 => 8,
            IcosupleLayer::CONSENSUS_TIER_9 => 9,
            IcosupleLayer::CONSENSUS_TIER_10 => 10,
            IcosupleLayer::CONSENSUS_TIER_11 => 11,
            IcosupleLayer::APPLICATION_TIER_12 => 12,
            IcosupleLayer::APPLICATION_TIER_13 => 13,
            IcosupleLayer::APPLICATION_TIER_14 => 14,
            IcosupleLayer::APPLICATION_TIER_15 => 15,
            IcosupleLayer::SECURITY_TIER_16 => 16,
            IcosupleLayer::SECURITY_TIER_17 => 17,
            IcosupleLayer::SECURITY_TIER_18 => 18,
            IcosupleLayer::SECURITY_TIER_19 => 19,
        }
    }

    pub fn class(self) -> LayerClass {
        match self {
            IcosupleLayer::INFRASTRUCTURE_TIER_0
            | IcosupleLayer::INFRASTRUCTURE_TIER_1
            | IcosupleLayer::INFRASTRUCTURE_TIER_2
            | IcosupleLayer::INFRASTRUCTURE_TIER_3 => LayerClass::Infrastructure,
            IcosupleLayer::NETWORK_TIER_4
            | IcosupleLayer::NETWORK_TIER_5
            | IcosupleLayer::NETWORK_TIER_6
            | IcosupleLayer::NETWORK_TIER_7 => LayerClass::Network,
            IcosupleLayer::CONSENSUS_TIER_8
            | IcosupleLayer::CONSENSUS_TIER_9
            | IcosupleLayer::CONSENSUS_TIER_10
            | IcosupleLayer::CONSENSUS_TIER_11 => LayerClass::Consensus,
            IcosupleLayer::APPLICATION_TIER_12
            | IcosupleLayer::APPLICATION_TIER_13
            | IcosupleLayer::APPLICATION_TIER_14
            | IcosupleLayer::APPLICATION_TIER_15 => LayerClass::Application,
            IcosupleLayer::SECURITY_TIER_16
            | IcosupleLayer::SECURITY_TIER_17
            | IcosupleLayer::SECURITY_TIER_18
            | IcosupleLayer::SECURITY_TIER_19 => LayerClass::Security,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            IcosupleLayer::INFRASTRUCTURE_TIER_0 => "quantum hardware + entropy nodes",
            IcosupleLayer::INFRASTRUCTURE_TIER_1 => "WASM runtimes + enclave hosts",
            IcosupleLayer::INFRASTRUCTURE_TIER_2 => "decentralized storage backplanes",
            IcosupleLayer::INFRASTRUCTURE_TIER_3 => "p2p transport (libp2p, QUIC)",
            IcosupleLayer::NETWORK_TIER_4 => "bootnodes & discovery",
            IcosupleLayer::NETWORK_TIER_5 => "low-latency gossip fabric",
            IcosupleLayer::NETWORK_TIER_6 => "state sync & DAG shadowing",
            IcosupleLayer::NETWORK_TIER_7 => "HVP scheduling + load balancers",
            IcosupleLayer::CONSENSUS_TIER_8 => "tuple validation & PQC checks",
            IcosupleLayer::CONSENSUS_TIER_9 => "temporal weighting + Lamport clocks",
            IcosupleLayer::CONSENSUS_TIER_10 => "HVP coordination & MPC aggregation",
            IcosupleLayer::CONSENSUS_TIER_11 => "asynchronous BFT + finality gadgets",
            IcosupleLayer::APPLICATION_TIER_12 => "Grapplang runtime adapters",
            IcosupleLayer::APPLICATION_TIER_13 => "DID + API gateways",
            IcosupleLayer::APPLICATION_TIER_14 => "SDK toolchains + wasm marketplace",
            IcosupleLayer::APPLICATION_TIER_15 => "front-end + DePIN UX surfaces",
            IcosupleLayer::SECURITY_TIER_16 => "PQC suite orchestration",
            IcosupleLayer::SECURITY_TIER_17 => "QRNG beacons & entropy routing",
            IcosupleLayer::SECURITY_TIER_18 => "QIES enclaves + FHE/MPC",
            IcosupleLayer::SECURITY_TIER_19 => "Audit logging + compliance bridges",
        }
    }

    pub fn try_from_index(idx: u8) -> Option<Self> {
        match idx {
            0 => Some(IcosupleLayer::INFRASTRUCTURE_TIER_0),
            1 => Some(IcosupleLayer::INFRASTRUCTURE_TIER_1),
            2 => Some(IcosupleLayer::INFRASTRUCTURE_TIER_2),
            3 => Some(IcosupleLayer::INFRASTRUCTURE_TIER_3),
            4 => Some(IcosupleLayer::NETWORK_TIER_4),
            5 => Some(IcosupleLayer::NETWORK_TIER_5),
            6 => Some(IcosupleLayer::NETWORK_TIER_6),
            7 => Some(IcosupleLayer::NETWORK_TIER_7),
            8 => Some(IcosupleLayer::CONSENSUS_TIER_8),
            9 => Some(IcosupleLayer::CONSENSUS_TIER_9),
            10 => Some(IcosupleLayer::CONSENSUS_TIER_10),
            11 => Some(IcosupleLayer::CONSENSUS_TIER_11),
            12 => Some(IcosupleLayer::APPLICATION_TIER_12),
            13 => Some(IcosupleLayer::APPLICATION_TIER_13),
            14 => Some(IcosupleLayer::APPLICATION_TIER_14),
            15 => Some(IcosupleLayer::APPLICATION_TIER_15),
            16 => Some(IcosupleLayer::SECURITY_TIER_16),
            17 => Some(IcosupleLayer::SECURITY_TIER_17),
            18 => Some(IcosupleLayer::SECURITY_TIER_18),
            19 => Some(IcosupleLayer::SECURITY_TIER_19),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_metadata_is_stable() {
        let layer = IcosupleLayer::APPLICATION_TIER_13;
        assert_eq!(layer.index(), 13);
        assert_eq!(layer.class(), LayerClass::Application);
        assert!(layer.description().contains("DID"));
        assert_eq!(IcosupleLayer::try_from_index(13).unwrap(), layer);
    }
}
