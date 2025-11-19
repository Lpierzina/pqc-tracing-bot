#![cfg(not(target_arch = "wasm32"))]

mod handshake_demo {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/handshake_demo.rs"
    ));

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        main()
    }
}

mod secret_sharing_demo {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/secret_sharing_demo.rs"
    ));

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        main().map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })
    }
}

mod qstp_mesh_sim {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/qstp_mesh_sim.rs"
    ));

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        main()
    }
}

mod qstp_performance {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/qstp_performance.rs"
    ));

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        main()
    }
}

struct EnvVarGuard {
    key: &'static str,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &'static str) -> Self {
        std::env::set_var(key, value);
        Self { key }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        std::env::remove_var(self.key);
    }
}

#[test]
fn handshake_demo_executes() {
    handshake_demo::run().expect("handshake demo should complete");
}

#[test]
fn qstp_mesh_simulation_executes() {
    qstp_mesh_sim::run().expect("qstp mesh simulation should complete");
}

#[test]
fn secret_sharing_demo_executes() {
    secret_sharing_demo::run().expect("secret sharing demo should complete");
}

#[test]
fn qstp_performance_benchmark_executes_quickly() {
    let _guard = EnvVarGuard::set("QSTP_PERF_ITERS", "5");
    qstp_performance::run().expect("qstp performance demo should complete");
}
