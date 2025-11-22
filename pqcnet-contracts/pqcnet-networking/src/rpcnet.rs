use autheo_pqc_core::error::PqcError;
use autheo_pqcnet_5dqeh::{ModuleError, MsgAnchorEdge, MsgAnchorEdgeResponse};
use pqcnet_qstp::{
    establish_runtime_tunnel, MeshPeerId, MeshRoutePlan, QstpEstablishedTunnel, QstpPeerMetadata,
    TunnelId, TupleChainStore,
};
use thiserror::Error;

/// Trait implemented by keepers/modules that can ingest `MsgAnchorEdge` requests.
pub trait AnchorEdgeEndpoint {
    fn submit_anchor_edge(
        &mut self,
        msg: MsgAnchorEdge,
    ) -> Result<MsgAnchorEdgeResponse, ModuleError>;
}

/// Deterministic RPCNet router that multiplexes anchor/tunnel requests for demos/tests.
pub struct RpcNetRouter<A, S> {
    anchor_endpoint: A,
    tuple_store: S,
}

impl<A, S> RpcNetRouter<A, S>
where
    A: AnchorEdgeEndpoint,
    S: TupleChainStore,
{
    pub fn new(anchor_endpoint: A, tuple_store: S) -> Self {
        Self {
            anchor_endpoint,
            tuple_store,
        }
    }

    /// Route an anchor-edge message to the configured keeper/module.
    pub fn anchor_edge(
        &mut self,
        msg: MsgAnchorEdge,
    ) -> Result<MsgAnchorEdgeResponse, RpcNetError> {
        self.anchor_endpoint
            .submit_anchor_edge(msg)
            .map_err(RpcNetError::from)
    }

    /// Establish a QSTP tunnel via the in-memory tuple store.
    pub fn open_tunnel(
        &mut self,
        req: MsgOpenTunnel,
    ) -> Result<MsgOpenTunnelResponse, RpcNetError> {
        let established = establish_runtime_tunnel(
            &req.client_request,
            req.peer_id,
            req.preferred_route,
            &mut self.tuple_store,
        )
        .map_err(RpcNetError::from)?;
        Ok(established.into())
    }

    /// Mutably access the underlying anchor endpoint (advanced diagnostics).
    pub fn anchor_endpoint_mut(&mut self) -> &mut A {
        &mut self.anchor_endpoint
    }
}

/// Request payload mirrored from `qstp.proto::OpenTunnelRequest`.
#[derive(Clone, Debug)]
pub struct MsgOpenTunnel {
    pub client_request: Vec<u8>,
    pub preferred_route: MeshRoutePlan,
    pub peer_id: MeshPeerId,
}

impl MsgOpenTunnel {
    pub fn new(
        client_request: impl Into<Vec<u8>>,
        route: MeshRoutePlan,
        peer_id: MeshPeerId,
    ) -> Self {
        Self {
            client_request: client_request.into(),
            preferred_route: route,
            peer_id,
        }
    }
}

/// Session material returned after `MsgOpenTunnel` completes.
#[derive(Clone, Debug)]
pub struct SessionKeyMaterial {
    pub tunnel_id: TunnelId,
    pub session_secret: Vec<u8>,
    pub route_epoch: u64,
    pub topic: String,
}

/// Response payload combining the handshake envelope + session metadata.
#[derive(Clone, Debug)]
pub struct MsgOpenTunnelResponse {
    pub handshake_envelope: Vec<u8>,
    pub peer_metadata: QstpPeerMetadata,
    pub session: SessionKeyMaterial,
}

impl From<QstpEstablishedTunnel> for MsgOpenTunnelResponse {
    fn from(established: QstpEstablishedTunnel) -> Self {
        let QstpEstablishedTunnel {
            tunnel,
            handshake_envelope,
            peer_metadata,
            session_secret,
        } = established;
        let session = SessionKeyMaterial {
            tunnel_id: tunnel.metadata().tunnel_id,
            session_secret,
            route_epoch: tunnel.route().epoch,
            topic: tunnel.route().topic.clone(),
        };
        Self {
            handshake_envelope,
            peer_metadata,
            session,
        }
    }
}

/// Errors surfaced by the RPCNet router.
#[derive(Error, Debug)]
pub enum RpcNetError {
    #[error(transparent)]
    Anchor(#[from] ModuleError),
    #[error(transparent)]
    Tunnel(#[from] PqcError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use autheo_pqcnet_5dqeh::{Icosuple, ModuleStorageLayout, PqcBinding, VertexId, VertexReceipt};
    use pqcnet_qstp::{InMemoryTupleChain, MeshPeerId, MeshQosClass, MeshRoutePlan};

    struct DummyAnchor {
        last_request: Option<MsgAnchorEdge>,
    }

    impl DummyAnchor {
        fn new() -> Self {
            Self { last_request: None }
        }
    }

    impl AnchorEdgeEndpoint for DummyAnchor {
        fn submit_anchor_edge(
            &mut self,
            msg: MsgAnchorEdge,
        ) -> Result<MsgAnchorEdgeResponse, ModuleError> {
            self.last_request = Some(msg);
            let receipt = VertexReceipt {
                vertex_id: VertexId([0u8; 32]),
                tw_score: 1.0,
                storage: autheo_pqcnet_5dqeh::StorageTarget::Hot,
                ann_similarity: 0.9,
                parents: 0,
                pqc_signature: None,
            };
            Ok(MsgAnchorEdgeResponse {
                storage: ModuleStorageLayout::default(),
                receipt,
            })
        }
    }

    fn sample_anchor() -> MsgAnchorEdge {
        MsgAnchorEdge {
            request_id: 1,
            chain_epoch: 1,
            parents: Vec::new(),
            parent_coherence: 0.1,
            lamport: 1,
            contribution_score: 0.5,
            ann_similarity: 0.9,
            qrng_entropy_bits: 256,
            pqc_binding: PqcBinding::simulated("rpcnet"),
            icosuple: Icosuple::synthesize("rpc", 1_024, 8, 0.9),
        }
    }

    #[test]
    fn router_delegates_anchor_edge() {
        let anchor = DummyAnchor::new();
        let tuple_store = InMemoryTupleChain::new();
        let mut router = RpcNetRouter::new(anchor, tuple_store);
        let response = router.anchor_edge(sample_anchor()).expect("anchor");
        assert_eq!(response.receipt.tw_score, 1.0);
        assert!(router.anchor_endpoint_mut().last_request.is_some());
    }

    #[test]
    fn router_opens_tunnel_via_qstp() {
        let anchor = DummyAnchor::new();
        let tuple_store = InMemoryTupleChain::new();
        let mut router = RpcNetRouter::new(anchor, tuple_store);
        let route = MeshRoutePlan {
            topic: "waku/rpcnet".into(),
            hops: vec![MeshPeerId::derive("peer-a")],
            qos: MeshQosClass::LowLatency,
            epoch: 1,
        };
        let req = MsgOpenTunnel::new(
            b"client=rpcnet&ts=1700",
            route,
            MeshPeerId::derive("peer-x"),
        );
        let resp = router.open_tunnel(req).expect("open tunnel");
        assert!(!resp.handshake_envelope.is_empty());
        assert_eq!(resp.session.topic, "waku/rpcnet");
        assert_eq!(resp.session.route_epoch, 1);
        assert!(!resp.session.session_secret.is_empty());
    }
}
