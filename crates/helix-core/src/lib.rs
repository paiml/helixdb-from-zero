//! HelixDB HTTP client helper plus four named runtime contracts (C1–C4)
//! covering the primitives the course teaches: vertex round trip, edge
//! traversal, vector search, and idempotent upsert.
//!
//! Each public async fn enforces its contract via an `assert!` immediately
//! after the HTTP round trip — the same pattern duckdb-from-zero and
//! valkey-from-zero use, adapted for HelixDB's REST surface.
//!
//! Formal spec: contracts/helix-rust-v1.yaml.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const DEFAULT_URL: &str = "http://127.0.0.1:6969";

/// Thin async client wrapping `reqwest::Client` with a base URL.
#[derive(Clone, Debug)]
pub struct HelixClient {
    base_url: String,
    http: Client,
}

impl HelixClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: Client::new(),
        }
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("HelixDB POST {url} failed"))?;
        let status = resp.status();
        let value: Value = resp
            .json()
            .await
            .with_context(|| format!("HelixDB POST {url} returned non-JSON (status={status})"))?;
        Ok(value)
    }
}

/// One vertex round-trip-able by the four named contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub embedding: Vec<f32>,
}

/// C1 vertex_round_trip — fetch-by-id after insert returns the same vertex.
pub async fn vertex_round_trip(client: &HelixClient, doc: &Document) -> Result<Document> {
    let _ = client
        .post(
            "/insert_document",
            json!({ "id": doc.id, "title": doc.title, "embedding": doc.embedding }),
        )
        .await?;
    let fetched: Document = serde_json::from_value(
        client
            .post("/get_document", json!({ "id": doc.id }))
            .await?,
    )
    .context("get_document returned malformed payload")?;
    // Provable contract C1 vertex_round_trip
    assert_eq!(
        fetched.id, doc.id,
        "C1 vertex_round_trip: get_document(id) must return the same id"
    );
    assert_eq!(
        fetched.title, doc.title,
        "C1 vertex_round_trip: title must round-trip"
    );
    Ok(fetched)
}

/// C2 edge_traversal — inserting A→B then traversing from A returns B in the
/// neighbour set.
pub async fn edge_traversal(
    client: &HelixClient,
    from_id: &str,
    to_id: &str,
    label: &str,
) -> Result<Vec<String>> {
    let _ = client
        .post(
            "/insert_edge",
            json!({ "from": from_id, "to": to_id, "label": label }),
        )
        .await?;
    let resp = client
        .post("/neighbours", json!({ "id": from_id, "label": label }))
        .await?;
    let neighbours: Vec<String> =
        serde_json::from_value(resp).context("neighbours returned malformed payload")?;
    // Provable contract C2 edge_traversal
    assert!(
        neighbours.iter().any(|id| id == to_id),
        "C2 edge_traversal: traversing from {from_id} via {label} must include {to_id}"
    );
    Ok(neighbours)
}

/// C3 vector_top_k_contains_self — searching for a vector with itself as the
/// query returns the source vertex as the top-1 result.
pub async fn vector_top_k_contains_self(
    client: &HelixClient,
    doc: &Document,
    k: usize,
) -> Result<Vec<String>> {
    let resp = client
        .post(
            "/vector_search",
            json!({ "embedding": doc.embedding, "k": k }),
        )
        .await?;
    let hits: Vec<String> =
        serde_json::from_value(resp).context("vector_search returned malformed payload")?;
    // Provable contract C3 vector_top_k_contains_self
    assert!(
        !hits.is_empty(),
        "C3 vector_top_k_contains_self: vector_search must return at least one hit"
    );
    assert_eq!(
        hits[0], doc.id,
        "C3 vector_top_k_contains_self: top-1 must be the source vertex {}",
        doc.id
    );
    Ok(hits)
}

/// C4 upsert_idempotent — inserting the same vertex twice does not duplicate
/// it; a count-by-id query returns exactly 1.
pub async fn upsert_idempotent(client: &HelixClient, doc: &Document) -> Result<u64> {
    let _ = vertex_round_trip(client, doc).await?;
    let _ = vertex_round_trip(client, doc).await?;
    let resp = client.post("/count_by_id", json!({ "id": doc.id })).await?;
    let count: u64 =
        serde_json::from_value(resp).context("count_by_id returned malformed payload")?;
    // Provable contract C4 upsert_idempotent
    assert_eq!(
        count, 1,
        "C4 upsert_idempotent: inserting the same id twice must leave exactly one vertex"
    );
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Document` serialization is round-trippable through `serde_json`.
    /// (Unit-level — does not require a live HelixDB server.)
    #[test]
    fn document_json_round_trip() {
        let d = Document {
            id: "doc-1".into(),
            title: "hello".into(),
            embedding: vec![0.1, 0.2, 0.3],
        };
        let s = serde_json::to_string(&d).expect("serialize");
        let back: Document = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back.id, "doc-1");
        assert_eq!(back.title, "hello");
        assert_eq!(back.embedding, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn helix_client_constructs_with_base_url() {
        let c = HelixClient::new("http://127.0.0.1:6969");
        assert_eq!(c.base_url, "http://127.0.0.1:6969");
    }
}
