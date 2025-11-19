use pqcnet_qstp::{
    establish_runtime_tunnel, hydrate_remote_tunnel, InMemoryTupleChain, MeshPeerId, MeshQosClass,
    MeshRoutePlan, TunnelRole,
};
use rcgen::generate_simple_self_signed;
use rustls::{
    Certificate, ClientConfig, ClientConnection, PrivateKey, RootCertStore, ServerConfig,
    ServerConnection, ServerName,
};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_ITERATIONS: usize = 200;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations = iterations_from_env();
    let server_id = MeshPeerId::derive("perf-node-a");
    let client_id = MeshPeerId::derive("perf-node-b");
    let payload = vec![0u8; 512];
    let aad = b"waku-app/perf";

    let (client_cfg, server_cfg) = build_tls_configs()?;
    let client_cfg = Arc::new(client_cfg);
    let server_cfg = Arc::new(server_cfg);

    let mut qstp_handshakes = Duration::ZERO;
    let mut qstp_payload = Duration::ZERO;
    let mut tls_handshakes = Duration::ZERO;
    let mut tls_payload = Duration::ZERO;

    for i in 0..iterations {
        let route = MeshRoutePlan {
            topic: "waku/perf".into(),
            hops: vec![MeshPeerId::derive("hop-perf")],
            qos: MeshQosClass::LowLatency,
            epoch: i as u64 + 1,
        };
        let mut tuple_chain = InMemoryTupleChain::new();

        let start = Instant::now();
        let mut qstp = establish_runtime_tunnel(
            format!("client=perf&ts={}", i).as_bytes(),
            client_id,
            route.clone(),
            &mut tuple_chain,
        )?;
        let mut qstp_remote = hydrate_remote_tunnel(
            qstp.session_secret.clone(),
            server_id,
            route,
            qstp.peer_metadata.clone(),
            TunnelRole::Responder,
        )?;
        qstp_handshakes += start.elapsed();

        let payload_start = Instant::now();
        let frame = qstp
            .tunnel
            .seal(&payload, aad)
            .expect("seal benchmark payload");
        let _ = qstp_remote
            .open(&frame, aad)
            .expect("decrypt benchmark payload");
        qstp_payload += payload_start.elapsed();

        let mut client =
            ClientConnection::new(client_cfg.clone(), ServerName::try_from("qstp.local")?)?;
        let mut server = ServerConnection::new(server_cfg.clone())?;
        let mut pipe = MemoryPipe::new();

        let tls_start = Instant::now();
        drive_tls(&mut client, &mut server, &mut pipe)?;
        tls_handshakes += tls_start.elapsed();

        let tls_payload_start = Instant::now();
        let echoed = exchange_payload(&mut client, &mut server, &mut pipe, &payload)?;
        assert_eq!(echoed.len(), payload.len());
        tls_payload += tls_payload_start.elapsed();
    }

    let qstp_handshake_ms = as_ms(qstp_handshakes) / iterations as f64;
    let qstp_payload_ms = as_ms(qstp_payload) / iterations as f64;
    let tls_handshake_ms = as_ms(tls_handshakes) / iterations as f64;
    let tls_payload_ms = as_ms(tls_payload) / iterations as f64;

    let handshake_overhead = percent_overhead(qstp_handshake_ms, tls_handshake_ms);
    let payload_overhead = percent_overhead(qstp_payload_ms, tls_payload_ms);
    let qstp_total = qstp_handshake_ms + qstp_payload_ms;
    let tls_total = tls_handshake_ms + tls_payload_ms;
    let end_to_end_overhead = percent_overhead(qstp_total, tls_total);

    println!("== QSTP vs TLS performance ({iterations} iters) ==");
    println!("QSTP handshake avg : {:.3} ms", qstp_handshake_ms);
    println!("TLS  handshake avg : {:.3} ms", tls_handshake_ms);
    println!("QSTP payload avg   : {:.3} ms", qstp_payload_ms);
    println!("TLS  payload avg   : {:.3} ms", tls_payload_ms);
    println!("Handshake overhead : {handshake_overhead:+.2}%");
    println!("Payload overhead   : {payload_overhead:+.2}%");
    println!("End-to-end overhead: {end_to_end_overhead:+.2}%");

    Ok(())
}

fn percent_overhead(qstp_ms: f64, tls_ms: f64) -> f64 {
    if tls_ms == 0.0 {
        return 0.0;
    }
    ((qstp_ms - tls_ms) / tls_ms) * 100.0
}

fn as_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn iterations_from_env() -> usize {
    const ENV_KEY: &str = "QSTP_PERF_ITERS";
    std::env::var(ENV_KEY)
        .ok()
        .and_then(|raw| raw.parse().ok())
        .filter(|value: &usize| *value > 0)
        .unwrap_or(DEFAULT_ITERATIONS)
}

fn build_tls_configs() -> Result<(ClientConfig, ServerConfig), Box<dyn std::error::Error>> {
    let cert = generate_simple_self_signed(vec!["qstp.local".into()])?;
    let cert_der = cert.serialize_der()?;
    let priv_der = cert.serialize_private_key_der();

    let server_config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(vec![Certificate(cert_der.clone())], PrivateKey(priv_der))?;

    let mut roots = RootCertStore::empty();
    roots
        .add(&Certificate(cert_der))
        .map_err(|e| format!("add root: {e:?}"))?;
    let client_config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok((client_config, server_config))
}

fn drive_tls(
    client: &mut ClientConnection,
    server: &mut ServerConnection,
    pipe: &mut MemoryPipe,
) -> io::Result<()> {
    while client.is_handshaking() || server.is_handshaking() {
        flush_client(client, &mut pipe.client_to_server)?;
        process_server(server, &mut pipe.client_to_server)?;
        flush_server(server, &mut pipe.server_to_client)?;
        process_client(client, &mut pipe.server_to_client)?;
    }
    Ok(())
}

fn exchange_payload(
    client: &mut ClientConnection,
    server: &mut ServerConnection,
    pipe: &mut MemoryPipe,
    payload: &[u8],
) -> io::Result<Vec<u8>> {
    client.writer().write_all(payload)?;
    client.writer().flush()?;
    flush_client(client, &mut pipe.client_to_server)?;
    process_server(server, &mut pipe.client_to_server)?;
    flush_server(server, &mut pipe.server_to_client)?;
    process_client(client, &mut pipe.server_to_client)?;

    let mut buf = vec![0u8; payload.len()];
    server.reader().read_exact(&mut buf)?;
    Ok(buf)
}

fn flush_client(conn: &mut ClientConnection, queue: &mut VecDeque<u8>) -> io::Result<()> {
    while conn.wants_write() {
        let mut writer = QueueWriter { queue };
        conn.write_tls(&mut writer)?;
    }
    Ok(())
}

fn flush_server(conn: &mut ServerConnection, queue: &mut VecDeque<u8>) -> io::Result<()> {
    while conn.wants_write() {
        let mut writer = QueueWriter { queue };
        conn.write_tls(&mut writer)?;
    }
    Ok(())
}

fn process_server(conn: &mut ServerConnection, queue: &mut VecDeque<u8>) -> io::Result<()> {
    while !queue.is_empty() {
        let mut reader = QueueReader { queue };
        if conn.read_tls(&mut reader)? == 0 {
            break;
        }
        conn.process_new_packets()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    }
    Ok(())
}

fn process_client(conn: &mut ClientConnection, queue: &mut VecDeque<u8>) -> io::Result<()> {
    while !queue.is_empty() {
        let mut reader = QueueReader { queue };
        if conn.read_tls(&mut reader)? == 0 {
            break;
        }
        conn.process_new_packets()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    }
    Ok(())
}

struct MemoryPipe {
    client_to_server: VecDeque<u8>,
    server_to_client: VecDeque<u8>,
}

impl MemoryPipe {
    fn new() -> Self {
        Self {
            client_to_server: VecDeque::new(),
            server_to_client: VecDeque::new(),
        }
    }
}

struct QueueWriter<'a> {
    queue: &'a mut VecDeque<u8>,
}

impl Write for QueueWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.queue.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct QueueReader<'a> {
    queue: &'a mut VecDeque<u8>,
}

impl Read for QueueReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut count = 0;
        while count < out.len() {
            match self.queue.pop_front() {
                Some(byte) => {
                    out[count] = byte;
                    count += 1;
                }
                None => break,
            }
        }
        Ok(count)
    }
}
