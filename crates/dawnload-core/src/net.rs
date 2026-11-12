//! The one HTTP client setup: rustls with the `ring` provider (pure Rust plus a little C/asm,
//! no cmake or NASM on Windows) and the OS certificate store, so corporate proxies work.

use std::sync::Once;
use std::time::Duration;

pub const USER_AGENT: &str = concat!("Dawnload/", env!("CARGO_PKG_VERSION"), " (+https://github.com/brenoandrew/Dawnload)");

fn install_provider() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

pub fn client_builder() -> reqwest::ClientBuilder {
    install_provider();
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
}

pub fn client() -> reqwest::Client {
    client_builder().build().expect("HTTP client")
}
