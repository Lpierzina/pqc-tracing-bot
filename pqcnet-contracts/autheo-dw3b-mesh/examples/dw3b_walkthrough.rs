use autheo_dw3b_mesh::{
    Dw3bMeshConfig, Dw3bMeshEngine, MeshAnonymizeRequest, MeshError, MeshResult, QtaidProveRequest,
};
use serde::de::DeserializeOwned;
use std::{
    env,
    fs,
    path::{Path, PathBuf},
};

fn main() -> MeshResult<()> {
    let config_path = resolve_config_path();
    let config: Dw3bMeshConfig =
        load_config(&config_path).map_err(|err| MeshError::InvalidParameter(err.to_string()))?;
    let mut engine = Dw3bMeshEngine::new(config);
    let request_path = resolve_request_path();
    let request: MeshAnonymizeRequest =
        load_config(&request_path).map_err(|err| MeshError::InvalidParameter(err.to_string()))?;
    let response = engine.anonymize_query(request)?;
    println!(
        "DW3B anonymize proof_id={} hops={} k-anon={:.6}",
        response.proof.proof_id,
        response.route_plan.hop_count(),
        response.proof.metrics.k_anonymity,
    );
    println!(
        "chaos λ={:.3} entropy_samples={}",
        response.chaos.lyapunov_exponent, response.entropy_snapshot.samples_generated,
    );
    let qtaid = engine.qtaid_prove(QtaidProveRequest {
        owner_did: "did:autheo:demo".into(),
        trait_name: "BRCA1=negative".into(),
        genome_segment: "AGCTTAGCTA".into(),
        bits_per_snp: 4,
    })?;
    println!(
        "QTAID tokens={} proof_id={}",
        qtaid.tokens.len(),
        qtaid.response.proof.proof_id,
    );
    Ok(())
}

fn resolve_config_path() -> String {
    locate_file(
        "DW3B_CONFIG",
        &["config/dw3b.toml", "config/dw3b.yaml"],
        "config/dw3b.toml",
    )
}

fn resolve_request_path() -> String {
    locate_file(
        "DW3B_REQUEST",
        &[
            "config/examples/dw3b_request.toml",
            "config/examples/dw3b_request.yaml",
        ],
        "config/examples/dw3b_request.yaml",
    )
}

fn load_config<T: DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;
    match Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "toml" => Ok(toml::from_str(&contents)?),
        "yaml" | "yml" => Ok(serde_yaml::from_str(&contents)?),
        ext => Err(format!("unsupported config extension '{ext}' for {path}").into()),
    }
}

fn locate_file(env_key: &str, candidates: &[&str], default_path: &str) -> String {
    if let Ok(env_path) = env::var(env_key) {
        return env_path;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut search_roots = vec![PathBuf::from(".")];
    search_roots.push(manifest_dir.clone());

    if let Some(parent) = manifest_dir.parent() {
        search_roots.push(parent.to_path_buf());
        if let Some(grandparent) = parent.parent() {
            search_roots.push(grandparent.to_path_buf());
        }
    }

    for root in search_roots {
        for candidate in candidates {
            let path = root.join(candidate);
            if path.exists() {
                return path.to_string_lossy().into_owned();
            }
        }
    }

    default_path.to_string()
}
