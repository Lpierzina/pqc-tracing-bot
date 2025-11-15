use alloc::vec::Vec;

/// Owned byte buffer for PQC operations.
pub type Bytes = Vec<u8>;

/// Millisecond-resolution timestamp.
pub type TimestampMs = u64;

/// Logical key identifier inside PQCNet (e.g., hash of pk + metadata).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyId(pub [u8; 32]);

/// Logical DAG edge identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeId(pub [u8; 32]);

/// Security level tags (e.g., 128-bit PQ security).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityLevel {
    /// ML-KEM-512 or ML-DSA-44 style security (~128-bit PQ).
    MlKem128,
    /// ML-KEM-768 (~192-bit PQ).
    MlKem192,
    /// ML-KEM-1024 (~256-bit PQ).
    MlKem256,
    /// ML-DSA-44 (~128-bit PQ).
    MlDsa128,
    /// ML-DSA-65 (~192-bit PQ).
    MlDsa192,
    /// ML-DSA-87 (~256-bit PQ).
    MlDsa256,
}
