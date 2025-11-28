use std::net::TcpListener;

use pqcnet_networking::{NetworkClient, NetworkingConfig, PeerConfig};

fn main() {
    let peers = vec![LocalPeer::new("peer-a"), LocalPeer::new("peer-b")];
    let mut config = NetworkingConfig::sample("127.0.0.1:7300");
    config.peers = peers.iter().map(LocalPeer::as_peer_config).collect();
    let client = NetworkClient::from_config("relayer-a", config);

    let receipt = client
        .publish("peer-a", "hello from relayer-a")
        .expect("peer exists in sample config");
    println!(
        "[pqcnet-networking] delivered to {} in {} ms",
        receipt.peer_id, receipt.latency_ms
    );

    let broadcast = client.broadcast("quorum-ping").expect("fan-out succeeds");
    println!(
        "[pqcnet-networking] broadcast delivered to {} peers",
        broadcast.len()
    );

    for message in client.drain_inflight() {
        println!(
            "[pqcnet-networking] {} -> {} payload={:?}",
            message.from, message.to, message.payload
        );
    }

    drop(peers);
}

struct LocalPeer {
    id: String,
    listener: TcpListener,
}

impl LocalPeer {
    fn new(id: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("listener address").to_string();
        println!("[local-peer:{id}] listening on {addr}");
        Self {
            id: id.to_owned(),
            listener,
        }
    }

    fn as_peer_config(&self) -> PeerConfig {
        PeerConfig {
            id: self.id.clone(),
            address: self.listener.local_addr().unwrap().to_string(),
        }
    }
}
