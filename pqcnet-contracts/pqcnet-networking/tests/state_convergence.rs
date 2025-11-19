use pqcnet_networking::{
    control_plane::{
        ControlCommand, ControlEvent, ControlPlane, ControlPlaneConfig, NodeAnnouncement,
    },
    pubsub::PubSubRouter,
    qs_dag::{QsDag, StateDiff, StateOp},
};

struct Node {
    plane: ControlPlane,
    dag: QsDag,
    lamport: u64,
}

impl Node {
    fn new(
        node_id: &str,
        router: PubSubRouter,
        config: ControlPlaneConfig,
        genesis: &StateDiff,
    ) -> Self {
        let plane = ControlPlane::new(node_id, router, config);
        let dag = QsDag::new(genesis.clone()).expect("genesis initializes DAG");
        Self {
            plane,
            dag,
            lamport: 0,
        }
    }

    fn announce(&self) {
        let announcement = NodeAnnouncement::new(self.plane.node_id());
        self.plane
            .announce(announcement)
            .expect("announce succeeds");
    }
}

#[test]
fn state_converges_across_nodes() {
    let router = PubSubRouter::default();
    let config = ControlPlaneConfig::default();
    let genesis = StateDiff::genesis("genesis", "bootstrap");

    let mut nodes: Vec<Node> = (0..4)
        .map(|idx| {
            Node::new(
                &format!("node-{}", idx),
                router.clone(),
                config.clone(),
                &genesis,
            )
        })
        .collect();

    for node in &nodes {
        node.announce();
    }
    flush_events(&mut nodes);

    for round in 0..8 {
        let idx = (round as usize) % nodes.len();
        let head = nodes[idx]
            .dag
            .canonical_head()
            .expect("head exists")
            .id
            .clone();
        let diff = StateDiff::new(
            format!("node-{}-{}", idx, round),
            nodes[idx].plane.node_id().to_string(),
            vec![head],
            nodes[idx].lamport + 1,
            vec![StateOp::upsert(format!("k{}", round), format!("v{}", idx))],
        );
        nodes[idx].lamport += 1;
        nodes[idx].dag.insert(diff.clone()).unwrap();
        nodes[idx]
            .plane
            .broadcast_command(ControlCommand::StateSync { diff })
            .unwrap();
        flush_events(&mut nodes);
    }

    let reference = nodes[0].dag.snapshot().expect("snapshot exists");
    for node in &nodes {
        let snapshot = node.dag.snapshot().expect("snapshot exists");
        assert_eq!(snapshot.values, reference.values);
        assert_eq!(snapshot.head_id, reference.head_id);
    }
}

fn flush_events(nodes: &mut [Node]) {
    loop {
        let mut progress = false;
        for idx in 0..nodes.len() {
            let events = nodes[idx].plane.poll_events().expect("poll events");
            for event in events {
                match event {
                    ControlEvent::Command(ControlCommand::StateSync { diff }) => {
                        if nodes[idx].dag.contains(&diff.id) {
                            continue;
                        }
                        if nodes[idx].dag.missing_parents(&diff).is_empty() {
                            nodes[idx].dag.insert(diff).unwrap();
                            progress = true;
                        }
                    }
                    ControlEvent::Discovery(_) => {
                        progress = true;
                    }
                    _ => {}
                }
            }
        }
        if !progress {
            break;
        }
    }
}
