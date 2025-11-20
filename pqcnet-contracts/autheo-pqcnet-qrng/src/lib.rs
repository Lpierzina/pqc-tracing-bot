//! Quantum random number generator harness for Autheo PQCNet.
//!
//! The QRNG module mixes multiple physical-style entropy sources (photon shot noise, vacuum
//! fluctuations, fallback synthetic noise) and binds the resulting randomness to a PQC envelope so it
//! can be hoisted into its own repository later. The simulation intentionally references Kyber and
//! Dilithium security levels to mirror what downstream tuplechain / icosuple components expect.

use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha3::digest::{ExtendableOutput, Update, XofReader};
use sha3::{Digest, Sha3_256, Shake256};
use std::time::{SystemTime, UNIX_EPOCH};

const MIN_BITS: u16 = 256;
const MAX_BITS: u16 = 8192;

/// Describes how much entropy a downstream consumer needs for a given tuple / icosuple operation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntropyRequest {
    /// Logical label so telemetry can map entropy back to workloads (tuplechain, qstp, etc.).
    pub label: String,
    /// Requested entropy size in bits.
    pub bits: u16,
    /// Optional reference to the icosuple or tuple the randomness belongs to.
    pub icosuple_reference: String,
    /// Kyber security level expected by the consumer (1, 3, or 5 in the PQC spec).
    pub kyber_level: u8,
    /// Dilithium security level expected by the consumer (1, 3, or 5).
    pub dilithium_level: u8,
}

impl EntropyRequest {
    /// Build a new entropy request scoped to a label.
    pub fn new(label: impl Into<String>, bits: u16) -> Self {
        Self {
            label: label.into(),
            bits,
            icosuple_reference: String::new(),
            kyber_level: 5,
            dilithium_level: 5,
        }
    }

    /// Helper for tuple / icosuple bound requests.
    pub fn for_icosuple(
        label: impl Into<String>,
        bits: u16,
        icosuple_reference: impl Into<String>,
    ) -> Self {
        Self {
            icosuple_reference: icosuple_reference.into(),
            ..Self::new(label, bits)
        }
    }

    /// Override Kyber / Dilithium security levels.
    pub fn with_security(mut self, kyber_level: u8, dilithium_level: u8) -> Self {
        self.kyber_level = kyber_level;
        self.dilithium_level = dilithium_level;
        self
    }

    /// Override the icosuple reference after construction.
    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.icosuple_reference = reference.into();
        self
    }

    fn normalized_bits(&self) -> u16 {
        self.bits.clamp(MIN_BITS, MAX_BITS)
    }
}

/// Metadata emitted for every physical-style entropy source mixed into the QRNG output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceReading {
    /// Source name (photon_shot_noise, vacuum_fluctuation, fallback, etc.).
    pub source: String,
    /// Bits contributed by this sample.
    pub bits: u16,
    /// Number of photon shots / optical samples captured for the reading.
    pub shot_count: u32,
    /// Estimated bias in parts-per-million.
    pub bias_ppm: f32,
    /// Estimated drift in parts-per-million.
    pub drift_ppm: f32,
    /// Raw entropy bytes captured before whitening.
    pub raw_entropy: Vec<u8>,
}

impl SourceReading {
    /// SHA3-256 checksum of the raw entropy payload.
    pub fn checksum(&self) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        Digest::update(&mut hasher, self.source.as_bytes());
        Digest::update(&mut hasher, &self.raw_entropy);
        let digest: [u8; 32] = hasher.finalize().into();
        digest
    }
}

/// PQC metadata binding the entropy batch to Kyber / Dilithium levels + icosuple context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PqcEnvelope {
    pub kyber_level: u8,
    pub dilithium_level: u8,
    pub qrng_entropy_bits: u16,
    pub icosuple_reference: String,
}

/// Result of a QRNG mix for a single request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QrngEntropyFrame {
    pub epoch: u64,
    pub sequence: u64,
    pub request: EntropyRequest,
    pub envelope: PqcEnvelope,
    pub entropy: Vec<u8>,
    pub sources: Vec<SourceReading>,
    pub kyber_refresh: bool,
    pub dilithium_refresh: bool,
    pub timestamp_ps: u128,
    pub checksum: [u8; 32],
}

impl QrngEntropyFrame {
    /// Total entropy bits after whitening.
    pub fn entropy_bits(&self) -> usize {
        self.entropy.len() * 8
    }

    /// Render the entropy payload as a hex seed for dashboards / demos.
    pub fn as_hex_seed(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(self.entropy.len() * 2);
        for byte in &self.entropy {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0xF) as usize] as char);
        }
        out
    }
}

/// Epoch-level telemetry for a batch of entropy requests.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QrngTelemetry {
    pub epoch: u64,
    pub frames: Vec<QrngEntropyFrame>,
    pub aggregated_entropy_bits: u64,
    pub kyber_refreshes: u32,
    pub dilithium_refreshes: u32,
}

/// Interface each entropy source must implement so it can be mixed into the QRNG output.
pub trait QrngSource: Send {
    /// Telemetry-friendly label.
    fn label(&self) -> &'static str;
    /// Produce a reading for the request. `normalized_bits` is already clamped.
    fn sample(&mut self, request: &EntropyRequest, normalized_bits: u16) -> SourceReading;
}

/// Photon shot noise source (laser diodes + beam splitters style).
pub struct PhotonShotNoiseSource {
    rng: ChaCha20Rng,
}

impl PhotonShotNoiseSource {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha20Rng::seed_from_u64(seed),
        }
    }
}

impl QrngSource for PhotonShotNoiseSource {
    fn label(&self) -> &'static str {
        "photon_shot_noise"
    }

    fn sample(&mut self, request: &EntropyRequest, normalized_bits: u16) -> SourceReading {
        let tuple_bonus = if request.label.to_ascii_lowercase().contains("tuple") {
            1.15
        } else {
            1.0
        };
        let per_source_bits = ((normalized_bits as f32 * 0.5 * tuple_bonus).round() as u16)
            .max(128)
            .min(normalized_bits);
        let bytes = ((per_source_bits as usize + 7) / 8).max(32);
        let mut raw = vec![0u8; bytes];
        self.rng.fill_bytes(&mut raw);
        let shot_count =
            (per_source_bits as f32 * 4.0 * tuple_bonus) as u32 + self.rng.gen_range(128..2048);
        SourceReading {
            source: self.label().to_string(),
            bits: per_source_bits,
            shot_count,
            bias_ppm: self.rng.gen_range(-50.0..50.0),
            drift_ppm: self.rng.gen_range(-6.0..6.0),
            raw_entropy: raw,
        }
    }
}

/// Vacuum fluctuation source (balanced homodyne detection style).
pub struct VacuumFluctuationSource {
    rng: ChaCha20Rng,
}

impl VacuumFluctuationSource {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha20Rng::seed_from_u64(seed),
        }
    }
}

impl QrngSource for VacuumFluctuationSource {
    fn label(&self) -> &'static str {
        "vacuum_fluctuation"
    }

    fn sample(&mut self, request: &EntropyRequest, normalized_bits: u16) -> SourceReading {
        let qstp_bonus = if request.label.to_ascii_lowercase().contains("qstp") {
            1.1
        } else {
            1.0
        };
        let per_source_bits = ((normalized_bits as f32 * 0.5 * qstp_bonus).round() as u16)
            .max(128)
            .min(normalized_bits);
        let bytes = ((per_source_bits as usize + 7) / 8).max(32);
        let mut raw = vec![0u8; bytes];
        self.rng.fill_bytes(&mut raw);
        let shot_count =
            (per_source_bits as f32 * 3.0 * qstp_bonus) as u32 + self.rng.gen_range(64..1024);
        SourceReading {
            source: self.label().to_string(),
            bits: per_source_bits,
            shot_count,
            bias_ppm: self.rng.gen_range(-20.0..20.0),
            drift_ppm: self.rng.gen_range(-3.0..3.0),
            raw_entropy: raw,
        }
    }
}

/// Mixer that combines multiple entropy sources and binds them to PQC envelopes.
pub struct QrngMixer {
    rng: ChaCha20Rng,
    sources: Vec<Box<dyn QrngSource>>,
}

impl QrngMixer {
    /// Build a mixer with a deterministic RNG seed so tests and demos are reproducible.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha20Rng::seed_from_u64(seed),
            sources: Vec::new(),
        }
    }

    /// Register an entropy source.
    pub fn register_source<S>(&mut self, source: S)
    where
        S: QrngSource + 'static,
    {
        self.sources.push(Box::new(source));
    }

    /// Number of active entropy sources.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Generate a QRNG frame for a single request.
    pub fn generate_frame(
        &mut self,
        epoch: u64,
        sequence: u64,
        request: EntropyRequest,
    ) -> QrngEntropyFrame {
        let mut normalized = request.clone();
        normalized.bits = normalized.normalized_bits();
        if normalized.icosuple_reference.is_empty() {
            normalized.icosuple_reference =
                format!("icosuple:{}:{}", normalized.label, epoch).to_lowercase();
        }

        let mut samples = Vec::new();
        if self.sources.is_empty() {
            samples.push(self.synthetic_sample(&normalized));
        } else {
            for source in self.sources.iter_mut() {
                samples.push(source.sample(&normalized, normalized.bits));
            }
        }

        let mut shake = Shake256::default();
        shake.update(&epoch.to_le_bytes());
        shake.update(&sequence.to_le_bytes());
        shake.update(normalized.label.as_bytes());
        shake.update(&normalized.bits.to_le_bytes());
        shake.update(normalized.icosuple_reference.as_bytes());
        shake.update(&[normalized.kyber_level, normalized.dilithium_level]);

        for sample in &samples {
            shake.update(sample.source.as_bytes());
            shake.update(&sample.bits.to_le_bytes());
            shake.update(&sample.shot_count.to_le_bytes());
            shake.update(&sample.bias_ppm.to_le_bytes());
            shake.update(&sample.drift_ppm.to_le_bytes());
            shake.update(&sample.raw_entropy);
        }

        let byte_len = ((normalized.bits as usize + 7) / 8).max(32);
        let mut entropy = vec![0u8; byte_len];
        let mut reader = shake.finalize_xof();
        reader.read(&mut entropy);

        let checksum: [u8; 32] = Sha3_256::digest(&entropy).into();
        let kyber_refresh = self.rng.gen_bool(0.1);
        let dilithium_refresh = self.rng.gen_bool(0.05);

        let envelope = PqcEnvelope {
            kyber_level: normalized.kyber_level,
            dilithium_level: normalized.dilithium_level,
            qrng_entropy_bits: normalized.bits,
            icosuple_reference: normalized.icosuple_reference.clone(),
        };

        QrngEntropyFrame {
            epoch,
            sequence,
            request: normalized,
            envelope,
            entropy,
            sources: samples,
            kyber_refresh,
            dilithium_refresh,
            timestamp_ps: timestamp_ps(),
            checksum,
        }
    }

    fn synthetic_sample(&mut self, request: &EntropyRequest) -> SourceReading {
        let bytes = ((request.bits as usize + 7) / 8).max(32);
        let mut raw = vec![0u8; bytes];
        self.rng.fill_bytes(&mut raw);
        SourceReading {
            source: "synthetic_fallback".into(),
            bits: request.bits,
            shot_count: request.bits as u32 * 2,
            bias_ppm: 0.0,
            drift_ppm: 0.0,
            raw_entropy: raw,
        }
    }
}

/// Deterministic simulator that emits epoch-level telemetry.
pub struct QrngSim {
    mixer: QrngMixer,
    epoch: u64,
}

impl QrngSim {
    /// Build a simulator with default entropy sources.
    pub fn new(seed: u64) -> Self {
        let mut mixer = QrngMixer::new(seed);
        mixer.register_source(PhotonShotNoiseSource::new(seed ^ 0xa55a_a55a));
        mixer.register_source(VacuumFluctuationSource::new(seed.rotate_left(13)));
        Self { mixer, epoch: 0 }
    }

    /// Provide your own mixer (for advanced testing scenarios).
    pub fn with_mixer(mixer: QrngMixer) -> Self {
        Self { mixer, epoch: 0 }
    }

    /// Mutable access to the underlying mixer.
    pub fn mixer_mut(&mut self) -> &mut QrngMixer {
        &mut self.mixer
    }

    /// Run the simulator for a batch of requests, returning telemetry for the epoch.
    pub fn run_epoch(&mut self, requests: &[EntropyRequest]) -> QrngTelemetry {
        assert!(
            !requests.is_empty(),
            "at least one entropy request must be provided"
        );
        let epoch = self.epoch;
        let mut frames = Vec::with_capacity(requests.len());
        let mut kyber_refreshes = 0;
        let mut dilithium_refreshes = 0;

        for (idx, request) in requests.iter().enumerate() {
            let frame = self
                .mixer
                .generate_frame(epoch, idx as u64, request.clone());
            if frame.kyber_refresh {
                kyber_refreshes += 1;
            }
            if frame.dilithium_refresh {
                dilithium_refreshes += 1;
            }
            frames.push(frame);
        }

        self.epoch += 1;
        let aggregated_entropy_bits = frames.iter().map(|frame| frame.request.bits as u64).sum();

        QrngTelemetry {
            epoch,
            frames,
            aggregated_entropy_bits,
            kyber_refreshes,
            dilithium_refreshes,
        }
    }
}

fn timestamp_ps() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() * 1_000)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_request_clamps_bits() {
        let request = EntropyRequest::new("tuplechain", 65_535);
        assert_eq!(request.normalized_bits(), MAX_BITS);
    }

    #[test]
    fn mixer_generates_entropy_even_without_sources() {
        let mut mixer = QrngMixer::new(7);
        let request = EntropyRequest::new("fallback", 512);
        let frame = mixer.generate_frame(0, 0, request);
        assert_eq!(frame.sources.len(), 1);
        assert_eq!(frame.request.bits, 512);
        assert_eq!(frame.entropy_bits(), 512);
    }

    #[test]
    fn simulator_batches_requests() {
        let mut sim = QrngSim::new(42);
        let requests = vec![
            EntropyRequest::for_icosuple("tuplechain", 2048, "ico-alpha"),
            EntropyRequest::for_icosuple("qstp", 1024, "ico-beta"),
        ];
        let telemetry = sim.run_epoch(&requests);
        assert_eq!(telemetry.frames.len(), 2);
        assert!(telemetry.aggregated_entropy_bits >= 3_000);
    }
}
