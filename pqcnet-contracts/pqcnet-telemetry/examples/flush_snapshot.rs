use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use pqcnet_telemetry::{TelemetryConfig, TelemetryHandle};

fn main() {
    let collector = HttpCollector::start(1);
    let telemetry = TelemetryHandle::from_config(TelemetryConfig::sample(&collector.url));

    for _ in 0..3 {
        telemetry
            .record_counter("ingest.success", 1)
            .expect("within u64 range");
    }
    telemetry.record_latency_ms("pipeline", 42);
    telemetry.record_latency_ms("pipeline", 54);

    let snapshot = telemetry.flush().expect("telemetry export succeeds");
    println!(
        "[pqcnet-telemetry] counters={:?} latencies={:?}",
        snapshot.counters, snapshot.latencies_ms
    );
}

struct HttpCollector {
    url: String,
    join: Option<thread::JoinHandle<()>>,
}

impl HttpCollector {
    fn start(expected_requests: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind collector");
        let addr = listener.local_addr().expect("collector addr");
        let handle = thread::spawn(move || {
            for _ in 0..expected_requests {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                }
            }
        });
        Self {
            url: format!("http://{}", addr),
            join: Some(handle),
        }
    }
}

impl Drop for HttpCollector {
    fn drop(&mut self) {
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}
