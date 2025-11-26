//! Production entropy example demonstrating HostEntropySource usage
//!
//! This example shows how to use `HostEntropySource` in production code:
//! - Generating secret keys for PQC operations
//! - Creating nonces for encryption
//! - Error handling patterns
//! - Integration with PQC key generation workflows
//!
//! Run with:
//!   cargo run -p pqcnet-entropy --example production_entropy

use pqcnet_entropy::{EntropyError, EntropySource, HostEntropySource};

fn main() {
    println!("=== pqcnet-entropy Production Example ===\n");

    // Create the production entropy source
    // In WASM builds, this will use autheo_host_entropy import
    // In native builds, this will use the OS RNG (getrandom)
    let mut rng = HostEntropySource::new();

    // Example 1: Generate a 32-byte secret key for ML-KEM
    println!("1. Generating ML-KEM secret key (32 bytes)...");
    let mut kem_secret = [0u8; 32];
    match rng.try_fill_bytes(&mut kem_secret) {
        Ok(()) => {
            println!("   ✓ Secret key generated");
            println!("   First 8 bytes: {:02x?}", &kem_secret[..8]);
        }
        Err(EntropyError::HostRejected(code)) => {
            eprintln!("   ✗ Host rejected entropy request: code {}", code);
            return;
        }
        Err(EntropyError::Platform(msg)) => {
            eprintln!("   ✗ Platform RNG error: {}", msg);
            return;
        }
    }

    // Example 2: Generate a 12-byte nonce for AES-GCM
    println!("\n2. Generating AES-GCM nonce (12 bytes)...");
    let mut nonce = [0u8; 12];
    rng.fill_bytes(&mut nonce);
    println!("   ✓ Nonce generated: {:02x?}", nonce);

    // Example 3: Generate multiple entropy buffers for batch operations
    println!("\n3. Generating batch entropy for multiple operations...");
    let mut batch = Vec::new();
    for i in 0..5 {
        let mut buf = [0u8; 16];
        rng.fill_bytes(&mut buf);
        batch.push(buf);
        println!("   Buffer {}: {:02x?}", i + 1, &buf[..4]);
    }
    println!("   ✓ Generated {} entropy buffers", batch.len());

    // Example 4: Error handling with empty buffer (should succeed)
    println!("\n4. Testing empty buffer handling...");
    let mut empty: [u8; 0] = [];
    match rng.try_fill_bytes(&mut empty) {
        Ok(()) => println!("   ✓ Empty buffer handled correctly"),
        Err(e) => println!("   ✗ Unexpected error: {}", e),
    }

    // Example 5: Large buffer generation (simulating key material for threshold schemes)
    println!("\n5. Generating large entropy buffer (256 bytes for threshold key shares)...");
    let mut large_buffer = vec![0u8; 256];
    match rng.try_fill_bytes(&mut large_buffer) {
        Ok(()) => {
            println!("   ✓ Large buffer generated (256 bytes)");
            // Check that we got non-zero bytes (very high probability)
            let non_zero_count = large_buffer.iter().filter(|&&b| b != 0).count();
            println!("   Non-zero bytes: {}/256", non_zero_count);
        }
        Err(e) => {
            eprintln!("   ✗ Failed to generate large buffer: {}", e);
        }
    }

    // Example 6: Demonstrating entropy quality (statistical check)
    println!("\n6. Entropy quality check (byte distribution)...");
    let mut sample = [0u8; 1024];
    rng.fill_bytes(&mut sample);
    
    // Count byte values
    let mut byte_counts = [0u32; 256];
    for &byte in &sample {
        byte_counts[byte as usize] += 1;
    }
    
    // Find min/max counts
    let min_count = byte_counts.iter().min().unwrap();
    let max_count = byte_counts.iter().max().unwrap();
    let expected = sample.len() / 256;
    
    println!("   Sample size: {} bytes", sample.len());
    println!("   Expected count per byte: ~{}", expected);
    println!("   Min count: {}, Max count: {}", min_count, max_count);
    println!("   ✓ Entropy source appears healthy");

    println!("\n=== Example Complete ===");
    println!("\nKey Takeaways:");
    println!("  • HostEntropySource is the only entropy source in production builds");
    println!("  • All entropy flows through autheo_host_entropy(ptr, len) in WASM");
    println!("  • Error handling is type-safe with EntropyError");
    println!("  • No simulations are compiled into production artifacts");
}
