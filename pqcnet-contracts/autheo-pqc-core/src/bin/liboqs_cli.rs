use autheo_pqc_core::liboqs::{
    LibOqsConfig, LibOqsDsaAlgorithm, LibOqsKemAlgorithm, LibOqsProvider,
};
use autheo_pqc_core::types::TimestampMs;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), autheo_pqc_core::error::PqcError> {
    let cli_args: Vec<String> = env::args().collect();
    let message = parse_message_arg(&cli_args)
        .unwrap_or_else(|| "zer0veil::quantum_channel=default".to_owned());

    let mut config = LibOqsConfig::default();
    config.kem_algorithm = LibOqsKemAlgorithm::MlKem768;
    config.dsa_algorithm = LibOqsDsaAlgorithm::MlDsa65;

    println!("Using {:?} for ML-KEM", config.kem_algorithm);
    println!("Using {:?} for ML-DSA", config.dsa_algorithm);

    let mut provider = LibOqsProvider::new(config.clone())?;
    let now = unix_timestamp_ms();
    let artifacts = provider.keygen(now)?;
    println!(
        "Generated KEM key id={} (expires at {})",
        hex(&artifacts.kem_state.id.0),
        artifacts.kem_state.expires_at
    );
    println!(
        "Generated DSA key id={} (level={:?})",
        hex(&artifacts.signing_state.id.0),
        artifacts.signing_state.level
    );

    let signature = provider.sign(message.as_bytes())?;
    provider.verify(message.as_bytes(), &signature)?;
    println!(
        "Signature OK (len={}): {}",
        signature.len(),
        hex(&signature[..32.min(signature.len())])
    );

    match provider.rotate(now + config.rotation_interval_ms + 1)? {
        Some(rotation) => {
            println!(
                "Rotated KEM key {} → {}",
                hex(&rotation.kem.old.id.0),
                hex(&rotation.kem.new.id.0)
            );
            println!("New signing key id={}", hex(&rotation.signing_state.id.0));
        }
        None => println!("Rotation not required yet."),
    }

    Ok(())
}

fn parse_message_arg(args: &[String]) -> Option<String> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--message" {
            return iter.next().cloned();
        }
    }
    None
}

fn unix_timestamp_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as TimestampMs)
        .unwrap_or(0)
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}
