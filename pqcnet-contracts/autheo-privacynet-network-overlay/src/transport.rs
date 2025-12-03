use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use autheo_pqc_core::error::PqcResult;
use pqcnet_qstp::{
    establish_runtime_tunnel, hydrate_remote_tunnel, InMemoryTupleChain, MeshTransport, QstpFrame,
    QstpTunnel, TunnelRole,
};
use serde::{Deserialize, Serialize};

use crate::{config::QstpConfig, error::OverlayResult};

const JSONRPC_AAD: &[u8] = b"privacynet-jsonrpc";

pub struct QstpGateway<T: MeshTransport> {
    tunnel: QstpTunnel,
    transport: T,
    aad: Vec<u8>,
}

impl<T: MeshTransport> QstpGateway<T> {
    pub fn new(tunnel: QstpTunnel, transport: T) -> Self {
        Self {
            tunnel,
            transport,
            aad: JSONRPC_AAD.to_vec(),
        }
    }

    pub fn seal_json<V: Serialize>(&mut self, value: &V) -> OverlayResult<()> {
        let payload = serde_json::to_vec(value)?;
        let frame = self.tunnel.seal(&payload, &self.aad)?;
        self.transport.publish(frame)?;
        Ok(())
    }

    pub fn try_recv_json(&mut self) -> OverlayResult<Option<serde_json::Value>> {
        if let Some(frame) = self.transport.try_recv(&self.tunnel.route().topic) {
            let bytes = self.tunnel.open(&frame, &self.aad)?;
            let value = serde_json::from_slice(&bytes)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    pub fn metadata(&self) -> &pqcnet_qstp::QstpTunnelMetadata {
        self.tunnel.metadata()
    }
}

#[derive(Clone)]
pub struct LoopbackMesh {
    inbound: Arc<Mutex<VecDeque<QstpFrame>>>,
    outbound: Arc<Mutex<VecDeque<QstpFrame>>>,
}

impl LoopbackMesh {
    pub fn pair() -> (Self, Self) {
        let a_in = Arc::new(Mutex::new(VecDeque::new()));
        let b_in = Arc::new(Mutex::new(VecDeque::new()));
        let a = Self {
            inbound: a_in.clone(),
            outbound: b_in.clone(),
        };
        let b = Self {
            inbound: b_in,
            outbound: a_in,
        };
        (a, b)
    }
}

impl MeshTransport for LoopbackMesh {
    fn publish(&mut self, frame: QstpFrame) -> PqcResult<()> {
        self.outbound.lock().unwrap().push_back(frame);
        Ok(())
    }

    fn try_recv(&mut self, topic: &str) -> Option<QstpFrame> {
        let mut queue = self.inbound.lock().unwrap();
        if let Some(index) = queue.iter().position(|frame| frame.topic == topic) {
            queue.remove(index)
        } else {
            None
        }
    }
}

pub fn loopback_gateways(
    config: &QstpConfig,
) -> OverlayResult<(QstpGateway<LoopbackMesh>, QstpGateway<LoopbackMesh>)> {
    let mut tuple_store = InMemoryTupleChain::new();
    let route = config.route_plan();
    let init_peer = config.local_peer();
    let resp_peer = config.remote_peer();
    let handshake = format!(
        "init={}::resp={}::ts={}",
        config.local_peer_label, config.remote_peer_label, route.epoch
    );
    let established = establish_runtime_tunnel(
        handshake.as_bytes(),
        init_peer,
        route.clone(),
        &mut tuple_store,
    )?;
    let responder = hydrate_remote_tunnel(
        established.session_secret.clone(),
        resp_peer,
        route.clone(),
        established.peer_metadata.clone(),
        TunnelRole::Responder,
    )?;
    let (mesh_a, mesh_b) = LoopbackMesh::pair();
    let initiator = QstpGateway::new(established.tunnel, mesh_a);
    let responder = QstpGateway::new(responder, mesh_b);
    Ok((initiator, responder))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayFrame {
    VertexCreated {
        vertex_id: String,
        label: String,
        payload_bytes: usize,
    },
    ProofGenerated {
        proof_id: String,
        did: String,
        attribute: String,
    },
    CredentialRevoked {
        credential_id: String,
        tx_hash: String,
    },
    QtaidTokenized {
        owner_did: String,
        token_count: usize,
    },
}
