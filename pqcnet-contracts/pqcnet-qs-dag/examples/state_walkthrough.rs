use pqcnet_qs_dag::{DagError, QsDag, StateDiff, StateOp, TemporalWeight};

fn main() -> Result<(), DagError> {
    let genesis = StateDiff::genesis("genesis", "bootstrap");
    let mut dag = QsDag::with_temporal_weight(genesis, TemporalWeight::new(16))?;

    let relayer_diff = StateDiff::new(
        "relayer-1",
        "relayer-a",
        vec!["genesis".into()],
        1,
        vec![
            StateOp::upsert("relayer/a/status", "online"),
            StateOp::upsert("relayer/a/stake", "25000"),
        ],
    );
    dag.insert(relayer_diff)?;

    let route_diff = StateDiff::new(
        "route-update-1",
        "qace",
        vec!["relayer-1".into()],
        2,
        vec![
            StateOp::upsert("route/topic:waku/leader", "relayer-a"),
            StateOp::upsert("route/topic:waku/latency-ms", "42"),
        ],
    );
    dag.insert(route_diff)?;

    let snapshot = dag.snapshot().expect("reachable head");
    println!("Canonical head: {}", snapshot.head_id);
    for (key, value) in snapshot.values.iter() {
        println!("{} => {}", key, value);
    }

    Ok(())
}
