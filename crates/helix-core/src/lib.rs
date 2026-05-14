//! HelixDB HTTP client helper plus four named runtime contracts (C1–C4)
//! covering the primitives the course teaches: vertex round trip, edge
//! traversal, vector search, and idempotent upsert.
//!
//! Each public async fn enforces its contract via an `assert!` immediately
//! after the HTTP round trip — the same pattern duckdb-from-zero and
//! valkey-from-zero use, adapted for HelixDB's REST surface.
//!
//! Helix exposes one HTTP endpoint per `QUERY` declared in `db/queries.hx`
//! (PascalCase). Every response is wrapped in the return-variable name from
//! the query (e.g. `{"d": {...}}`, `{"nbrs": [...]}`, `{"cnt": 1}`); the
//! client unwraps that wrapper for the caller.
//!
//! Formal spec: contracts/helix-rust-v1.yaml.

use anyhow::{anyhow, Context, Result};
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

    /// POST `body` to `/QueryName`, returning the raw JSON response. Errors
    /// surface as `Err`; `GRAPH_ERROR` responses (e.g. unique-index breach)
    /// are returned as `Ok(Value)` so callers can decide whether the error
    /// is expected (C4 upsert) or terminal (C1/C2/C3).
    pub async fn call(&self, query: &str, body: Value) -> Result<Value> {
        let url = format!("{}/{}", self.base_url, query);
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

    /// Unwrap Helix's return-variable wrapper: `{"d": <value>}` → `<value>`.
    pub fn unwrap_return<'a>(resp: &'a Value, var: &str) -> Result<&'a Value> {
        resp.get(var)
            .ok_or_else(|| anyhow!("expected return variable `{var}` in {resp}"))
    }
}

/// A vertex returned by `InsertDocument` / `GetDocumentByTitle`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    #[serde(rename = "Title")]
    pub title: String,
}

/// C1 vertex_round_trip — inserting a document by title and fetching it by
/// title returns the same id + title.
pub async fn vertex_round_trip(client: &HelixClient, title: &str) -> Result<Document> {
    let inserted = client
        .call("InsertDocument", json!({ "title": title }))
        .await?;
    let inserted_doc: Document =
        serde_json::from_value(HelixClient::unwrap_return(&inserted, "d")?.clone())
            .context("InsertDocument returned malformed payload")?;
    let fetched = client
        .call("GetDocumentByTitle", json!({ "title": title }))
        .await?;
    let fetched_doc: Document =
        serde_json::from_value(HelixClient::unwrap_return(&fetched, "d")?.clone())
            .context("GetDocumentByTitle returned malformed payload")?;
    // Provable contract C1 vertex_round_trip
    assert_eq!(
        fetched_doc.id, inserted_doc.id,
        "C1 vertex_round_trip: GetDocumentByTitle must return the same id as InsertDocument"
    );
    assert_eq!(
        fetched_doc.title, title,
        "C1 vertex_round_trip: title must round-trip"
    );
    Ok(fetched_doc)
}

/// C2 edge_traversal — inserting `from --Related[kind]--> to` then traversing
/// the neighbours of `from` returns `to` in the result set.
pub async fn edge_traversal(
    client: &HelixClient,
    from_title: &str,
    to_title: &str,
    kind: &str,
) -> Result<Vec<Document>> {
    let _ = client
        .call(
            "InsertRelated",
            json!({ "from_title": from_title, "to_title": to_title, "kind": kind }),
        )
        .await?;
    let resp = client
        .call("Neighbours", json!({ "title": from_title }))
        .await?;
    let nbrs: Vec<Document> =
        serde_json::from_value(HelixClient::unwrap_return(&resp, "nbrs")?.clone())
            .context("Neighbours returned malformed payload")?;
    // Provable contract C2 edge_traversal
    assert!(
        nbrs.iter().any(|d| d.title == to_title),
        "C2 edge_traversal: traversing from {from_title} via Related[{kind}] must include {to_title}"
    );
    Ok(nbrs)
}

/// One hit returned by `VectorSearch` — vector + similarity score + the
/// `DocTitle` tag that maps the embedding back to its source vertex.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorHit {
    pub id: String,
    #[serde(rename = "DocTitle")]
    pub doc_title: String,
    pub score: f64,
}

/// C3 vector_top_k_contains_self — inserting an embedding tagged with a
/// document title, then searching with that same embedding, returns the
/// inserted vector as the top-1 result (cosine distance ≈ 0).
pub async fn vector_top_k_contains_self(
    client: &HelixClient,
    title: &str,
    embedding: &[f64],
    k: i32,
) -> Result<Vec<VectorHit>> {
    let _ = client
        .call("InsertVector", json!({ "title": title, "vec": embedding }))
        .await?;
    let resp = client
        .call("VectorSearch", json!({ "vec": embedding, "k": k }))
        .await?;
    let hits: Vec<VectorHit> =
        serde_json::from_value(HelixClient::unwrap_return(&resp, "hits")?.clone())
            .context("VectorSearch returned malformed payload")?;
    // Provable contract C3 vector_top_k_contains_self
    assert!(
        !hits.is_empty(),
        "C3 vector_top_k_contains_self: VectorSearch must return at least one hit"
    );
    assert_eq!(
        hits[0].doc_title, title,
        "C3 vector_top_k_contains_self: top-1 must be the source vector tagged {title}"
    );
    Ok(hits)
}

/// C4 upsert_idempotent — `UNIQUE INDEX Title` is the enforcement primitive.
/// First `InsertDocument(title)` succeeds; second one returns a
/// `GRAPH_ERROR` rather than silently duplicating; `CountByTitle(title)`
/// returns exactly 1.
pub async fn upsert_idempotent(client: &HelixClient, title: &str) -> Result<u64> {
    // First insert — we don't care whether the vertex already exists; we
    // only care that after both calls the count is exactly 1.
    let _ = client
        .call("InsertDocument", json!({ "title": title }))
        .await?;
    // Second insert — expected to surface `GRAPH_ERROR` from the UNIQUE
    // INDEX. That is part of the contract: the engine refuses to duplicate.
    let dup = client
        .call("InsertDocument", json!({ "title": title }))
        .await?;
    let resp = client
        .call("CountByTitle", json!({ "title": title }))
        .await?;
    let count: u64 = serde_json::from_value(HelixClient::unwrap_return(&resp, "cnt")?.clone())
        .context("CountByTitle returned malformed payload")?;
    // Provable contract C4 upsert_idempotent
    assert_eq!(
        count, 1,
        "C4 upsert_idempotent: inserting the same title twice must leave exactly one vertex \
         (second-insert response was {dup})"
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
        };
        let s = serde_json::to_string(&d).expect("serialize");
        let back: Document = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back.id, "doc-1");
        assert_eq!(back.title, "hello");
    }

    /// Helix's wire format wraps return values in the query's return
    /// variable name. `unwrap_return` peels that wrapper.
    #[test]
    fn unwraps_return_variable_wrapper() {
        let wrapped =
            serde_json::from_str::<Value>(r#"{"d":{"id":"abc","Title":"hello"}}"#).unwrap();
        let inner = HelixClient::unwrap_return(&wrapped, "d").expect("unwrap d");
        let doc: Document = serde_json::from_value(inner.clone()).expect("deserialize Document");
        assert_eq!(doc.id, "abc");
        assert_eq!(doc.title, "hello");
    }

    #[test]
    fn helix_client_constructs_with_base_url() {
        let c = HelixClient::new("http://127.0.0.1:6969");
        assert_eq!(c.base_url, "http://127.0.0.1:6969");
    }
}
