#[cfg(feature = "sim")]
use core::ops::RangeInclusive;

#[cfg(feature = "sim")]
use pqcnet_entropy::{EntropyError, EntropySource, SimEntropySource};

#[cfg(feature = "sim")]
const ENTROPY_PANIC: &str =
    "QRNG host entropy bridge failed - ensure the runtime wires autheo_host_entropy correctly";

#[cfg(feature = "sim")]
pub struct QrngEntropyRng {
    source: SimEntropySource,
}

#[cfg(feature = "sim")]
impl QrngEntropyRng {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            source: SimEntropySource::with_seed(seed),
        }
    }

    pub fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
        self.source.try_fill_bytes(dest)
    }

    #[track_caller]
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.try_fill_bytes(dest)
            .unwrap_or_else(|err| panic!("{ENTROPY_PANIC}: {err:?}"));
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
        if start == end {
            return start;
        }
        let span = end.saturating_sub(start);
        start + (self.next_u64() % (span.saturating_add(1)))
    }

    pub fn gen_range_f64(&mut self, range: RangeInclusive<f64>) -> f64 {
        let start = *range.start();
        let end = *range.end();
        if start == end {
            return start;
        }
        let unit = (self.next_u64() as f64) / (u64::MAX as f64);
        start + (end - start) * unit
    }
}

#[cfg(feature = "sim")]
impl EntropySource for QrngEntropyRng {
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), EntropyError> {
        self.source.try_fill_bytes(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "sim")]
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
}
