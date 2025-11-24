use pqcnet_qs_dag::{
    DagError, HierarchicalVerificationPools, IcosupleLayer, PayloadProfile, PoolTier, QIPTag,
    QrngCore, QsDag, ShardManager, ShardPolicy, StateDiff, StateOp, TemporalWeight, TupleDomain,
    TupleEnvelope, TupleValidation, VerificationVerdict,
};

struct DemoQrng(u64);

impl QrngCore for DemoQrng {
    type Error = core::convert::Infallible;

    fn next_u64(&mut self) -> Result<u64, Self::Error> {
        let current = self.0;
        self.0 = self.0.wrapping_add(1);
        Ok(current)
    }
}

fn main() -> Result<(), DagError> {
    let genesis = StateDiff::genesis("genesis", "bootstrap");
    let mut dag = QsDag::with_temporal_weight(genesis, TemporalWeight::new(32))?;

    let tuple = TupleEnvelope::new(
        TupleDomain::Finance,
        IcosupleLayer::CONSENSUS_TIER_9,
        PayloadProfile::AssetTransfer,
        "did:finance:alpha",
        "did:finance:beta",
        1_000_000,
        b"zk-settlement v1",
        [0xA5; 32],
        1_713_861_234_112,
        QIPTag::Bridge("QIP:Solana".to_string()),
        None,
        TupleValidation::new("Dilithium5", vec![0; 64], vec![1; 64]),
    )
    .without_inline_payload();

    let settlement_diff = StateDiff::with_tuple(
        "tuple-finance-001",
        "validator-alpha",
        vec!["genesis".into()],
        1,
        vec![
            StateOp::upsert("finance/routes/solana", "bridge-online"),
            StateOp::upsert("finance/latency-ms", "2.4"),
        ],
        tuple,
    );
    dag.insert(settlement_diff.clone())?;

    // Hierarchical verification pools: 2 tiers, QRNG-elected coordinators.
    let mut hvp = HierarchicalVerificationPools::new(
        vec![PoolTier::new(8, 4), PoolTier::new(9, 4)],
        DemoQrng(42),
    );
    for validator in ["alice", "bob", "carol", "dave"] {
        hvp.register_validator(8, validator.to_string());
    }
    hvp.register_validator(9, "eve".to_string());
    hvp.register_validator(9, "frank".to_string());
    let coordinators = hvp.elect_coordinators().expect("qrng never fails");
    println!("QRNG coordinators: {coordinators:?}");
    for validator in ["alice", "bob", "carol", "dave"] {
        if let Some(outcome) =
            hvp.submit_vote(settlement_diff.id.clone(), &validator.to_string(), VerificationVerdict::Approve)
        {
            println!("Finalized tuple {} with verdict {:?}", outcome.tuple_id, outcome.verdict);
            break;
        }
    }

    // Dynamic tuple sharding per domain.
    let mut shards = ShardManager::new(ShardPolicy::new(2));
    let assignment = shards
        .assign(settlement_diff.clone())
        .expect("shard assignment succeeds");
    println!(
        "Tuple {} routed to shard {:?} with anchor {:02x?}",
        assignment.tuple_id, assignment.shard_id, &assignment.global_anchor[..4]
    );

    let snapshot = dag.snapshot().expect("reachable head");
    println!("Canonical head: {}", snapshot.head_id);
    for (key, value) in snapshot.values.iter() {
        println!("{} => {}", key, value);
    }

    Ok(())
}
