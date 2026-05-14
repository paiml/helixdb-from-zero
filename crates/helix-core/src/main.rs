//! `helix-demo` — runs the four named runtime contracts against a live
//! HelixDB instance and prints which assertion each one checked.
//!
//! Prereq: `make up` brought up HelixDB at the default URL (env `HELIX_URL`,
//! defaulting to `http://127.0.0.1:6969`) AND the schema in `db/schema.hx`
//! plus the seven queries in `db/queries.hx` were deployed via `helix push
//! dev`. `make up` does both.
//!
//! Run with `make demo` from the repo root.

use anyhow::Result;
use helix_core::{
    edge_traversal, upsert_idempotent, vector_top_k_contains_self, vertex_round_trip, HelixClient,
    DEFAULT_URL,
};

#[tokio::main]
async fn main() -> Result<()> {
    let url = std::env::var("HELIX_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    println!("HelixDB demo · client → {url}");
    let client = HelixClient::new(url);

    // Use timestamp-suffixed titles so re-runs of `make demo` against a
    // persisted volume do not collide on UNIQUE INDEX Title.
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string());

    let alpha = format!("alpha-{suffix}");
    let beta = format!("beta-{suffix}");
    let gamma = format!("gamma-{suffix}");

    println!("\nC1 vertex_round_trip:");
    let back = vertex_round_trip(&client, &alpha).await?;
    println!("  got id={} title={} ✓", back.id, back.title);

    let _ = vertex_round_trip(&client, &beta).await?;

    println!("\nC2 edge_traversal:");
    let nbrs = edge_traversal(&client, &alpha, &beta, "cites").await?;
    println!(
        "  {} → cites → {:?} ✓",
        alpha,
        nbrs.iter().map(|d| &d.title).collect::<Vec<_>>()
    );

    println!("\nC3 vector_top_k_contains_self:");
    // Use a unique embedding per run so the search top-1 is guaranteed to be
    // *this* run's insert, not a stale vector from a prior `make demo`. The
    // suffix is millisecond-resolution; collisions inside one run are not
    // possible.
    let seed: f64 = (suffix.parse::<u128>().unwrap_or(0) % 1_000_000) as f64 / 1_000_000.0;
    let embedding = [seed, seed + 0.1, seed + 0.2, seed + 0.3];
    let hits = vector_top_k_contains_self(&client, &alpha, &embedding, 5).await?;
    println!(
        "  top-{} hits = {:?} ✓",
        hits.len(),
        hits.iter().map(|h| &h.doc_title).collect::<Vec<_>>()
    );

    println!("\nC4 upsert_idempotent:");
    let count = upsert_idempotent(&client, &gamma).await?;
    println!("  count_by_title({gamma}) = {count} ✓");

    println!("\n4 / 4 contracts asserted.");
    Ok(())
}
