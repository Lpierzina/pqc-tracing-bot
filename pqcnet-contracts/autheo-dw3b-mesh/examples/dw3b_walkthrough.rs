use autheo_dw3b_mesh::{
    Dw3bMeshConfig, Dw3bMeshEngine, MeshAnonymizeRequest, MeshResult, QtaidProveRequest,
};

fn main() -> MeshResult<()> {
    let mut engine = Dw3bMeshEngine::new(Dw3bMeshConfig::production());
    let response = engine.anonymize_query(MeshAnonymizeRequest::demo())?;
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
