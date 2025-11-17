use crate::error::{PqcError, PqcResult};
use crate::handshake::{self, HandshakeArtifacts};
use crate::key_manager::ThresholdPolicy;
use crate::qace::{PathSet, QaceAction, QaceDecision, QaceEngine, QaceMetrics, QaceRequest};
use crate::types::{Bytes, KeyId, TimestampMs};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use blake2::Blake2s256;
use core::cmp;
use core::fmt;
use digest::Digest;

const TUPLE_LABEL: &[u8] = b"QSTP:TUPLE";

/// Stable identifier for a QSTP tunnel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TunnelId(pub [u8; 16]);

impl TunnelId {
    /// Access the tunnel identifier as bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Display for TunnelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Logical identifier for mesh peers (e.g., Waku peer IDs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshPeerId(pub [u8; 32]);

impl MeshPeerId {
    /// Deterministically derive a mesh peer identifier from an arbitrary label (tests/demo only).
    pub fn derive(label: &str) -> Self {
        use blake2::Blake2s256;

        let mut hasher = Blake2s256::new();
        hasher.update(label.as_bytes());
        let digest = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&digest[..32]);
        MeshPeerId(id)
    }
}

/// QoS class propagated to mesh transports (maps to Waku topic parameters).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshQosClass {
    Gossip,
    LowLatency,
    Control,
}

impl MeshQosClass {
    fn as_u8(self) -> u8 {
        match self {
            MeshQosClass::Gossip => 0x01,
            MeshQosClass::LowLatency => 0x02,
            MeshQosClass::Control => 0x03,
        }
    }
}

/// Routing hints used to publish encrypted frames across the mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshRoutePlan {
    pub topic: String,
    pub hops: Vec<MeshPeerId>,
    pub qos: MeshQosClass,
    pub epoch: u64,
}

impl MeshRoutePlan {
    /// Compute a stable hash over the route plan.
    pub fn route_hash(&self) -> [u8; 32] {
        let mut hasher = Blake2s256::new();
        hasher.update(self.topic.as_bytes());
        hasher.update(&[self.qos.as_u8()]);
        hasher.update(self.epoch.to_le_bytes());
        for hop in &self.hops {
            hasher.update(hop.0);
        }
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest[..32]);
        out
    }
}

/// Role determines how directional keys are derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelRole {
    Initiator,
    Responder,
}

/// Frame propagated over the mesh transport (e.g., via Waku pub-sub).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QstpFrame {
    pub tunnel_id: TunnelId,
    pub topic: String,
    pub seq: u64,
    pub nonce: [u8; 12],
    pub route_hash: [u8; 32],
    pub route_epoch: u64,
    pub ciphertext: Bytes,
}

/// Public metadata shared with remote peers via `qstp.proto`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QstpPeerMetadata {
    pub tunnel_id: TunnelId,
    pub kem_key_id: KeyId,
    pub signing_key_id: KeyId,
    pub threshold: ThresholdPolicy,
    pub tuple_pointer: TuplePointer,
    pub established_at: TimestampMs,
}

/// Runtime output returned when establishing a tunnel.
pub struct QstpEstablishedTunnel {
    pub tunnel: QstpTunnel,
    pub handshake_envelope: Bytes,
    pub peer_metadata: QstpPeerMetadata,
    pub session_secret: Bytes,
}

/// TupleChain pointer (opaque handle) returned after storing encrypted metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TuplePointer(pub [u8; 16]);

impl TuplePointer {
    pub fn zero() -> Self {
        TuplePointer([0u8; 16])
    }
}

/// Encrypted TupleChain record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TupleRecord {
    pub nonce: [u8; 12],
    pub ciphertext: Bytes,
}

/// TupleChain storage contract.
pub trait TupleChainStore {
    fn put(&mut self, record: TupleRecord) -> PqcResult<TuplePointer>;
    fn fetch(&self, pointer: &TuplePointer) -> Option<TupleRecord>;
}

/// Simple in-memory TupleChain implementation (used by examples/tests).
pub struct InMemoryTupleChain {
    storage: BTreeMap<[u8; 16], TupleRecord>,
}

impl InMemoryTupleChain {
    pub fn new() -> Self {
        Self {
            storage: BTreeMap::new(),
        }
    }
}

impl Default for InMemoryTupleChain {
    fn default() -> Self {
        Self::new()
    }
}

impl TupleChainStore for InMemoryTupleChain {
    fn put(&mut self, record: TupleRecord) -> PqcResult<TuplePointer> {
        let mut hasher = Blake2s256::new();
        hasher.update(&record.nonce);
        hasher.update(&record.ciphertext);
        let digest = hasher.finalize();
        let mut ptr = [0u8; 16];
        ptr.copy_from_slice(&digest[..16]);
        self.storage.insert(ptr, record);
        Ok(TuplePointer(ptr))
    }

    fn fetch(&self, pointer: &TuplePointer) -> Option<TupleRecord> {
        self.storage.get(&pointer.0).cloned()
    }
}

/// Mesh transport contract (maps naturally onto Waku pub-sub networks).
pub trait MeshTransport {
    fn publish(&mut self, frame: QstpFrame) -> PqcResult<()>;
    fn try_recv(&mut self, topic: &str) -> Option<QstpFrame>;
}

/// Deterministic in-memory mesh simulator satisfying the transport trait.
pub struct InMemoryMesh {
    topics: BTreeMap<String, VecDeque<QstpFrame>>,
}

impl InMemoryMesh {
    pub fn new() -> Self {
        Self {
            topics: BTreeMap::new(),
        }
    }

    pub fn len(&self, topic: &str) -> usize {
        self.topics.get(topic).map(|q| q.len()).unwrap_or(0)
    }
}

impl Default for InMemoryMesh {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshTransport for InMemoryMesh {
    fn publish(&mut self, frame: QstpFrame) -> PqcResult<()> {
        self.topics
            .entry(frame.topic.clone())
            .or_insert_with(VecDeque::new)
            .push_back(frame);
        Ok(())
    }

    fn try_recv(&mut self, topic: &str) -> Option<QstpFrame> {
        self.topics.get_mut(topic)?.pop_front()
    }
}

/// Internal metadata stored alongside cipher state.
#[derive(Clone, Debug)]
pub struct QstpTunnelMetadata {
    pub tunnel_id: TunnelId,
    pub kem_key_id: KeyId,
    pub signing_key_id: KeyId,
    pub threshold: ThresholdPolicy,
    pub peer: MeshPeerId,
    pub qos: MeshQosClass,
    pub established_at: TimestampMs,
    pub tuple_pointer: TuplePointer,
}

struct TxState {
    cipher: Aes256Gcm,
    nonce_base: [u8; 12],
    seq: u64,
}

struct RxState {
    cipher: Aes256Gcm,
    nonce_base: [u8; 12],
}

/// Active tunnel containing derived symmetric material.
pub struct QstpTunnel {
    metadata: QstpTunnelMetadata,
    role: TunnelRole,
    route: MeshRoutePlan,
    route_hash: [u8; 32],
    tuple_key: [u8; 32],
    tx: TxState,
    rx: RxState,
    recv_watermark: u64,
    alternates: Vec<MeshRoutePlan>,
    last_decision: Option<QaceDecision>,
}

impl QstpTunnel {
    /// Encrypt an application payload into a mesh-ready frame.
    pub fn seal(&mut self, payload: &[u8], app_aad: &[u8]) -> PqcResult<QstpFrame> {
        let seq = self.tx.seq;
        let nonce = compose_nonce(&self.tx.nonce_base, seq);
        let aad = build_aad(&self.metadata.tunnel_id, &self.route_hash, seq, app_aad);
        let ciphertext = self
            .tx
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: payload,
                    aad: &aad,
                },
            )
            .map_err(|_| PqcError::PrimitiveFailure("qstp aes-gcm seal failed"))?;
        self.tx.seq = self.tx.seq.wrapping_add(1);

        Ok(QstpFrame {
            tunnel_id: self.metadata.tunnel_id,
            topic: self.route.topic.clone(),
            seq,
            nonce,
            route_hash: self.route_hash,
            route_epoch: self.route.epoch,
            ciphertext,
        })
    }

    /// Decrypt a frame produced by the remote endpoint.
    pub fn open(&mut self, frame: &QstpFrame, app_aad: &[u8]) -> PqcResult<Bytes> {
        if frame.tunnel_id != self.metadata.tunnel_id {
            return Err(PqcError::InvalidInput("tunnel id mismatch"));
        }
        if frame.route_hash != self.route_hash {
            return Err(PqcError::InvalidInput("route hash mismatch"));
        }
        if frame.route_epoch != self.route.epoch {
            return Err(PqcError::InvalidInput("route epoch mismatch"));
        }
        if frame.seq < self.recv_watermark {
            return Err(PqcError::LimitExceeded("frame replayed"));
        }
        let expected_nonce = compose_nonce(&self.rx.nonce_base, frame.seq);
        if expected_nonce != frame.nonce {
            return Err(PqcError::InvalidInput("nonce mismatch"));
        }
        let aad = build_aad(
            &self.metadata.tunnel_id,
            &self.route_hash,
            frame.seq,
            app_aad,
        );
        let plaintext = self
            .rx
            .cipher
            .decrypt(
                Nonce::from_slice(&frame.nonce),
                Payload {
                    msg: &frame.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| PqcError::VerifyFailed)?;
        self.recv_watermark = frame.seq + 1;
        Ok(plaintext)
    }

    /// Attach fallback routes that QACE can leverage.
    pub fn register_alternate_routes(&mut self, routes: Vec<MeshRoutePlan>) {
        self.alternates = routes;
    }

    /// Apply QACE metrics and enact any reroute/rekey decision that is returned.
    pub fn apply_qace<E: QaceEngine>(
        &mut self,
        metrics: QaceMetrics,
        engine: &mut E,
    ) -> PqcResult<QaceDecision> {
        let path_set = PathSet::new(self.route.clone(), self.alternates.clone());
        let request = QaceRequest {
            tunnel_id: &self.metadata.tunnel_id,
            telemetry_epoch: self.route.epoch,
            metrics,
            path_set,
        };
        let decision = engine.evaluate(request)?;
        let route_changed = self.route != decision.path_set.primary;
        if route_changed {
            self.install_route(decision.path_set.primary.clone());
        }
        self.alternates = decision.path_set.alternates.clone();
        match &decision.action {
            QaceAction::Maintain => {}
            QaceAction::Rekey => self.rotate_route_material(),
            QaceAction::Reroute(_) => {
                if !route_changed {
                    self.install_route(decision.path_set.primary.clone());
                }
            }
        }
        self.last_decision = Some(decision.clone());
        Ok(decision)
    }

    /// Retrieve immutable metadata associated with the tunnel.
    pub fn metadata(&self) -> &QstpTunnelMetadata {
        &self.metadata
    }

    /// Current active route.
    pub fn route(&self) -> &MeshRoutePlan {
        &self.route
    }

    /// Fetch TupleChain metadata, ensuring ciphertext remains opaque to intermediaries.
    pub fn fetch_tuple_metadata<S: TupleChainStore>(
        &self,
        store: &S,
    ) -> PqcResult<TupleMetadataPlain> {
        let record =
            store
                .fetch(&self.metadata.tuple_pointer)
                .ok_or(PqcError::IntegrationError(
                    "tuple metadata missing from store".into(),
                ))?;
        let plaintext = decrypt_tuple_metadata(&self.tuple_key, &record)?;
        TupleMetadataPlain::from_bytes(&plaintext)
    }

    fn rotate_route_material(&mut self) {
        let salt = derive_context(&self.metadata.tunnel_id, &self.route_hash, self.route.epoch);
        let (send_label, recv_label) = match self.role {
            TunnelRole::Initiator => (
                b"route:init->resp".as_slice(),
                b"route:resp->init".as_slice(),
            ),
            TunnelRole::Responder => (
                b"route:resp->init".as_slice(),
                b"route:init->resp".as_slice(),
            ),
        };
        let send_material = kdf_expand(&self.tuple_key, send_label, &salt, 12);
        let recv_material = kdf_expand(&self.tuple_key, recv_label, &salt, 12);
        self.tx.nonce_base.copy_from_slice(&send_material);
        self.rx.nonce_base.copy_from_slice(&recv_material);
    }

    fn install_route(&mut self, route: MeshRoutePlan) {
        self.route = route;
        self.route_hash = self.route.route_hash();
        self.rotate_route_material();
    }
}

/// Plaintext TupleChain metadata persisted off-chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TupleMetadataPlain {
    pub tunnel_id: TunnelId,
    pub kem_key_id: KeyId,
    pub signing_key_id: KeyId,
    pub threshold: ThresholdPolicy,
    pub route_hash: [u8; 32],
    pub qos: MeshQosClass,
    pub route_epoch: u64,
    pub established_at: TimestampMs,
}

impl TupleMetadataPlain {
    fn to_bytes(&self) -> Bytes {
        let mut out = Bytes::with_capacity(16 + 32 + 32 + 2 + 32 + 1 + 8 + 8);
        out.extend_from_slice(self.tunnel_id.as_bytes());
        out.extend_from_slice(&self.kem_key_id.0);
        out.extend_from_slice(&self.signing_key_id.0);
        out.push(self.threshold.t);
        out.push(self.threshold.n);
        out.extend_from_slice(&self.route_hash);
        out.push(self.qos.as_u8());
        out.extend_from_slice(&self.route_epoch.to_le_bytes());
        out.extend_from_slice(&self.established_at.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8]) -> PqcResult<Self> {
        if bytes.len() < 16 + 32 + 32 + 2 + 32 + 1 + 8 + 8 {
            return Err(PqcError::InvalidInput("tuple metadata truncated"));
        }
        let mut cursor = 0usize;
        let mut tunnel = [0u8; 16];
        tunnel.copy_from_slice(&bytes[cursor..cursor + 16]);
        cursor += 16;
        let mut kem = [0u8; 32];
        kem.copy_from_slice(&bytes[cursor..cursor + 32]);
        cursor += 32;
        let mut signing = [0u8; 32];
        signing.copy_from_slice(&bytes[cursor..cursor + 32]);
        cursor += 32;
        let threshold = ThresholdPolicy {
            t: bytes[cursor],
            n: bytes[cursor + 1],
        };
        cursor += 2;
        let mut route_hash = [0u8; 32];
        route_hash.copy_from_slice(&bytes[cursor..cursor + 32]);
        cursor += 32;
        let qos = match bytes[cursor] {
            0x01 => MeshQosClass::Gossip,
            0x02 => MeshQosClass::LowLatency,
            0x03 => MeshQosClass::Control,
            _ => return Err(PqcError::InvalidInput("unknown qos class")),
        };
        cursor += 1;
        let mut epoch_bytes = [0u8; 8];
        epoch_bytes.copy_from_slice(&bytes[cursor..cursor + 8]);
        cursor += 8;
        let mut established_bytes = [0u8; 8];
        established_bytes.copy_from_slice(&bytes[cursor..cursor + 8]);

        Ok(Self {
            tunnel_id: TunnelId(tunnel),
            kem_key_id: KeyId(kem),
            signing_key_id: KeyId(signing),
            threshold,
            route_hash,
            qos,
            route_epoch: u64::from_le_bytes(epoch_bytes),
            established_at: u64::from_le_bytes(established_bytes),
        })
    }
}

/// Establish a QSTP tunnel while running inside the WASM runtime.
pub fn establish_runtime_tunnel<S: TupleChainStore>(
    request: &[u8],
    peer: MeshPeerId,
    mut route: MeshRoutePlan,
    tuple_store: &mut S,
) -> PqcResult<QstpEstablishedTunnel> {
    if request.is_empty() {
        return Err(PqcError::InvalidInput("qstp request missing"));
    }
    if route.topic.is_empty() {
        return Err(PqcError::InvalidInput("mesh topic missing"));
    }
    let artifacts = handshake::build_handshake_artifacts(request)?;
    if route.epoch == 0 {
        route.epoch = artifacts.timestamp_ms;
    }
    finalize_tunnel(artifacts, peer, route, tuple_store, TunnelRole::Initiator)
}

/// Hydrate a tunnel from metadata shared via `qstp.proto` (e.g., remote peer).
pub fn hydrate_remote_tunnel(
    shared_secret: Bytes,
    peer: MeshPeerId,
    route: MeshRoutePlan,
    metadata: QstpPeerMetadata,
    role: TunnelRole,
) -> PqcResult<QstpTunnel> {
    let tuple_key = derive_tuple_key(&shared_secret, &metadata.tunnel_id, &route);
    let (tx_state, rx_state) =
        derive_directional_ciphers(&shared_secret, &metadata.tunnel_id, &route, role)?;
    Ok(QstpTunnel {
        metadata: QstpTunnelMetadata {
            tunnel_id: metadata.tunnel_id,
            kem_key_id: metadata.kem_key_id,
            signing_key_id: metadata.signing_key_id,
            threshold: metadata.threshold,
            peer,
            qos: route.qos,
            established_at: metadata.established_at,
            tuple_pointer: metadata.tuple_pointer,
        },
        role,
        route_hash: route.route_hash(),
        route,
        tuple_key,
        tx: tx_state,
        rx: rx_state,
        recv_watermark: 0,
        alternates: Vec::new(),
        last_decision: None,
    })
}

fn finalize_tunnel<S: TupleChainStore>(
    artifacts: HandshakeArtifacts,
    peer: MeshPeerId,
    route: MeshRoutePlan,
    tuple_store: &mut S,
    role: TunnelRole,
) -> PqcResult<QstpEstablishedTunnel> {
    let handshake_bytes = serialize_artifacts(&artifacts)?;
    let HandshakeArtifacts {
        threshold,
        kem_state,
        signing_state,
        ciphertext,
        shared_secret,
        signature,
        timestamp_ms,
    } = artifacts;

    let tunnel_id = derive_tunnel_id(&ciphertext, &signature, &route);
    let route_hash = route.route_hash();

    let tuple_plain = TupleMetadataPlain {
        tunnel_id,
        kem_key_id: kem_state.id.clone(),
        signing_key_id: signing_state.id.clone(),
        threshold,
        route_hash,
        qos: route.qos,
        route_epoch: route.epoch,
        established_at: timestamp_ms,
    };

    let tuple_key = derive_tuple_key(&shared_secret, &tunnel_id, &route);
    let tuple_record = encrypt_tuple_metadata(&tuple_key, tuple_plain.to_bytes())?;
    let tuple_pointer = tuple_store.put(tuple_record)?;

    let (tx_state, rx_state) =
        derive_directional_ciphers(&shared_secret, &tunnel_id, &route, role)?;

    let metadata = QstpTunnelMetadata {
        tunnel_id,
        kem_key_id: kem_state.id.clone(),
        signing_key_id: signing_state.id.clone(),
        threshold,
        peer,
        qos: route.qos,
        established_at: timestamp_ms,
        tuple_pointer,
    };

    let peer_metadata = QstpPeerMetadata {
        tunnel_id,
        kem_key_id: kem_state.id,
        signing_key_id: signing_state.id,
        threshold,
        tuple_pointer,
        established_at: timestamp_ms,
    };

    let tunnel = QstpTunnel {
        metadata,
        role,
        route_hash,
        route,
        tuple_key,
        tx: tx_state,
        rx: rx_state,
        recv_watermark: 0,
        alternates: Vec::new(),
        last_decision: None,
    };

    Ok(QstpEstablishedTunnel {
        tunnel,
        handshake_envelope: handshake_bytes,
        peer_metadata,
        session_secret: shared_secret,
    })
}

fn serialize_artifacts(artifacts: &HandshakeArtifacts) -> PqcResult<Bytes> {
    let total_len = handshake::compute_handshake_len(artifacts);
    let mut buffer = vec![0u8; total_len];
    let written = handshake::serialize_handshake(artifacts, &mut buffer)?;
    buffer.truncate(written);
    Ok(buffer)
}

fn derive_tunnel_id(ciphertext: &[u8], signature: &[u8], route: &MeshRoutePlan) -> TunnelId {
    let mut hasher = Blake2s256::new();
    hasher.update(ciphertext);
    hasher.update(signature);
    hasher.update(route.route_hash());
    let digest = hasher.finalize();
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    TunnelId(id)
}

fn derive_directional_ciphers(
    shared_secret: &[u8],
    tunnel_id: &TunnelId,
    route: &MeshRoutePlan,
    role: TunnelRole,
) -> PqcResult<(TxState, RxState)> {
    let context = derive_context(tunnel_id, &route.route_hash(), route.epoch);
    let (tx_label, rx_label) = match role {
        TunnelRole::Initiator => (b"role:init->resp".as_slice(), b"role:resp->init".as_slice()),
        TunnelRole::Responder => (b"role:resp->init".as_slice(), b"role:init->resp".as_slice()),
    };

    let tx_material = kdf_expand(shared_secret, tx_label, &context, 44);
    let rx_material = kdf_expand(shared_secret, rx_label, &context, 44);

    let tx_key = &tx_material[..32];
    let tx_nonce = &tx_material[32..44];
    let rx_key = &rx_material[..32];
    let rx_nonce = &rx_material[32..44];

    let tx_cipher = Aes256Gcm::new_from_slice(tx_key)
        .map_err(|_| PqcError::PrimitiveFailure("invalid aes-gcm tx key"))?;
    let rx_cipher = Aes256Gcm::new_from_slice(rx_key)
        .map_err(|_| PqcError::PrimitiveFailure("invalid aes-gcm rx key"))?;

    let mut tx_nonce_base = [0u8; 12];
    tx_nonce_base.copy_from_slice(tx_nonce);
    let mut rx_nonce_base = [0u8; 12];
    rx_nonce_base.copy_from_slice(rx_nonce);

    Ok((
        TxState {
            cipher: tx_cipher,
            nonce_base: tx_nonce_base,
            seq: 0,
        },
        RxState {
            cipher: rx_cipher,
            nonce_base: rx_nonce_base,
        },
    ))
}

fn derive_tuple_key(shared_secret: &[u8], tunnel_id: &TunnelId, route: &MeshRoutePlan) -> [u8; 32] {
    let context = derive_context(tunnel_id, &route.route_hash(), route.epoch);
    let material = kdf_expand(shared_secret, TUPLE_LABEL, &context, 32);
    let mut key = [0u8; 32];
    key.copy_from_slice(&material);
    key
}

fn encrypt_tuple_metadata(tuple_key: &[u8; 32], plaintext: Bytes) -> PqcResult<TupleRecord> {
    let cipher = Aes256Gcm::new_from_slice(tuple_key)
        .map_err(|_| PqcError::PrimitiveFailure("invalid tuple key"))?;
    let nonce_material = kdf_expand(tuple_key, b"tuple-nonce", b"", 12);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&nonce_material);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| PqcError::PrimitiveFailure("tuple aes-gcm encrypt failed"))?;
    Ok(TupleRecord { nonce, ciphertext })
}

fn decrypt_tuple_metadata(tuple_key: &[u8; 32], record: &TupleRecord) -> PqcResult<Bytes> {
    let cipher = Aes256Gcm::new_from_slice(tuple_key)
        .map_err(|_| PqcError::PrimitiveFailure("invalid tuple key"))?;
    cipher
        .decrypt(Nonce::from_slice(&record.nonce), record.ciphertext.as_ref())
        .map_err(|_| PqcError::VerifyFailed)
}

fn derive_context(tunnel_id: &TunnelId, route_hash: &[u8; 32], epoch: u64) -> Bytes {
    let mut ctx = Bytes::with_capacity(16 + 32 + 8);
    ctx.extend_from_slice(tunnel_id.as_bytes());
    ctx.extend_from_slice(route_hash);
    ctx.extend_from_slice(&epoch.to_le_bytes());
    ctx
}

fn kdf_expand(shared: &[u8], label: &[u8], context: &[u8], out_len: usize) -> Bytes {
    let mut output = Bytes::with_capacity(out_len);
    let mut counter: u8 = 1;
    while output.len() < out_len {
        let mut hasher = Blake2s256::new();
        hasher.update(label);
        hasher.update(&[counter]);
        hasher.update(shared);
        hasher.update(context);
        let digest = hasher.finalize();
        let take = cmp::min(out_len - output.len(), digest.len());
        output.extend_from_slice(&digest[..take]);
        counter = counter.wrapping_add(1);
    }
    output
}

fn compose_nonce(base: &[u8; 12], seq: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&base[..4]);
    nonce[4..12].copy_from_slice(&seq.to_le_bytes());
    nonce
}

fn build_aad(tunnel_id: &TunnelId, route_hash: &[u8; 32], seq: u64, app_aad: &[u8]) -> Bytes {
    let mut aad = Bytes::with_capacity(16 + 32 + 8 + app_aad.len());
    aad.extend_from_slice(tunnel_id.as_bytes());
    aad.extend_from_slice(route_hash);
    aad.extend_from_slice(&seq.to_le_bytes());
    aad.extend_from_slice(app_aad);
    aad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::DemoMlKem;
    use crate::kem::MlKemEngine;
    use crate::qace::{QaceAction, SimpleQace};
    use crate::runtime;
    use alloc::boxed::Box;

    #[test]
    fn handshake_between_two_endpoints_matches_shared_secret() {
        let engine = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let server_pair = engine.keygen().expect("server keypair");

        let client_enc = engine
            .encapsulate(&server_pair.public_key)
            .expect("encapsulate");
        let server_secret = engine
            .decapsulate(&server_pair.secret_key, &client_enc.ciphertext)
            .expect("decapsulate");
        assert_eq!(client_enc.shared_secret, server_secret);
    }

    #[test]
    fn qstp_tunnel_encrypts_and_decrypts_payload() {
        runtime::reset_state_for_tests();
        let mut tuple_chain = InMemoryTupleChain::new();
        let server_peer = MeshPeerId::derive("node-a");
        let client_peer = MeshPeerId::derive("node-b");
        let route = MeshRoutePlan {
            topic: "waku/mesh/alpha".into(),
            hops: vec![MeshPeerId::derive("edge-1")],
            qos: MeshQosClass::LowLatency,
            epoch: 42,
        };
        let mut established = establish_runtime_tunnel(
            b"client=test&ts=42",
            client_peer,
            route.clone(),
            &mut tuple_chain,
        )
        .expect("tunnel");
        let mut responder = hydrate_remote_tunnel(
            established.session_secret.clone(),
            server_peer,
            route,
            established.peer_metadata.clone(),
            TunnelRole::Responder,
        )
        .expect("hydrate");
        let frame = established
            .tunnel
            .seal(b"hello-qstp", b"app")
            .expect("seal");
        let plaintext = responder.open(&frame, b"app").expect("open");
        assert_eq!(plaintext, b"hello-qstp");
    }

    #[test]
    fn qstp_rerouted_payload_decrypts() {
        runtime::reset_state_for_tests();
        let mut tuple_chain = InMemoryTupleChain::new();
        let server_peer = MeshPeerId::derive("reroute-server");
        let client_peer = MeshPeerId::derive("reroute-client");
        let primary = MeshRoutePlan {
            topic: "waku/reroute-primary".into(),
            hops: vec![MeshPeerId::derive("hop-main")],
            qos: MeshQosClass::LowLatency,
            epoch: 7,
        };
        let mut established = establish_runtime_tunnel(
            b"client=reroute",
            client_peer,
            primary.clone(),
            &mut tuple_chain,
        )
        .expect("tunnel");
        let mut responder = hydrate_remote_tunnel(
            established.session_secret.clone(),
            server_peer,
            primary,
            established.peer_metadata.clone(),
            TunnelRole::Responder,
        )
        .expect("hydrate");
        let first = established
            .tunnel
            .seal(b"initial", b"ctx")
            .expect("first seal");
        let clear = responder.open(&first, b"ctx").expect("first decrypt");
        assert_eq!(clear, b"initial");

        let alternate = MeshRoutePlan {
            topic: "waku/reroute-alt".into(),
            hops: vec![MeshPeerId::derive("hop-alt")],
            qos: MeshQosClass::Control,
            epoch: 8,
        };
        established
            .tunnel
            .register_alternate_routes(vec![alternate.clone()]);
        let mut hook = SimpleQace::default();
        let decision = established
            .tunnel
            .apply_qace(
                QaceMetrics {
                    latency_ms: 1,
                    loss_bps: 8_000,
                    threat_score: 99,
                    route_changes: 0,
                    ..Default::default()
                },
                &mut hook,
            )
            .expect("reroute apply");
        let new_route = match &decision.action {
            QaceAction::Reroute(route) => route.clone(),
            other => panic!("expected reroute, got {other:?}"),
        };
        responder.register_alternate_routes(vec![new_route.clone()]);
        let responder_decision = responder
            .apply_qace(
                QaceMetrics {
                    latency_ms: 1,
                    loss_bps: 8_000,
                    threat_score: 99,
                    route_changes: 1,
                    ..Default::default()
                },
                &mut SimpleQace::default(),
            )
            .expect("responder reroute");
        assert_eq!(responder_decision.path_set.primary.topic, new_route.topic);

        let rerouted = established
            .tunnel
            .seal(b"after-reroute", b"ctx")
            .expect("rerouted seal");
        let clear_two = responder.open(&rerouted, b"ctx").expect("rerouted decrypt");
        assert_eq!(clear_two, b"after-reroute");
    }

    #[test]
    fn eavesdropper_cannot_decrypt_frame() {
        runtime::reset_state_for_tests();
        let mut tuple_chain = InMemoryTupleChain::new();
        let server_peer = MeshPeerId::derive("edge-server");
        let client_peer = MeshPeerId::derive("edge-client");
        let route = MeshRoutePlan {
            topic: "waku/test".into(),
            hops: vec![],
            qos: MeshQosClass::Gossip,
            epoch: 7,
        };
        let mut established =
            establish_runtime_tunnel(b"client=aa", client_peer, route.clone(), &mut tuple_chain)
                .expect("tunnel");
        let mut attacker = hydrate_remote_tunnel(
            vec![0u8; established.session_secret.len()],
            server_peer,
            route.clone(),
            established.peer_metadata.clone(),
            TunnelRole::Responder,
        )
        .expect("attacker tunnel");
        let frame = established
            .tunnel
            .seal(b"secret", b"ctx")
            .expect("seal payload");
        assert!(matches!(
            attacker.open(&frame, b"ctx"),
            Err(PqcError::VerifyFailed | PqcError::InvalidInput(_))
        ));
        // ensure tuple metadata decrypts
        let tuple = established
            .tunnel
            .fetch_tuple_metadata(&tuple_chain)
            .expect("fetch tuple");
        assert_eq!(tuple.tunnel_id, established.tunnel.metadata().tunnel_id);
    }

    #[test]
    fn qace_reroute_updates_route_hash() {
        runtime::reset_state_for_tests();
        let mut tuple_chain = InMemoryTupleChain::new();
        let peer = MeshPeerId::derive("qace");
        let route = MeshRoutePlan {
            topic: "waku/r0".into(),
            hops: vec![MeshPeerId::derive("hop-a")],
            qos: MeshQosClass::Control,
            epoch: 100,
        };
        let mut established =
            establish_runtime_tunnel(b"client=qace", peer, route, &mut tuple_chain)
                .expect("tunnel");
        let mut hook = SimpleQace::default();
        let new_route = MeshRoutePlan {
            topic: "waku/r1".into(),
            hops: vec![MeshPeerId::derive("hop-b")],
            qos: MeshQosClass::Control,
            epoch: 101,
        };
        established
            .tunnel
            .register_alternate_routes(vec![new_route.clone()]);
        let decision = established
            .tunnel
            .apply_qace(
                QaceMetrics {
                    latency_ms: 1,
                    loss_bps: 10,
                    threat_score: 90,
                    route_changes: 0,
                    ..Default::default()
                },
                &mut hook,
            )
            .expect("qace apply");
        assert_eq!(decision.path_set.primary.topic, "waku/r1");
    }
}
