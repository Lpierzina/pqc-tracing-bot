use pqcnet_qace::{GaQace, QaceGaConfig, QaceMetrics, QaceWeights};
use pqcnet_qstp::{
    establish_runtime_tunnel, hydrate_remote_tunnel, InMemoryMesh, InMemoryTupleChain, MeshPeerId,
    MeshQosClass, MeshRoutePlan, MeshTransport, TunnelRole,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_id = MeshPeerId::derive("qstp-node-a");
    let client_id = MeshPeerId::derive("qstp-node-b");

    let primary_route = MeshRoutePlan {
        topic: "waku/mesh/primary".into(),
        hops: vec![MeshPeerId::derive("hop-n1"), MeshPeerId::derive("hop-n2")],
        qos: MeshQosClass::LowLatency,
        epoch: 1,
    };

    let mut tuple_a = InMemoryTupleChain::new();

    let mut node_a = establish_runtime_tunnel(
        b"client=qstp-mesh-sim&ts=1",
        client_id,
        primary_route.clone(),
        &mut tuple_a,
    )?;

    let mut node_b = hydrate_remote_tunnel(
        node_a.session_secret.clone(),
        server_id,
        primary_route.clone(),
        node_a.peer_metadata.clone(),
        TunnelRole::Responder,
    )?;

    let mut mesh = InMemoryMesh::new();

    println!("== QSTP Mesh Simulator ==");
    println!(
        "tunnel_id={} topic={} hops={}",
        node_a.tunnel.metadata().tunnel_id,
        primary_route.topic,
        primary_route.hops.len()
    );

    let frame = node_a
        .tunnel
        .seal(b"waku::order-intent", b"waku-app/tuplechain")
        .expect("seal frame");
    mesh.publish(frame.clone())?;
    let delivered = mesh
        .try_recv(&primary_route.topic)
        .expect("delivered frame");
    let cleartext = node_b
        .open(&delivered, b"waku-app/tuplechain")
        .expect("decrypt payload");
    println!(
        "node_b decrypted payload: {}",
        String::from_utf8_lossy(&cleartext)
    );

    node_a.tunnel.register_alternate_routes(vec![MeshRoutePlan {
        topic: "waku/mesh/failsafe".into(),
        hops: vec![MeshPeerId::derive("hop-b1")],
        qos: MeshQosClass::Control,
        epoch: 2,
    }]);
    let mut engine = GaQace::new(
        QaceGaConfig {
            rng_seed: Some(41),
            ..Default::default()
        },
        QaceWeights::default(),
    );
    let decision = node_a.tunnel.apply_qace(
        QaceMetrics {
            latency_ms: 2,
            loss_bps: 7_500,
            threat_score: 94,
            route_changes: 0,
            ..Default::default()
        },
        &mut engine,
    )?;
    println!(
        "QACE decision action={:?} score={} primary={}",
        decision.action, decision.score, decision.path_set.primary.topic
    );
    node_b.register_alternate_routes(vec![decision.path_set.primary.clone()]);
    let mut responder_engine = engine.clone();
    let responder_decision = node_b.apply_qace(
        QaceMetrics {
            latency_ms: 2,
            loss_bps: 7_500,
            threat_score: 94,
            route_changes: 1,
            ..Default::default()
        },
        &mut responder_engine,
    )?;
    println!(
        "node_b route switched to {} (confidence {:.2})",
        responder_decision.path_set.primary.topic, responder_decision.convergence.confidence
    );

    let rerouted_frame = node_a
        .tunnel
        .seal(b"waku::rerouted-intent", b"waku-app/tuplechain")
        .expect("seal rerouted");
    mesh.publish(rerouted_frame.clone())?;
    let rerouted = mesh
        .try_recv(&node_a.tunnel.route().topic)
        .expect("rerouted frame");
    let cleartext_reroute = node_b
        .open(&rerouted, b"waku-app/tuplechain")
        .expect("decrypt rerouted");
    println!(
        "rerouted payload decrypted: {}",
        String::from_utf8_lossy(&cleartext_reroute)
    );

    let tuple_plain = node_a.tunnel.fetch_tuple_metadata(&tuple_a)?;
    println!(
        "tuplechain pointer {:?} route_hash {}",
        node_a.tunnel.metadata().tuple_pointer.0,
        hex(&tuple_plain.route_hash)
    );

    let mut attacker = hydrate_remote_tunnel(
        vec![0u8; node_a.session_secret.len()],
        server_id,
        node_a.tunnel.route().clone(),
        node_a.peer_metadata.clone(),
        TunnelRole::Responder,
    )?;
    let tampered = attacker.open(&rerouted, b"waku-app/tuplechain");
    println!("eavesdrop decrypt result: {:?}", tampered.err());

    println!("mesh simulator finished");
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use core::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}
