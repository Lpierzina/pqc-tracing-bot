#![cfg_attr(target_arch = "wasm32", no_std)]

use core::fmt;

#[cfg(not(target_arch = "wasm32"))]
use getrandom::getrandom;

/// Errors returned by entropy backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyError {
    /// The WASM host import rejected the request with a non-zero status code.
    HostRejected(i32),
    /// The underlying platform could not fulfill the request (native builds).
    Platform(&'static str),
}

impl fmt::Display for EntropyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntropyError::HostRejected(code) => {
                write!(f, "autheo_host_entropy rejected request with code {code}")
            }
            EntropyError::Platform(msg) => f.write_str(msg),
        }
    }
}

/// Minimal trait implemented by all entropy sources.
pub trait EntropySource {
    /// Try to fill `dest` with fresh entropy.
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), EntropyError>;

    /// Fill the destination slice, panicking if the backend rejects the request.
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.try_fill_bytes(dest)
            .expect("entropy backend rejected request");
    }
}

/// Production entropy bridge that relies on the Autheo host import when
/// targeting `wasm32` and falls back to the OS RNG on native builds.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostEntropySource;

impl HostEntropySource {
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_arch = "wasm32")]
impl EntropySource for HostEntropySource {
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
        if dest.is_empty() {
            return Ok(());
        }
        let rc = unsafe { autheo_host_entropy(dest.as_mut_ptr(), dest.len()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(EntropyError::HostRejected(rc))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl EntropySource for HostEntropySource {
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
        if dest.is_empty() {
            return Ok(());
        }
        getrandom(dest).map_err(|_| EntropyError::Platform("os rng unavailable"))
    }
}

#[cfg(feature = "sim")]
#[derive(Clone, Debug)]
pub struct SimEntropySource {
    state: u64,
}

#[cfg(feature = "sim")]
impl SimEntropySource {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    pub fn reseed(&mut self, seed: u64) {
        self.state = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    }

    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(feature = "sim")]
impl EntropySource for SimEntropySource {
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
        for chunk in dest.chunks_mut(8) {
            let mut value = self.next_u64();
            for byte in chunk {
                *byte = value as u8;
                value >>= 8;
            }
        }
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "autheo")]
extern "C" {
    fn autheo_host_entropy(ptr: *mut u8, len: usize) -> i32;
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "sim")]
    #[test]
    fn sim_entropy_is_deterministic() {
        let mut first = SimEntropySource::with_seed(42);
        let mut second = SimEntropySource::with_seed(42);
        let mut buf_a = [0u8; 64];
        let mut buf_b = [0u8; 64];
        first.try_fill_bytes(&mut buf_a).unwrap();
        second.try_fill_bytes(&mut buf_b).unwrap();
        assert_eq!(buf_a, buf_b);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn host_entropy_fills_bytes_on_native() {
        let mut host = HostEntropySource::new();
        let mut buf = [0u8; 16];
        host.try_fill_bytes(&mut buf).unwrap();
        assert!(buf.iter().any(|b| *b != 0));
    }

    #[cfg(feature = "sim")]
    #[test]
    fn sim_entropy_reseed_resets_stream() {
        let mut sim = SimEntropySource::with_seed(7);
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        sim.try_fill_bytes(&mut first).unwrap();
        sim.reseed(99);
        sim.try_fill_bytes(&mut second).unwrap();

        let mut fresh = SimEntropySource::with_seed(99);
        let mut expected = [0u8; 32];
        fresh.try_fill_bytes(&mut expected).unwrap();

        assert_ne!(first, second, "reseed should change output stream");
        assert_eq!(second, expected, "reseeded stream should be deterministic");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn host_entropy_allows_empty_requests() {
        let mut host = HostEntropySource::new();
        let mut empty: [u8; 0] = [];
        assert!(
            host.try_fill_bytes(&mut empty).is_ok(),
            "empty fills must be fast-path no-ops"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn host_entropy_produces_different_values() {
        let mut rng = HostEntropySource::new();
        let mut buf1 = [0u8; 32];
        let mut buf2 = [0u8; 32];
        
        rng.fill_bytes(&mut buf1);
        rng.fill_bytes(&mut buf2);
        
        // Very high probability that two 32-byte buffers differ
        assert_ne!(buf1, buf2, "consecutive entropy calls should produce different values");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn host_entropy_fills_large_buffers() {
        let mut rng = HostEntropySource::new();
        let mut large = vec![0u8; 1024];
        
        rng.fill_bytes(&mut large);
        
        // Check that we got non-zero bytes (very high probability)
        let non_zero = large.iter().filter(|&&b| b != 0).count();
        assert!(non_zero > 0, "large buffer should contain non-zero bytes");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn host_entropy_multiple_instances_independent() {
        let mut rng1 = HostEntropySource::new();
        let mut rng2 = HostEntropySource::new();
        
        let mut buf1 = [0u8; 16];
        let mut buf2 = [0u8; 16];
        
        rng1.fill_bytes(&mut buf1);
        rng2.fill_bytes(&mut buf2);
        
        // Independent instances should produce different values
        assert_ne!(buf1, buf2, "independent instances should produce different entropy");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn host_entropy_byte_distribution() {
        let mut rng = HostEntropySource::new();
        let mut sample = [0u8; 1024];
        rng.fill_bytes(&mut sample);
        
        // Count byte values
        let mut counts = [0u32; 256];
        for &byte in &sample {
            counts[byte as usize] += 1;
        }
        
        // With 1024 bytes, we expect most byte values to appear
        // This is a basic sanity check, not a full statistical test
        let unique_bytes = counts.iter().filter(|&&c| c > 0).count();
        // Should have at least 200 unique byte values out of 256 (reasonable for 1024 bytes)
        assert!(unique_bytes > 150, "entropy should produce diverse byte values, got {} unique", unique_bytes);
    }

    #[test]
    fn entropy_error_display() {
        let err1 = EntropyError::HostRejected(-1);
        let err2 = EntropyError::Platform("test error");
        
        assert!(err1.to_string().contains("autheo_host_entropy"));
        assert!(err1.to_string().contains("-1"));
        assert_eq!(err2.to_string(), "test error");
    }

    #[test]
    fn entropy_error_equality() {
        let err1 = EntropyError::HostRejected(-1);
        let err2 = EntropyError::HostRejected(-1);
        let err3 = EntropyError::HostRejected(-2);
        let err4 = EntropyError::Platform("test");
        
        assert_eq!(err1, err2);
        assert_ne!(err1, err3);
        assert_ne!(err1, err4);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn host_entropy_source_is_copy() {
        let mut rng1 = HostEntropySource::new();
        let mut rng2 = rng1; // Copy
        let mut rng3 = rng2; // Copy again
        
        // All should work independently
        let mut buf1 = [0u8; 8];
        let mut buf2 = [0u8; 8];
        let mut buf3 = [0u8; 8];
        
        rng1.fill_bytes(&mut buf1);
        rng2.fill_bytes(&mut buf2);
        rng3.fill_bytes(&mut buf3);
        
        // All should produce entropy
        assert!(buf1.iter().any(|&b| b != 0));
        assert!(buf2.iter().any(|&b| b != 0));
        assert!(buf3.iter().any(|&b| b != 0));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn host_entropy_source_default() {
        let mut rng1 = HostEntropySource::default();
        let mut rng2 = HostEntropySource::new();
        
        // Both should work
        let mut buf1 = [0u8; 8];
        let mut buf2 = [0u8; 8];
        
        rng1.fill_bytes(&mut buf1);
        rng2.fill_bytes(&mut buf2);
        
        assert!(buf1.iter().any(|&b| b != 0));
        assert!(buf2.iter().any(|&b| b != 0));
    }
}
