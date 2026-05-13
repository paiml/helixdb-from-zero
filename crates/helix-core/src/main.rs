//! `helix-demo` — runs the four named runtime contracts against a live
//! HelixDB instance and prints which assertion each one checked.
//!
//! Assumes:
//!   1. `make up` brought up HelixDB at the default URL (env `HELIX_URL`,
//!      defaulting to http://127.0.0.1:6969).
//!   2. The schema in `helix/schema.hx` has been pushed via `helix push`
//!      so the endpoints (`/insert_document`, `/get_document`, `/insert_edge`,
//!      `/neighbours`, `/vector_search`, `/count_by_id`) are mounted.
//!
//! Run with `make demo` from the repo root.

use anyhow::Result;
use helix_core::{
    edge_traversal, upsert_idempotent, vector_top_k_contains_self, vertex_round_trip, Document,
    HelixClient, DEFAULT_URL,
};

#[tokio::main]
async fn main() -> Result<()> {
    let url = std::env::var("HELIX_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    println!("HelixDB demo · client → {url}");
    let client = HelixClient::new(url);

    let doc = Document {
        id: "doc-1".into(),
        title: "from-zero".into(),
        embedding: vec![0.1, 0.2, 0.3, 0.4],
    };

    println!("\nC1 vertex_round_trip:");
    let back = vertex_round_trip(&client, &doc).await?;
    println!("  got id={} title={} ✓", back.id, back.title);

    let neighbour = Document {
        id: "doc-2".into(),
        title: "neighbour".into(),
        embedding: vec![0.11, 0.21, 0.31, 0.41],
    };
    let _ = vertex_round_trip(&client, &neighbour).await?;

    println!("\nC2 edge_traversal:");
    let neighbours = edge_traversal(&client, &doc.id, &neighbour.id, "related_to").await?;
    println!("  {} → related_to → {:?} ✓", doc.id, neighbours);

    println!("\nC3 vector_top_k_contains_self:");
    let hits = vector_top_k_contains_self(&client, &doc, 5).await?;
    println!("  top-{} hits = {:?} ✓", hits.len(), hits);

    println!("\nC4 upsert_idempotent:");
    let count = upsert_idempotent(&client, &doc).await?;
    println!("  count_by_id({}) = {} ✓", doc.id, count);

    println!("\n4 / 4 contracts asserted.");
    Ok(())
}
