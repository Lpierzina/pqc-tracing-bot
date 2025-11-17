//! PQCNet handshake façade exported to WASM hosts.
//!
//! The handshake now performs the full ML-KEM + ML-DSA flow:
//! 1. Load the currently active ML-KEM key from the [`KeyManager`].
//! 2. Run `encapsulate_for_current()` to derive a fresh shared secret.
//! 3. Bind `ciphertext || shared_secret || host_request` with ML-DSA via
//!    `SignatureManager::sign_kem_transcript`.
//! 4. Serialize the artifacts (threshold policy, key identifiers, metadata,
//!    ciphertext, shared secret, and transcript signature) into a binary record
//!    returned to the host.

use crate::dsa::MlDsaEngine;
use crate::error::{PqcError, PqcResult};
use crate::kem::MlKemEngine;
use crate::key_manager::{KemKeyState, ThresholdPolicy};
use crate::runtime;
use crate::signatures::DsaKeyState;
use crate::types::{Bytes, KeyId, SecurityLevel, TimestampMs};
use blake2::Blake2s256;
use core::cmp;
use digest::{Digest, Output};

const HANDSHAKE_MAGIC: &[u8; 4] = b"PQC1";
const HANDSHAKE_VERSION: u8 = 1;
pub(crate) const HANDSHAKE_HEADER_LEN: usize = 4  // magic
    + 1                                // version
    + 1                                // kem level
    + 1                                // dsa level
    + 1                                // threshold t
    + 1                                // threshold n
    + 1                                // reserved
    + 32                               // kem key id
    + 32                               // dsa key id
    + 8                                // kem created_at
    + 8                                // kem expires_at
    + (2 * 5); // section lengths

/// Execute the full handshake flow and serialize the result into `response`.
pub fn execute_handshake(request: &[u8], response: &mut [u8]) -> PqcResult<usize> {
    if request.is_empty() {
        return Err(PqcError::InvalidInput("handshake request is empty"));
    }

    let timestamp_hint = parse_timestamp_hint(request);
    let artifacts = build_handshake_artifacts_with_hint(request, timestamp_hint)?;
    serialize_handshake(&artifacts, response)
}

pub(crate) fn build_handshake_artifacts(request: &[u8]) -> PqcResult<HandshakeArtifacts> {
    build_handshake_artifacts_with_hint(request, parse_timestamp_hint(request))
}

fn build_handshake_artifacts_with_hint(
    request: &[u8],
    timestamp_hint: Option<TimestampMs>,
) -> PqcResult<HandshakeArtifacts> {
    runtime::with_contract_state(|state| -> PqcResult<HandshakeArtifacts> {
        let now_ms = state.advance_time(timestamp_hint);
        // Rotate the ML-KEM key if needed before encapsulation.
        let _ = state.key_manager.rotate_if_needed(now_ms)?;
        let (kem_state, encapsulation) = state.key_manager.encapsulate_for_current()?;
        let threshold = state.key_manager.threshold_policy();

        let signature = state.signature_manager.sign_kem_transcript(
            &state.signing_secret_key,
            &encapsulation,
            request,
        )?;

        Ok(HandshakeArtifacts {
            threshold,
            kem_state,
            signing_state: state.signing_key_state.clone(),
            ciphertext: encapsulation.ciphertext,
            shared_secret: encapsulation.shared_secret,
            signature,
            timestamp_ms: now_ms,
        })
    })
}

pub(crate) fn serialize_handshake(
    artifacts: &HandshakeArtifacts,
    out: &mut [u8],
) -> PqcResult<usize> {
    let ciphertext_len = artifacts.ciphertext.len();
    let shared_secret_len = artifacts.shared_secret.len();
    let signature_len = artifacts.signature.len();
    let kem_pk_len = artifacts.kem_state.public_key.len();
    let dsa_pk_len = artifacts.signing_state.public_key.len();

    let total_len = HANDSHAKE_HEADER_LEN
        + ciphertext_len
        + shared_secret_len
        + signature_len
        + kem_pk_len
        + dsa_pk_len;

    if out.len() < total_len {
        return Err(PqcError::LimitExceeded("response buffer too small"));
    }

    let mut offset = 0;

    out[offset..offset + 4].copy_from_slice(HANDSHAKE_MAGIC);
    offset += 4;
    out[offset] = HANDSHAKE_VERSION;
    offset += 1;
    out[offset] = encode_security_level(artifacts.kem_state.level);
    offset += 1;
    out[offset] = encode_security_level(artifacts.signing_state.level);
    offset += 1;
    out[offset] = artifacts.threshold.t;
    offset += 1;
    out[offset] = artifacts.threshold.n;
    offset += 1;
    out[offset] = 0; // reserved
    offset += 1;

    offset += copy_key_id(&artifacts.kem_state.id, &mut out[offset..]);
    offset += copy_key_id(&artifacts.signing_state.id, &mut out[offset..]);

    offset += copy_u64(artifacts.kem_state.created_at, &mut out[offset..]);
    offset += copy_u64(artifacts.kem_state.expires_at, &mut out[offset..]);

    let ciphertext_len_u16 = len_to_u16(ciphertext_len, "ciphertext section too large")?;
    let shared_secret_len_u16 = len_to_u16(shared_secret_len, "shared secret section too large")?;
    let signature_len_u16 = len_to_u16(signature_len, "signature section too large")?;
    let kem_pk_len_u16 = len_to_u16(kem_pk_len, "ml-kem public key too large")?;
    let dsa_pk_len_u16 = len_to_u16(dsa_pk_len, "ml-dsa public key too large")?;

    offset += copy_u16(ciphertext_len_u16, &mut out[offset..]);
    offset += copy_u16(shared_secret_len_u16, &mut out[offset..]);
    offset += copy_u16(signature_len_u16, &mut out[offset..]);
    offset += copy_u16(kem_pk_len_u16, &mut out[offset..]);
    offset += copy_u16(dsa_pk_len_u16, &mut out[offset..]);

    debug_assert_eq!(offset, HANDSHAKE_HEADER_LEN);

    offset += copy_slice(&artifacts.ciphertext, &mut out[offset..]);
    offset += copy_slice(&artifacts.shared_secret, &mut out[offset..]);
    offset += copy_slice(&artifacts.signature, &mut out[offset..]);
    offset += copy_slice(&artifacts.kem_state.public_key, &mut out[offset..]);
    offset += copy_slice(&artifacts.signing_state.public_key, &mut out[offset..]);

    Ok(offset)
}

pub(crate) fn compute_handshake_len(artifacts: &HandshakeArtifacts) -> usize {
    HANDSHAKE_HEADER_LEN
        + artifacts.ciphertext.len()
        + artifacts.shared_secret.len()
        + artifacts.signature.len()
        + artifacts.kem_state.public_key.len()
        + artifacts.signing_state.public_key.len()
}

fn copy_slice(src: &[u8], dst: &mut [u8]) -> usize {
    let len = src.len();
    dst[..len].copy_from_slice(src);
    len
}

fn copy_key_id(id: &KeyId, dst: &mut [u8]) -> usize {
    dst[..32].copy_from_slice(&id.0);
    32
}

fn copy_u64(value: u64, dst: &mut [u8]) -> usize {
    dst[..8].copy_from_slice(&value.to_le_bytes());
    8
}

fn copy_u16(value: u16, dst: &mut [u8]) -> usize {
    dst[..2].copy_from_slice(&value.to_le_bytes());
    2
}

fn len_to_u16(len: usize, label: &'static str) -> PqcResult<u16> {
    if len > u16::MAX as usize {
        return Err(PqcError::LimitExceeded(label));
    }
    Ok(len as u16)
}

fn encode_security_level(level: SecurityLevel) -> u8 {
    match level {
        SecurityLevel::MlKem128 | SecurityLevel::MlDsa128 => 0x01,
        SecurityLevel::MlKem192 | SecurityLevel::MlDsa192 => 0x02,
        SecurityLevel::MlKem256 | SecurityLevel::MlDsa256 => 0x03,
    }
}

fn parse_timestamp_hint(request: &[u8]) -> Option<TimestampMs> {
    if let Ok(as_str) = core::str::from_utf8(request) {
        for part in as_str.split('&') {
            if let Some(value) = part.strip_prefix("ts=") {
                if let Ok(parsed) = value.parse::<u64>() {
                    return Some(parsed);
                }
            }
        }
    }
    None
}

pub(crate) struct HandshakeArtifacts {
    pub threshold: ThresholdPolicy,
    pub kem_state: KemKeyState,
    pub signing_state: DsaKeyState,
    pub ciphertext: Bytes,
    pub shared_secret: Bytes,
    pub signature: Bytes,
    pub timestamp_ms: TimestampMs,
}

const CLIENT_TRANSCRIPT_LABEL: &[u8] = b"qstp:client-transcript";
const SERVER_TRANSCRIPT_LABEL: &[u8] = b"qstp:server-transcript";
const INIT_NONCE_LABEL: &[u8] = b"qstp:init-nonce";
const RESP_NONCE_LABEL: &[u8] = b"qstp:resp-nonce";
const SESSION_ID_LABEL: &[u8] = b"qstp:session-id";
const DIR_INIT_TO_RESP_LABEL: &[u8] = b"qstp:dir:init->resp";
const DIR_RESP_TO_INIT_LABEL: &[u8] = b"qstp:dir:resp->init";
const TUPLE_KEY_LABEL: &[u8] = b"qstp:tuple-key";

/// Direction of the tunnel endpoint when deriving session keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandshakeRole {
    Initiator,
    Responder,
}

/// Serialized handshake request emitted by the initiator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeInit {
    pub route_hash: [u8; 32],
    pub ciphertext: Bytes,
    pub initiator_nonce: [u8; 32],
    pub client_signature: Bytes,
    pub client_signing_key_id: KeyId,
    pub client_signing_public_key: Bytes,
    pub server_signing_key_id: KeyId,
    pub application_data: Bytes,
}

/// Response emitted by the responder after decapsulation and verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeResponse {
    pub route_hash: [u8; 32],
    pub session_id: [u8; 32],
    pub responder_nonce: [u8; 32],
    pub responder_signature: Bytes,
    pub server_signing_key_id: KeyId,
    pub server_signing_public_key: Bytes,
    pub server_kem_key_id: KeyId,
    pub server_kem_public_key: Bytes,
}

#[derive(Clone, Debug)]
pub struct InitiatorState {
    shared_secret: Bytes,
    route_hash: [u8; 32],
    initiator_nonce: [u8; 32],
    server_signing_key_id: KeyId,
    server_signing_public_key: Bytes,
}

#[derive(Clone, Debug)]
pub struct ResponderState {
    shared_secret: Bytes,
    route_hash: [u8; 32],
    initiator_nonce: [u8; 32],
    responder_nonce: [u8; 32],
    session_id: [u8; 32],
}

/// AES and TupleChain material derived from the shared secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionKeys {
    pub session_id: [u8; 32],
    pub send_key: [u8; 32],
    pub send_nonce: [u8; 12],
    pub recv_key: [u8; 32],
    pub recv_nonce: [u8; 12],
    pub tuple_key: [u8; 32],
}

pub enum DeriveSessionInput<'a> {
    Initiator {
        state: InitiatorState,
        request: &'a HandshakeInit,
        response: &'a HandshakeResponse,
        dsa: &'a MlDsaEngine,
    },
    Responder {
        state: ResponderState,
    },
}

pub struct InitHandshakeConfig<'a> {
    pub kem: &'a MlKemEngine,
    pub dsa: &'a MlDsaEngine,
    pub server_kem_public_key: &'a [u8],
    pub server_signing_public_key: &'a [u8],
    pub server_signing_key_id: KeyId,
    pub client_signing_secret_key: &'a [u8],
    pub client_signing_public_key: &'a [u8],
    pub client_signing_key_id: KeyId,
    pub route_hash: [u8; 32],
    pub application_data: &'a [u8],
}

pub struct RespondHandshakeConfig<'a> {
    pub kem: &'a MlKemEngine,
    pub dsa: &'a MlDsaEngine,
    pub server_kem_secret_key: &'a [u8],
    pub server_kem_public_key: &'a [u8],
    pub server_kem_key_id: KeyId,
    pub server_signing_secret_key: &'a [u8],
    pub server_signing_public_key: &'a [u8],
    pub server_signing_key_id: KeyId,
}

/// Initiate a Kyber/Dilithium handshake for a given route hash and application payload.
pub fn init_handshake(cfg: InitHandshakeConfig<'_>) -> PqcResult<(HandshakeInit, InitiatorState)> {
    let encapsulation = cfg.kem.encapsulate(cfg.server_kem_public_key)?;
    let ciphertext = encapsulation.ciphertext;
    let shared_secret = encapsulation.shared_secret;

    let initiator_nonce = derive_initiator_nonce(
        &cfg.route_hash,
        &ciphertext,
        cfg.application_data,
        &cfg.client_signing_key_id,
        &cfg.server_signing_key_id,
    );
    let transcript = build_client_transcript(
        &cfg.route_hash,
        cfg.application_data,
        &ciphertext,
        &initiator_nonce,
        &cfg.client_signing_key_id,
        &cfg.server_signing_key_id,
    );
    let signature = cfg.dsa.sign(cfg.client_signing_secret_key, &transcript)?;

    let init = HandshakeInit {
        route_hash: cfg.route_hash,
        ciphertext,
        initiator_nonce,
        client_signature: signature,
        client_signing_key_id: cfg.client_signing_key_id.clone(),
        client_signing_public_key: cfg.client_signing_public_key.to_vec(),
        server_signing_key_id: cfg.server_signing_key_id.clone(),
        application_data: cfg.application_data.to_vec(),
    };

    let state = InitiatorState {
        shared_secret,
        route_hash: init.route_hash,
        initiator_nonce: init.initiator_nonce,
        server_signing_key_id: init.server_signing_key_id.clone(),
        server_signing_public_key: cfg.server_signing_public_key.to_vec(),
    };

    Ok((init, state))
}

/// Verify the initiator payload, decapsulate the shared secret, and emit a response.
pub fn respond_handshake(
    cfg: RespondHandshakeConfig<'_>,
    request: &HandshakeInit,
) -> PqcResult<(HandshakeResponse, ResponderState)> {
    if request.server_signing_key_id != cfg.server_signing_key_id {
        return Err(PqcError::InvalidInput("server signing key mismatch"));
    }

    let expected_nonce = derive_initiator_nonce(
        &request.route_hash,
        &request.ciphertext,
        &request.application_data,
        &request.client_signing_key_id,
        &cfg.server_signing_key_id,
    );
    if expected_nonce != request.initiator_nonce {
        return Err(PqcError::VerifyFailed);
    }

    let transcript = build_client_transcript(
        &request.route_hash,
        &request.application_data,
        &request.ciphertext,
        &request.initiator_nonce,
        &request.client_signing_key_id,
        &request.server_signing_key_id,
    );
    cfg.dsa.verify(
        &request.client_signing_public_key,
        &transcript,
        &request.client_signature,
    )?;

    let shared_secret = cfg
        .kem
        .decapsulate(cfg.server_kem_secret_key, &request.ciphertext)?;

    let responder_nonce = derive_responder_nonce(
        &shared_secret,
        &request.route_hash,
        &request.initiator_nonce,
        &cfg.server_signing_key_id,
        &cfg.server_kem_key_id,
    );
    let session_id = derive_session_id(
        &shared_secret,
        &request.route_hash,
        &request.initiator_nonce,
        &responder_nonce,
    );

    let server_transcript = build_server_transcript(
        &request.route_hash,
        &request.initiator_nonce,
        &responder_nonce,
        &session_id,
        &cfg.server_signing_key_id,
        &request.client_signing_key_id,
    );
    let responder_signature = cfg
        .dsa
        .sign(cfg.server_signing_secret_key, &server_transcript)?;

    let response = HandshakeResponse {
        route_hash: request.route_hash,
        session_id,
        responder_nonce,
        responder_signature,
        server_signing_key_id: cfg.server_signing_key_id.clone(),
        server_signing_public_key: cfg.server_signing_public_key.to_vec(),
        server_kem_key_id: cfg.server_kem_key_id.clone(),
        server_kem_public_key: cfg.server_kem_public_key.to_vec(),
    };

    let state = ResponderState {
        shared_secret,
        route_hash: request.route_hash,
        initiator_nonce: request.initiator_nonce,
        responder_nonce,
        session_id: response.session_id,
    };

    debug_assert_eq!(response.session_id, session_id);
    debug_assert_eq!(state.session_id, response.session_id);

    Ok((response, state))
}

/// Derive send/receive and TupleChain keys for either handshake role.
pub fn derive_session_keys(input: DeriveSessionInput<'_>) -> PqcResult<SessionKeys> {
    match input {
        DeriveSessionInput::Initiator {
            state,
            request,
            response,
            dsa,
        } => {
            if response.route_hash != state.route_hash {
                return Err(PqcError::InvalidInput("route hash mismatch"));
            }
            if response.server_signing_key_id != state.server_signing_key_id {
                return Err(PqcError::InvalidInput("signing key mismatch"));
            }

            let InitiatorState {
                shared_secret,
                route_hash,
                initiator_nonce,
                server_signing_key_id,
                server_signing_public_key,
            } = state;

            let expected_nonce = derive_responder_nonce(
                &shared_secret,
                &route_hash,
                &initiator_nonce,
                &server_signing_key_id,
                &response.server_kem_key_id,
            );
            if expected_nonce != response.responder_nonce {
                return Err(PqcError::VerifyFailed);
            }

            let expected_session_id = derive_session_id(
                &shared_secret,
                &route_hash,
                &initiator_nonce,
                &response.responder_nonce,
            );
            if expected_session_id != response.session_id {
                return Err(PqcError::VerifyFailed);
            }

            let server_transcript = build_server_transcript(
                &route_hash,
                &initiator_nonce,
                &response.responder_nonce,
                &response.session_id,
                &server_signing_key_id,
                &request.client_signing_key_id,
            );
            dsa.verify(
                &server_signing_public_key,
                &server_transcript,
                &response.responder_signature,
            )?;

            Ok(derive_material(
                &shared_secret,
                &route_hash,
                &initiator_nonce,
                &response.responder_nonce,
                HandshakeRole::Initiator,
                response.session_id,
            ))
        }
        DeriveSessionInput::Responder { state } => {
            let ResponderState {
                shared_secret,
                route_hash,
                initiator_nonce,
                responder_nonce,
                session_id,
            } = state;
            Ok(derive_material(
                &shared_secret,
                &route_hash,
                &initiator_nonce,
                &responder_nonce,
                HandshakeRole::Responder,
                session_id,
            ))
        }
    }
}

fn derive_material(
    shared_secret: &[u8],
    route_hash: &[u8; 32],
    initiator_nonce: &[u8; 32],
    responder_nonce: &[u8; 32],
    role: HandshakeRole,
    session_id: [u8; 32],
) -> SessionKeys {
    let context = compose_context(route_hash, initiator_nonce, responder_nonce);
    let (send_label, recv_label) = match role {
        HandshakeRole::Initiator => (DIR_INIT_TO_RESP_LABEL, DIR_RESP_TO_INIT_LABEL),
        HandshakeRole::Responder => (DIR_RESP_TO_INIT_LABEL, DIR_INIT_TO_RESP_LABEL),
    };
    let send_material = kdf_expand(shared_secret, send_label, &context, 44);
    let recv_material = kdf_expand(shared_secret, recv_label, &context, 44);
    let tuple_material = kdf_expand(shared_secret, TUPLE_KEY_LABEL, &context, 32);

    let mut send_key = [0u8; 32];
    send_key.copy_from_slice(&send_material[..32]);
    let mut send_nonce = [0u8; 12];
    send_nonce.copy_from_slice(&send_material[32..44]);

    let mut recv_key = [0u8; 32];
    recv_key.copy_from_slice(&recv_material[..32]);
    let mut recv_nonce = [0u8; 12];
    recv_nonce.copy_from_slice(&recv_material[32..44]);

    let mut tuple_key = [0u8; 32];
    tuple_key.copy_from_slice(&tuple_material);

    SessionKeys {
        session_id,
        send_key,
        send_nonce,
        recv_key,
        recv_nonce,
        tuple_key,
    }
}

fn derive_initiator_nonce(
    route_hash: &[u8; 32],
    ciphertext: &[u8],
    application_data: &[u8],
    client_id: &KeyId,
    server_id: &KeyId,
) -> [u8; 32] {
    let mut hasher = Blake2s256::new();
    hasher.update(INIT_NONCE_LABEL);
    hasher.update(route_hash);
    hasher.update(ciphertext);
    hasher.update(application_data);
    hasher.update(&client_id.0);
    hasher.update(&server_id.0);
    digest_to_array(hasher.finalize())
}

fn derive_responder_nonce(
    shared_secret: &[u8],
    route_hash: &[u8; 32],
    initiator_nonce: &[u8; 32],
    server_signing_id: &KeyId,
    server_kem_id: &KeyId,
) -> [u8; 32] {
    let mut hasher = Blake2s256::new();
    hasher.update(RESP_NONCE_LABEL);
    hasher.update(shared_secret);
    hasher.update(route_hash);
    hasher.update(initiator_nonce);
    hasher.update(&server_signing_id.0);
    hasher.update(&server_kem_id.0);
    digest_to_array(hasher.finalize())
}

fn derive_session_id(
    shared_secret: &[u8],
    route_hash: &[u8; 32],
    initiator_nonce: &[u8; 32],
    responder_nonce: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Blake2s256::new();
    hasher.update(SESSION_ID_LABEL);
    hasher.update(shared_secret);
    hasher.update(route_hash);
    hasher.update(initiator_nonce);
    hasher.update(responder_nonce);
    digest_to_array(hasher.finalize())
}

fn build_client_transcript(
    route_hash: &[u8; 32],
    application_data: &[u8],
    ciphertext: &[u8],
    initiator_nonce: &[u8; 32],
    client_id: &KeyId,
    server_id: &KeyId,
) -> Bytes {
    let mut transcript = Bytes::with_capacity(
        CLIENT_TRANSCRIPT_LABEL.len()
            + route_hash.len()
            + application_data.len()
            + ciphertext.len()
            + initiator_nonce.len()
            + client_id.0.len()
            + server_id.0.len(),
    );
    transcript.extend_from_slice(CLIENT_TRANSCRIPT_LABEL);
    transcript.extend_from_slice(route_hash);
    transcript.extend_from_slice(application_data);
    transcript.extend_from_slice(ciphertext);
    transcript.extend_from_slice(initiator_nonce);
    transcript.extend_from_slice(&client_id.0);
    transcript.extend_from_slice(&server_id.0);
    transcript
}

fn build_server_transcript(
    route_hash: &[u8; 32],
    initiator_nonce: &[u8; 32],
    responder_nonce: &[u8; 32],
    session_id: &[u8; 32],
    server_id: &KeyId,
    client_id: &KeyId,
) -> Bytes {
    let mut transcript = Bytes::with_capacity(
        SERVER_TRANSCRIPT_LABEL.len()
            + route_hash.len()
            + initiator_nonce.len()
            + responder_nonce.len()
            + session_id.len()
            + server_id.0.len()
            + client_id.0.len(),
    );
    transcript.extend_from_slice(SERVER_TRANSCRIPT_LABEL);
    transcript.extend_from_slice(route_hash);
    transcript.extend_from_slice(initiator_nonce);
    transcript.extend_from_slice(responder_nonce);
    transcript.extend_from_slice(session_id);
    transcript.extend_from_slice(&server_id.0);
    transcript.extend_from_slice(&client_id.0);
    transcript
}

fn compose_context(
    route_hash: &[u8; 32],
    initiator_nonce: &[u8; 32],
    responder_nonce: &[u8; 32],
) -> Bytes {
    let mut ctx = Bytes::with_capacity(32 + 32 + 32);
    ctx.extend_from_slice(route_hash);
    ctx.extend_from_slice(initiator_nonce);
    ctx.extend_from_slice(responder_nonce);
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
        let block = hasher.finalize();
        let block_bytes = block.as_slice();
        let take = cmp::min(out_len - output.len(), block_bytes.len());
        output.extend_from_slice(&block_bytes[..take]);
        counter = counter.wrapping_add(1);
    }
    output
}

fn digest_to_array(output: Output<Blake2s256>) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(output.as_slice());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{DemoMlDsa, DemoMlKem};
    use crate::dsa::MlDsaEngine;
    use crate::kem::MlKemEngine;
    use crate::runtime;
    use alloc::boxed::Box;
    use blake2::Blake2s256;
    use digest::Digest;

    fn init_state() {
        runtime::reset_state_for_tests();
    }

    #[test]
    fn handshake_serializes_full_artifacts() {
        init_state();
        let mut out = vec![0u8; 512];
        let request = b"client=unit-test&ts=1700000123456";

        let written = execute_handshake(request, &mut out).expect("handshake");
        assert!(written > HANDSHAKE_HEADER_LEN);

        assert_eq!(&out[..4], HANDSHAKE_MAGIC);
        assert_eq!(out[4], HANDSHAKE_VERSION);

        let ciphertext_len = u16::from_le_bytes([
            out[HANDSHAKE_HEADER_LEN - 10],
            out[HANDSHAKE_HEADER_LEN - 9],
        ]) as usize;
        assert!(ciphertext_len > 0);

        let shared_secret_len =
            u16::from_le_bytes([out[HANDSHAKE_HEADER_LEN - 8], out[HANDSHAKE_HEADER_LEN - 7]])
                as usize;
        assert_eq!(shared_secret_len, 32);
    }

    #[test]
    fn rejects_empty_request() {
        init_state();
        let mut out = vec![0u8; 64];
        let err = execute_handshake(&[], &mut out).unwrap_err();
        assert_eq!(err, PqcError::InvalidInput("handshake request is empty"));
    }

    #[test]
    fn propagates_small_buffer_error() {
        init_state();
        let mut out = vec![0u8; HANDSHAKE_HEADER_LEN - 1];
        let err = execute_handshake(b"req", &mut out).unwrap_err();
        assert_eq!(err, PqcError::LimitExceeded("response buffer too small"));
    }

    #[test]
    fn handshake_round_trip_matches_session_keys() {
        let initiator_kem = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let responder_kem = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let initiator_dsa = MlDsaEngine::new(Box::new(DemoMlDsa::new()));
        let responder_dsa = MlDsaEngine::new(Box::new(DemoMlDsa::new()));

        let server_kem = responder_kem.keygen().expect("kem keypair");
        let server_kem_id = key_id_from(&server_kem.public_key, 1);

        let client_sign = initiator_dsa.keygen().expect("client sign key");
        let client_sign_id = key_id_from(&client_sign.public_key, 11);
        let server_sign = responder_dsa.keygen().expect("server sign key");
        let server_sign_id = key_id_from(&server_sign.public_key, 21);

        let route_hash = route_digest("waku/demo");
        let payload = b"client=qstp-handshake&ts=1700000000000";

        let (request, init_state) = init_handshake(InitHandshakeConfig {
            kem: &initiator_kem,
            dsa: &initiator_dsa,
            server_kem_public_key: &server_kem.public_key,
            server_signing_public_key: &server_sign.public_key,
            server_signing_key_id: server_sign_id.clone(),
            client_signing_secret_key: &client_sign.secret_key,
            client_signing_public_key: &client_sign.public_key,
            client_signing_key_id: client_sign_id.clone(),
            route_hash,
            application_data: payload,
        })
        .expect("initiate handshake");

        let (response, responder_state) = respond_handshake(
            RespondHandshakeConfig {
                kem: &responder_kem,
                dsa: &responder_dsa,
                server_kem_secret_key: &server_kem.secret_key,
                server_kem_public_key: &server_kem.public_key,
                server_kem_key_id: server_kem_id.clone(),
                server_signing_secret_key: &server_sign.secret_key,
                server_signing_public_key: &server_sign.public_key,
                server_signing_key_id: server_sign_id.clone(),
            },
            &request,
        )
        .expect("respond handshake");

        assert_eq!(response.session_id, responder_state.session_id);

        let initiator_keys = derive_session_keys(DeriveSessionInput::Initiator {
            state: init_state,
            request: &request,
            response: &response,
            dsa: &initiator_dsa,
        })
        .expect("derive initiator keys");
        let responder_keys = derive_session_keys(DeriveSessionInput::Responder {
            state: responder_state,
        })
        .expect("derive responder keys");

        assert_eq!(initiator_keys.session_id, responder_keys.session_id);
        assert_eq!(initiator_keys.send_key, responder_keys.recv_key);
        assert_eq!(initiator_keys.recv_key, responder_keys.send_key);
        assert_eq!(initiator_keys.tuple_key, responder_keys.tuple_key);
    }

    #[test]
    fn respond_handshake_rejects_tampered_signature() {
        let initiator_kem = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let responder_kem = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let initiator_dsa = MlDsaEngine::new(Box::new(DemoMlDsa::new()));
        let responder_dsa = MlDsaEngine::new(Box::new(DemoMlDsa::new()));

        let server_kem = responder_kem.keygen().expect("kem keypair");
        let server_kem_id = key_id_from(&server_kem.public_key, 2);
        let client_sign = initiator_dsa.keygen().expect("client sign key");
        let client_sign_id = key_id_from(&client_sign.public_key, 3);
        let server_sign = responder_dsa.keygen().expect("server sign key");
        let server_sign_id = key_id_from(&server_sign.public_key, 4);

        let route_hash = route_digest("waku/tamper");
        let payload = b"client=bad-sig";

        let (mut request, _) = init_handshake(InitHandshakeConfig {
            kem: &initiator_kem,
            dsa: &initiator_dsa,
            server_kem_public_key: &server_kem.public_key,
            server_signing_public_key: &server_sign.public_key,
            server_signing_key_id: server_sign_id.clone(),
            client_signing_secret_key: &client_sign.secret_key,
            client_signing_public_key: &client_sign.public_key,
            client_signing_key_id: client_sign_id,
            route_hash,
            application_data: payload,
        })
        .expect("initiate handshake");

        request.client_signature[0] ^= 0x01;

        let err = respond_handshake(
            RespondHandshakeConfig {
                kem: &responder_kem,
                dsa: &responder_dsa,
                server_kem_secret_key: &server_kem.secret_key,
                server_kem_public_key: &server_kem.public_key,
                server_kem_key_id: server_kem_id,
                server_signing_secret_key: &server_sign.secret_key,
                server_signing_public_key: &server_sign.public_key,
                server_signing_key_id: server_sign_id,
            },
            &request,
        )
        .unwrap_err();
        assert_eq!(err, PqcError::VerifyFailed);
    }

    #[test]
    fn derive_session_keys_rejects_bad_server_signature() {
        let initiator_kem = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let responder_kem = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let initiator_dsa = MlDsaEngine::new(Box::new(DemoMlDsa::new()));
        let responder_dsa = MlDsaEngine::new(Box::new(DemoMlDsa::new()));

        let server_kem = responder_kem.keygen().expect("kem keypair");
        let server_kem_id = key_id_from(&server_kem.public_key, 5);
        let client_sign = initiator_dsa.keygen().expect("client sign key");
        let client_sign_id = key_id_from(&client_sign.public_key, 6);
        let server_sign = responder_dsa.keygen().expect("server sign key");
        let server_sign_id = key_id_from(&server_sign.public_key, 7);

        let route_hash = route_digest("waku/verify");
        let payload = b"client=verify-server";

        let (request, init_state) = init_handshake(InitHandshakeConfig {
            kem: &initiator_kem,
            dsa: &initiator_dsa,
            server_kem_public_key: &server_kem.public_key,
            server_signing_public_key: &server_sign.public_key,
            server_signing_key_id: server_sign_id.clone(),
            client_signing_secret_key: &client_sign.secret_key,
            client_signing_public_key: &client_sign.public_key,
            client_signing_key_id: client_sign_id.clone(),
            route_hash,
            application_data: payload,
        })
        .expect("initiate handshake");

        let (mut response, responder_state) = respond_handshake(
            RespondHandshakeConfig {
                kem: &responder_kem,
                dsa: &responder_dsa,
                server_kem_secret_key: &server_kem.secret_key,
                server_kem_public_key: &server_kem.public_key,
                server_kem_key_id: server_kem_id,
                server_signing_secret_key: &server_sign.secret_key,
                server_signing_public_key: &server_sign.public_key,
                server_signing_key_id: server_sign_id.clone(),
            },
            &request,
        )
        .expect("respond handshake");

        response.responder_signature[0] ^= 0x80;

        let err = derive_session_keys(DeriveSessionInput::Initiator {
            state: init_state,
            request: &request,
            response: &response,
            dsa: &initiator_dsa,
        })
        .unwrap_err();
        assert_eq!(err, PqcError::VerifyFailed);

        // Responder side should still be able to derive since signature tampering happens after response.
        let responder_keys = derive_session_keys(DeriveSessionInput::Responder {
            state: responder_state,
        })
        .expect("responder derives");
        assert_eq!(responder_keys.session_id, response.session_id);
    }

    fn key_id_from(seed: &[u8], salt: u64) -> KeyId {
        let mut hasher = Blake2s256::new();
        hasher.update(seed);
        hasher.update(salt.to_le_bytes());
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(digest.as_slice());
        KeyId(out)
    }

    fn route_digest(label: &str) -> [u8; 32] {
        let mut hasher = Blake2s256::new();
        hasher.update(label.as_bytes());
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(digest.as_slice());
        out
    }
}
