use core::ops::RangeInclusive;

#[cfg(all(not(target_arch = "wasm32"), feature = "qrng-sim"))]
use autheo_pqcnet_qrng::{EntropyRequest, QrngSim};
use thiserror::Error;

#[cfg(all(not(target_arch = "wasm32"), not(feature = "qrng-sim")))]
compile_error!(
    "`qrng-sim` must remain enabled for non-wasm targets so 5DQEH can synthesize host entropy"
);

const CACHE_BYTES: usize = 1024;
const QRNG_MIN_BITS: usize = 256;
const QRNG_MAX_BITS: usize = 8_192;
const ENTROPY_PANIC: &str =
    "QRNG host entropy bridge failed - ensure the runtime wires autheo_host_entropy correctly";

/// Errors emitted when the entropy backend is unavailable.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EntropyError {
    #[error("host environment rejected entropy import with code {0}")]
    HostRejected(i32),
    #[error("qrng entropy frame was empty")]
    EmptyFrame,
}

/// Minimal trait for consumers that need deterministic entropy.
pub trait EntropySource {
    fn fill_entropy(&mut self, dest: &mut [u8]);
}

/// RNG that backs the 5DQEH simulator by streaming entropy from the QRNG module (native)
/// or a WASM host import (wasm32).
pub struct QrngEntropyRng {
    backend: Backend,
    cache: [u8; CACHE_BYTES],
    cursor: usize,
}

impl QrngEntropyRng {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            backend: Backend::new(seed),
            cache: [0u8; CACHE_BYTES],
            cursor: CACHE_BYTES,
        }
    }

    fn refill(&mut self) -> Result<(), EntropyError> {
        self.backend.fill(&mut self.cache)?;
        self.cursor = 0;
        Ok(())
    }

    /// Try to fill the requested slice with entropy, returning an error instead of panicking.
    pub fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
        if dest.is_empty() {
            return Ok(());
        }

        let mut offset = 0usize;
        while offset < dest.len() {
            if self.cursor >= self.cache.len() {
                self.refill()?;
            }
            let available = self.cache.len() - self.cursor;
            let take = available.min(dest.len() - offset);
            dest[offset..offset + take]
                .copy_from_slice(&self.cache[self.cursor..self.cursor + take]);
            self.cursor += take;
            offset += take;
        }
        Ok(())
    }

    /// Fill the slice with entropy, panicking if the host bridge is misconfigured.
    #[track_caller]
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.try_fill_bytes(dest)
            .unwrap_or_else(|err| panic!("{ENTROPY_PANIC}: {err}"));
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        self.fill_bytes(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    pub fn gen_bool(&mut self, probability: f64) -> bool {
        if probability <= 0.0 {
            return false;
        }
        if probability >= 1.0 {
            return true;
        }
        let threshold = (probability * u64::MAX as f64).clamp(0.0, u64::MAX as f64) as u64;
        self.next_u64() <= threshold
    }

    pub fn gen_range_u64(&mut self, range: RangeInclusive<u64>) -> u64 {
        let start = *range.start();
        let end = *range.end();
        debug_assert!(
            end >= start,
            "gen_range_u64 requires start <= end (start={start}, end={end})"
        );
        if start == end {
            return start;
        }
        let span = end.wrapping_sub(start);
        let sample = self.next_u64();
        start.wrapping_add(sample % (span.saturating_add(1)))
    }

    pub fn gen_range_f64(&mut self, range: RangeInclusive<f64>) -> f64 {
        let start = *range.start();
        let end = *range.end();
        debug_assert!(
            end >= start,
            "gen_range_f64 requires start <= end (start={start}, end={end})"
        );
        if start == end {
            return start;
        }
        let unit = (self.next_u64() as f64) / (u64::MAX as f64);
        start + (end - start) * unit
    }
}

impl EntropySource for QrngEntropyRng {
    fn fill_entropy(&mut self, dest: &mut [u8]) {
        self.fill_bytes(dest);
    }
}

enum Backend {
    #[cfg(all(not(target_arch = "wasm32"), feature = "qrng-sim"))]
    Sim(QrngSimBackend),
    #[cfg(target_arch = "wasm32")]
    Host(HostImportBackend),
}

impl Backend {
    fn new(seed: u64) -> Self {
        #[cfg(all(not(target_arch = "wasm32"), feature = "qrng-sim"))]
        {
            Self::Sim(QrngSimBackend::new(seed))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::Host(HostImportBackend::new(seed))
        }
    }

    fn fill(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
        match self {
            #[cfg(all(not(target_arch = "wasm32"), feature = "qrng-sim"))]
            Backend::Sim(sim) => sim.fill(dest),
            #[cfg(target_arch = "wasm32")]
            Backend::Host(host) => host.fill(dest),
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "qrng-sim"))]
struct QrngSimBackend {
    sim: QrngSim,
    label: String,
    sequence: u64,
}

#[cfg(all(not(target_arch = "wasm32"), feature = "qrng-sim"))]
impl QrngSimBackend {
    fn new(seed: u64) -> Self {
        Self {
            sim: QrngSim::new(seed),
            label: format!("5dqeh-host-{seed:x}"),
            sequence: 0,
        }
    }

    fn fill(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
        if dest.is_empty() {
            return Ok(());
        }
        let bits = (dest.len() * 8)
            .clamp(QRNG_MIN_BITS, QRNG_MAX_BITS)
            .try_into()
            .unwrap_or(QRNG_MAX_BITS as u16);
        let reference = format!("entropy-cache-{}", self.sequence);
        self.sequence = self.sequence.wrapping_add(1);

        let request = EntropyRequest::for_icosuple(self.label.clone(), bits, reference);
        let telemetry = self.sim.run_epoch(&[request]);
        let frame = telemetry
            .frames
            .first()
            .ok_or(EntropyError::EmptyFrame)?;
        expand_entropy(dest, &frame.entropy);
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
struct HostImportBackend {
    _seed_hint: u64,
}

#[cfg(target_arch = "wasm32")]
impl HostImportBackend {
    fn new(seed: u64) -> Self {
        Self { _seed_hint: seed }
    }

    fn fill(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
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

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "autheo")]
extern "C" {
    fn autheo_host_entropy(ptr: *mut u8, len: usize) -> i32;
}

fn expand_entropy(dest: &mut [u8], entropy: &[u8]) {
    if entropy.is_empty() {
        return;
    }
    if entropy.len() >= dest.len() {
        dest.copy_from_slice(&entropy[..dest.len()]);
        return;
    }
    let mut offset = 0usize;
    while offset < dest.len() {
        let take = entropy.len().min(dest.len() - offset);
        dest[offset..offset + take].copy_from_slice(&entropy[..take]);
        offset += take;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qrng_entropy_rng_is_deterministic_under_seed() {
        let mut rng = QrngEntropyRng::with_seed(0x5d0e);
        let mut bytes = [0u8; 32];
        rng.try_fill_bytes(&mut bytes).expect("entropy");
        assert_eq!(
            &bytes[..8],
            &[0x1c, 0xae, 0x46, 0x8a, 0x01, 0x22, 0x99, 0x95]
        );
    }

    #[test]
    fn gen_range_helpers_respect_bounds() {
        let mut rng = QrngEntropyRng::with_seed(0xabc);
        for _ in 0..64 {
            let value = rng.gen_range_u64(10..=20);
            assert!((10..=20).contains(&value));
            let float = rng.gen_range_f64(0.25..=0.75);
            assert!(float >= 0.25 && float <= 0.75);
        }
    }
}
