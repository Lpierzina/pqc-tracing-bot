use pqcnet_networking::{
    control_plane::{
        ControlCommand, ControlEvent, ControlPlane, ControlPlaneConfig, NodeAnnouncement,
    },
    pubsub::PubSubRouter,
    qs_dag::{QsDag, StateDiff, StateOp},
};

fn main() {
    let router = PubSubRouter::default();
    let config = ControlPlaneConfig::default();
    let genesis = StateDiff::genesis("genesis", "bootstrap");

    let mut nodes: Vec<Node> = ["alpha", "beta", "gamma"]
        .into_iter()
        .map(|id| Node::new(id, router.clone(), config.clone(), &genesis))
        .collect();

    println!("== announcing nodes ==");
    for node in &nodes {
        node.announce();
    }
    pump_events(&mut nodes);

    println!("== exchanging state diffs ==");
    for round in 0..9 {
        let idx = (round as usize) % nodes.len();
        nodes[idx].propose_diff(round);
        pump_events(&mut nodes);
    }

    println!("== snapshots per node ==");
    let reference = nodes[0].dag.snapshot().expect("snapshot must exist");
    for node in &nodes {
        let snapshot = node.dag.snapshot().expect("snapshot must exist");
        println!(
            "[node:{id}] head={head} values={values:?}",
            id = node.plane.node_id(),
            head = snapshot.head_id,
            values = snapshot.values
        );
        assert_eq!(snapshot.head_id, reference.head_id);
        assert_eq!(snapshot.values, reference.values);
    }
    println!("All nodes converged via QS-DAG state sync!");
}

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
        self.plane
            .announce(NodeAnnouncement::new(self.plane.node_id()))
            .expect("announcement broadcast");
    }

    fn propose_diff(&mut self, round: u64) {
        let head = self
            .dag
            .canonical_head()
            .expect("canonical head exists")
            .id
            .clone();
        let diff = StateDiff::new(
            format!("{}-diff-{}", self.plane.node_id(), round),
            self.plane.node_id().to_string(),
            vec![head],
            {
                self.lamport += 1;
                self.lamport
            },
            vec![StateOp::upsert(
                format!("service-{round}"),
                format!("owner-{}", self.plane.node_id()),
            )],
        );
        self.dag.insert(diff.clone()).expect("local insert");
        self.plane
            .broadcast_command(ControlCommand::StateSync { diff })
            .expect("broadcast diff");
    }
}

fn pump_events(nodes: &mut [Node]) {
    loop {
        let mut progress = false;
        for node in nodes.iter_mut() {
            let events = node.plane.poll_events().expect("poll events");
            for event in events {
                match event {
                    ControlEvent::Discovery(announcement) => {
                        println!(
                            "[{}] discovered node {}",
                            node.plane.node_id(),
                            announcement.node_id
                        );
                        progress = true;
                    }
                    ControlEvent::Command(ControlCommand::StateSync { diff }) => {
                        if node.dag.contains(&diff.id) {
                            continue;
                        }
                        if node.dag.missing_parents(&diff).is_empty() {
                            let diff_id = diff.id.clone();
                            node.dag.insert(diff).expect("insert diff");
                            println!("[{}] applied diff {}", node.plane.node_id(), diff_id);
                            progress = true;
                        }
                    }
                    ControlEvent::Command(ControlCommand::Ping { .. }) => {
                        // not emitted in the example but handled for completeness
                    }
                    ControlEvent::Command(ControlCommand::Custom { .. }) => {}
                }
            }
        }
        if !progress {
            break;
        }
    }
}
