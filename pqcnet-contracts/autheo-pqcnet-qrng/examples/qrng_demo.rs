use autheo_pqcnet_qrng::{EntropyRequest, QrngSim};

fn main() {
    let mut sim = QrngSim::new(1337);
    let requests = vec![
        EntropyRequest::for_icosuple("tuplechain-anchor", 3072, "ico-mainnet-0001"),
        EntropyRequest::for_icosuple("qstp-telemetry", 2048, "ico-qstp-telemetry")
            .with_security(3, 3),
        EntropyRequest::new("depin-metering", 1024).with_reference("ico-depin-07"),
    ];

    let telemetry = sim.run_epoch(&requests);
    println!(
        "Epoch {} => {} entropy frames ({} bits)",
        telemetry.epoch,
        telemetry.frames.len(),
        telemetry.aggregated_entropy_bits
    );

    for frame in telemetry.frames {
        println!(
            "- {} :: {} bits · Kyber L{} · Dilithium L{} · kyber_refresh={} · seed={}",
            frame.request.label,
            frame.request.bits,
            frame.envelope.kyber_level,
            frame.envelope.dilithium_level,
            frame.kyber_refresh,
            frame.as_hex_seed().chars().take(32).collect::<String>()
        );
        println!(
            "  sources: {} (bias_ppm ~ {:.2}, drift_ppm ~ {:.2})",
            frame.sources.len(),
            frame.sources.iter().map(|s| s.bias_ppm).sum::<f32>() / frame.sources.len() as f32,
            frame.sources.iter().map(|s| s.drift_ppm).sum::<f32>() / frame.sources.len() as f32
        );
    }
}
