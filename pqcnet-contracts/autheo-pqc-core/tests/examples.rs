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

#[test]
fn handshake_demo_executes() {
    handshake_demo::run().expect("handshake demo should complete");
}

#[test]
fn secret_sharing_demo_executes() {
    secret_sharing_demo::run().expect("secret sharing demo should complete");
}
