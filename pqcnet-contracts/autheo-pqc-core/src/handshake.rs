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

use crate::error::{PqcError, PqcResult};
use crate::key_manager::{KemKeyState, ThresholdPolicy};
use crate::runtime;
use crate::signatures::DsaKeyState;
use crate::types::{Bytes, KeyId, SecurityLevel, TimestampMs};

const HANDSHAKE_MAGIC: &[u8; 4] = b"PQC1";
const HANDSHAKE_VERSION: u8 = 1;
const HANDSHAKE_HEADER_LEN: usize = 4  // magic
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
    let artifacts = runtime::with_contract_state(|state| -> PqcResult<HandshakeArtifacts> {
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
        })
    })?;

    serialize_handshake(&artifacts, response)
}

fn serialize_handshake(artifacts: &HandshakeArtifacts, out: &mut [u8]) -> PqcResult<usize> {
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

struct HandshakeArtifacts {
    threshold: ThresholdPolicy,
    kem_state: KemKeyState,
    signing_state: DsaKeyState,
    ciphertext: Bytes,
    shared_secret: Bytes,
    signature: Bytes,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime;

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
}
