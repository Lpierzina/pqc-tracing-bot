use autheo_pqcnet_5dqeh::{QrngEntropyRng, VertexId};

fn main() {
    let mut rng = QrngEntropyRng::with_seed(0x5d0e);
    let vertex_a = VertexId::random(&mut rng);
    let vertex_b = VertexId::random(&mut rng);

    println!("host entropy demo");
    println!("vertex-a {}", vertex_a);
    println!("vertex-b {}", vertex_b);
    let throughput = rng.gen_range_f64(1_000.0..=2_500.0);
    let qkd = rng.gen_bool(0.5);
    let raw = rng.next_u64();
    println!(
        "laser throughput {:.2} Gbps, qkd={}, entropy-word=0x{raw:016x}",
        throughput, qkd
    );
}
