use std::time::Instant;

use anyhow::{Context, Result};
use autheo_pqcnet_5dezph::{warm_halo2_key_cache, EzphConfig};

fn main() -> Result<()> {
    let config = EzphConfig::default();
    println!(
        "[warm_halo2] warming Halo2 cache for circuit '{}' (curve={}, soundness={:.2e})",
        config.zk.circuit_id, config.zk.curve, config.zk.soundness
    );
    let start = Instant::now();
    warm_halo2_key_cache(&config.zk).context("Halo2 cache warmup failed")?;
    println!(
        "[warm_halo2] completed in {:.2?} (artifacts under config/crypto)",
        start.elapsed()
    );
    Ok(())
}
