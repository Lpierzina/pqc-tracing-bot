use autheo_pqcnet_qrng::{EntropyRequest, QrngSim};

#[test]
fn telemetry_tracks_refreshes() {
    let mut sim = QrngSim::new(99);
    let telemetry = sim.run_epoch(&[
        EntropyRequest::for_icosuple("tuplechain", 2048, "ico-demo-1"),
        EntropyRequest::for_icosuple("qstp", 3072, "ico-demo-2"),
    ]);
    assert_eq!(telemetry.frames.len(), 2);
    assert!(
        telemetry.kyber_refreshes <= 2 && telemetry.dilithium_refreshes <= 2,
        "refresh counters should not exceed frame count"
    );
}

#[test]
fn different_seeds_generate_distinct_entropy() {
    let mut sim_a = QrngSim::new(1);
    let mut sim_b = QrngSim::new(2);
    let requests = vec![EntropyRequest::new("tuplechain", 1024)];
    let frame_a = sim_a.run_epoch(&requests).frames.remove(0);
    let frame_b = sim_b.run_epoch(&requests).frames.remove(0);
    assert_ne!(
        frame_a.checksum, frame_b.checksum,
        "entropy should differ when seeds differ"
    );
}
