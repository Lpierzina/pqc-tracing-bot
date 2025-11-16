use crate::error::{PqcError, PqcResult};
use crate::key_manager::ThresholdPolicy;
use crate::types::{Bytes, KeyId, TimestampMs};
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareMetadata {
    pub key_id: KeyId,
    pub key_version: u32,
    pub created_at: TimestampMs,
    pub threshold: ThresholdPolicy,
    pub share_index: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretShare {
    pub metadata: ShareMetadata,
    pub value: Bytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretSharePackage {
    pub key_id: KeyId,
    pub key_version: u32,
    pub created_at: TimestampMs,
    pub threshold: ThresholdPolicy,
    pub shares: Vec<SecretShare>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveredSecret {
    pub key_id: KeyId,
    pub key_version: u32,
    pub created_at: TimestampMs,
    pub threshold: ThresholdPolicy,
    pub secret: Bytes,
}

#[cfg(not(target_arch = "wasm32"))]
use alloc::string::String;

#[cfg(not(target_arch = "wasm32"))]
use shamir::{SecretData, ShamirError};

#[cfg(not(target_arch = "wasm32"))]
pub fn split_secret(
    secret_key: &[u8],
    key_id: &KeyId,
    key_version: u32,
    created_at: TimestampMs,
    threshold: ThresholdPolicy,
) -> PqcResult<SecretSharePackage> {
    validate_secret(secret_key)?;
    validate_threshold(threshold)?;

    let encoded = encode_secret(secret_key);
    let secret_data = SecretData::with_secret(&encoded, threshold.t);

    let mut shares = Vec::with_capacity(threshold.n as usize);
    for index in 1..=threshold.n {
        let raw_share = secret_data.get_share(index).map_err(map_shamir_error)?;
        let metadata = ShareMetadata {
            key_id: key_id.clone(),
            key_version,
            created_at,
            threshold,
            share_index: index,
        };
        shares.push(SecretShare {
            metadata,
            value: raw_share,
        });
    }

    Ok(SecretSharePackage {
        key_id: key_id.clone(),
        key_version,
        created_at,
        threshold,
        shares,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn combine_secret(shares: &[SecretShare]) -> PqcResult<RecoveredSecret> {
    if shares.is_empty() {
        return Err(PqcError::InvalidInput("no shares provided"));
    }

    let reference = &shares[0].metadata;
    validate_threshold(reference.threshold)?;
    let minimum = reference.threshold.t as usize;
    if shares.len() < minimum {
        return Err(PqcError::ThresholdFailure(
            "insufficient shares to reconstruct secret",
        ));
    }

    let mut seen = [false; 256];
    let mut payloads: Vec<Vec<u8>> = Vec::with_capacity(shares.len());
    for share in shares {
        ensure_metadata_matches(reference, &share.metadata)?;
        let idx = share.metadata.share_index as usize;
        if idx == 0 || seen[idx] {
            return Err(PqcError::InvalidInput("duplicate share index detected"));
        }
        seen[idx] = true;
        payloads.push(share.value.clone());
    }

    let recovered = SecretData::recover_secret(reference.threshold.t, payloads)
        .ok_or(PqcError::ThresholdFailure("shamir reconstruction failed"))?;
    let secret = decode_secret(&recovered)?;

    Ok(RecoveredSecret {
        key_id: reference.key_id.clone(),
        key_version: reference.key_version,
        created_at: reference.created_at,
        threshold: reference.threshold,
        secret,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn validate_secret(secret_key: &[u8]) -> PqcResult<()> {
    if secret_key.is_empty() {
        return Err(PqcError::InvalidInput("secret key cannot be empty"));
    }
    Ok(())
}

fn validate_threshold(policy: ThresholdPolicy) -> PqcResult<()> {
    if policy.t == 0 || policy.n == 0 {
        return Err(PqcError::InvalidInput(
            "threshold parameters must be non-zero",
        ));
    }
    if policy.t > policy.n {
        return Err(PqcError::InvalidInput(
            "threshold minimum cannot exceed share count",
        ));
    }
    if policy.n > u8::MAX {
        return Err(PqcError::InvalidInput("share count cannot exceed 255"));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_metadata_matches(reference: &ShareMetadata, candidate: &ShareMetadata) -> PqcResult<()> {
    if reference.key_id != candidate.key_id
        || reference.key_version != candidate.key_version
        || reference.created_at != candidate.created_at
        || reference.threshold != candidate.threshold
    {
        return Err(PqcError::InvalidInput(
            "share metadata mismatch prevents reconstruction",
        ));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn encode_secret(secret: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(secret.len() * 2);
    for byte in secret {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_secret(payload: &str) -> PqcResult<Bytes> {
    let bytes = payload.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(PqcError::InternalError(
            "encoded secret contains an odd number of nibbles",
        ));
    }

    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks(2) {
        let upper = decode_nibble(chunk[0])?;
        let lower = decode_nibble(chunk[1])?;
        out.push((upper << 4) | lower);
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_nibble(byte: u8) -> PqcResult<u8> {
    let nibble = match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => {
            return Err(PqcError::InvalidInput(
                "encoded secret contains invalid hex",
            ));
        }
    };
    Ok(nibble)
}

#[cfg(not(target_arch = "wasm32"))]
fn map_shamir_error(err: ShamirError) -> PqcError {
    match err {
        ShamirError::InvalidShareCount => {
            PqcError::ThresholdFailure("shamir rejected share identifier")
        }
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::types::KeyId;

    fn key_id(byte: u8) -> KeyId {
        KeyId([byte; 32])
    }

    #[test]
    fn split_and_combine_two_of_three() {
        let secret = vec![0x01, 0x02, 0x03, 0x04];
        let policy = ThresholdPolicy { t: 2, n: 3 };
        let package = split_secret(&secret, &key_id(0xAA), 7, 111, policy).unwrap();
        assert_eq!(package.shares.len(), 3);
        assert_eq!(package.threshold, policy);

        let recovered =
            combine_secret(&package.shares[..2]).expect("reconstruction with 2 shares succeeds");
        assert_eq!(recovered.secret, secret);
        assert_eq!(recovered.key_version, 7);
    }

    #[test]
    fn split_and_combine_three_of_five() {
        let secret = (0..32).collect::<Vec<_>>();
        let policy = ThresholdPolicy { t: 3, n: 5 };
        let package = split_secret(&secret, &key_id(0xBB), 9, 222, policy).unwrap();

        let recovered =
            combine_secret(&package.shares[..3]).expect("reconstruction with threshold shares");
        assert_eq!(recovered.secret, secret);
        assert_eq!(recovered.threshold, policy);
    }

    #[test]
    fn combining_mismatched_metadata_fails() {
        let secret = vec![42u8; 32];
        let policy = ThresholdPolicy { t: 2, n: 3 };
        let mut package = split_secret(&secret, &key_id(0xCC), 1, 333, policy).unwrap();
        package.shares[1].metadata.key_version = 99;

        let err = combine_secret(&package.shares[..2]).unwrap_err();
        assert_eq!(
            err,
            PqcError::InvalidInput("share metadata mismatch prevents reconstruction")
        );
    }
}
