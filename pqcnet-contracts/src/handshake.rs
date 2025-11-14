//! Minimal handshake facade exported to WASM hosts.
//!
//! The current implementation is intentionally simple: it derives a 32-byte
//! digest over the caller-provided request payload and returns it to the host.
//! This keeps the ABI stable while the underlying PQC wiring (Kyber/Dilithium)
//! is brought online.

use crate::error::{PqcError, PqcResult};
use blake2::Blake2s256;
use digest::Digest;

/// Fixed length of the handshake response written back to the host.
pub const HANDSHAKE_RESPONSE_BYTES: usize = 32;

/// Deterministically derive a handshake response from the provided request.
///
/// * `request` – Arbitrary bytes supplied by the host (nonce, transcript, etc.).
/// * `response` – Caller-provided buffer that will receive 32 bytes of output.
///
/// Returns the number of bytes written (always 32) or an error if the input is
/// empty / the response buffer is undersized.
pub fn execute_handshake(request: &[u8], response: &mut [u8]) -> PqcResult<usize> {
    if request.is_empty() {
        return Err(PqcError::InvalidInput("handshake request is empty"));
    }
    if response.len() < HANDSHAKE_RESPONSE_BYTES {
        return Err(PqcError::LimitExceeded("response buffer too small"));
    }

    let mut hasher = Blake2s256::new();
    hasher.update(b"PQCNET_HANDSHAKE_V0");
    hasher.update(request);
    let digest = hasher.finalize();

    response[..HANDSHAKE_RESPONSE_BYTES].copy_from_slice(&digest[..HANDSHAKE_RESPONSE_BYTES]);
    Ok(HANDSHAKE_RESPONSE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_is_deterministic() {
        let mut out = [0u8; HANDSHAKE_RESPONSE_BYTES];
        let mut out2 = [0u8; HANDSHAKE_RESPONSE_BYTES];
        let request = b"demo-client-nonce";

        let len = execute_handshake(request, &mut out).unwrap();
        let len2 = execute_handshake(request, &mut out2).unwrap();

        assert_eq!(len, HANDSHAKE_RESPONSE_BYTES);
        assert_eq!(len2, HANDSHAKE_RESPONSE_BYTES);
        assert_eq!(out.to_vec(), out2.to_vec());
    }

    #[test]
    fn rejects_empty_request() {
        let mut out = [0u8; HANDSHAKE_RESPONSE_BYTES];
        let err = execute_handshake(&[], &mut out).unwrap_err();
        assert_eq!(err, PqcError::InvalidInput("handshake request is empty"));
    }

    #[test]
    fn rejects_small_response_buffer() {
        let mut out = [0u8; HANDSHAKE_RESPONSE_BYTES - 1];
        let err = execute_handshake(b"req", &mut out).unwrap_err();
        assert_eq!(err, PqcError::LimitExceeded("response buffer too small"));
    }
}
