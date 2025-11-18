use pqcnet_networking::{NetworkClient, NetworkingConfig};

fn main() {
    let config = NetworkingConfig::sample("127.0.0.1:7300");
    let client = NetworkClient::from_config("relayer-a", config);

    let receipt = client
        .publish("peer-a", "hello from relayer-a")
        .expect("peer exists in sample config");
    println!(
        "[pqcnet-networking] delivered to {} in {} ms",
        receipt.peer_id, receipt.latency_ms
    );

    let broadcast = client.broadcast("quorum-ping");
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
}
