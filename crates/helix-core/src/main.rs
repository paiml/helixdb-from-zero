//! `helix-demo` — runs the four named runtime contracts against a live
//! HelixDB instance via [`helix_core::run`] and prints which assertion
//! each one checked.
//!
//! Prereq: `make up` brought up HelixDB at the default URL (env `HELIX_URL`,
//! defaulting to `http://127.0.0.1:6969`) AND the schema in `db/schema.hx`
//! plus the seven queries in `db/queries.hx` were deployed via `helix push
//! dev`. `make up` does both.

use anyhow::Result;
use helix_core::{run, HelixClient, DEFAULT_PORT, DEFAULT_URL};

#[tokio::main]
async fn main() -> Result<()> {
    let url =
        std::env::var("HELIX_URL").unwrap_or_else(|_| format!("{DEFAULT_URL}:{DEFAULT_PORT}"));
    println!("HelixDB demo · client → {url}");
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string());
    let client = HelixClient::new(&url);
    run(client.raw(), &suffix).await
}
