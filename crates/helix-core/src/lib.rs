//! HelixDB client wrapper plus four named runtime contracts (C1–C4)
//! covering the primitives the course teaches: vertex round trip, edge
//! traversal, vector search, and idempotent upsert.
//!
//! Each public async fn enforces its contract via an `assert!` immediately
//! after the HTTP round trip — the same pattern duckdb-from-zero and
//! valkey-from-zero use, adapted for HelixDB's REST surface.
//!
//! The HTTP transport is provided by `helix_rs::HelixDB` (the official
//! HelixDB Rust SDK). Each `QUERY <Name>` declared in `db/queries.hx` is
//! mounted at `POST /<Name>` on the running instance; helix-rs's
//! `HelixDBClient::query::<Input, Output>` handles serde encoding/decoding
//! and the `{return_var: value}` wrapper that Helix puts around every
//! return value.
//!
//! Formal spec: contracts/helix-rust-v1.yaml.

use anyhow::{Context, Result};
use helix_rs::{HelixDB, HelixDBClient};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const DEFAULT_URL: &str = "http://127.0.0.1";
pub const DEFAULT_PORT: u16 = 6969;

/// Thin wrapper over `helix_rs::HelixDB` carrying the four named runtime
/// contracts. Built from a `host:port` pair via [`HelixClient::new`].
#[derive(Debug, Clone)]
pub struct HelixClient {
    inner: HelixDB,
}

impl HelixClient {
    /// Connect by URL: `http://127.0.0.1:6969` etc. Splits the URL into
    /// (endpoint, port) the way `helix_rs::HelixDB::new` expects.
    pub fn new(url: &str) -> Self {
        let (endpoint, port) = split_url(url);
        Self {
            inner: HelixDB::new(Some(&endpoint), port, None),
        }
    }

    /// Direct construction from an already-built `helix_rs::HelixDB`.
    /// Useful for tests or for callers that need to pass an `api_key`.
    pub fn from_helix_db(db: HelixDB) -> Self {
        Self { inner: db }
    }

    /// Borrow the underlying `helix_rs::HelixDB` for advanced uses (custom
    /// query bindings, generic queries not covered by the four contracts).
    pub fn raw(&self) -> &HelixDB {
        &self.inner
    }
}

fn split_url(url: &str) -> (String, Option<u16>) {
    if let Some(rest) = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")) {
        let scheme_prefix = if url.starts_with("https://") {
            "https://"
        } else {
            "http://"
        };
        if let Some((host, port_str)) = rest.rsplit_once(':') {
            if let Ok(port) = port_str.parse::<u16>() {
                return (format!("{scheme_prefix}{host}"), Some(port));
            }
        }
        return (format!("{scheme_prefix}{rest}"), None);
    }
    (url.to_string(), None)
}

/// A vertex returned by `InsertDocument` / `GetDocumentByTitle`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    #[serde(rename = "Title")]
    pub title: String,
}

#[derive(Debug, Deserialize)]
struct InsertDocumentResp {
    d: Document,
}

#[derive(Debug, Deserialize)]
struct GetDocumentResp {
    d: Document,
}

#[derive(Debug, Deserialize)]
struct NeighboursResp {
    nbrs: Vec<Document>,
}

#[derive(Debug, Deserialize)]
struct VectorSearchResp {
    hits: Vec<VectorHit>,
}

#[derive(Debug, Deserialize)]
struct CountResp {
    cnt: u64,
}

/// C1 vertex_round_trip — inserting a document by title and fetching it by
/// title returns the same id + title.
pub async fn vertex_round_trip(client: &HelixClient, title: &str) -> Result<Document> {
    let ins: InsertDocumentResp = client
        .inner
        .query("InsertDocument", &json!({ "title": title }))
        .await
        .context("InsertDocument failed")?;
    let got: GetDocumentResp = client
        .inner
        .query("GetDocumentByTitle", &json!({ "title": title }))
        .await
        .context("GetDocumentByTitle failed")?;
    // Provable contract C1 vertex_round_trip
    assert_eq!(
        got.d.id, ins.d.id,
        "C1 vertex_round_trip: GetDocumentByTitle must return the same id as InsertDocument"
    );
    assert_eq!(
        got.d.title, title,
        "C1 vertex_round_trip: title must round-trip"
    );
    Ok(got.d)
}

/// C2 edge_traversal — inserting `from --Related[kind]--> to` then traversing
/// the neighbours of `from` returns `to` in the result set.
pub async fn edge_traversal(
    client: &HelixClient,
    from_title: &str,
    to_title: &str,
    kind: &str,
) -> Result<Vec<Document>> {
    let _: serde_json::Value = client
        .inner
        .query(
            "InsertRelated",
            &json!({ "from_title": from_title, "to_title": to_title, "kind": kind }),
        )
        .await
        .context("InsertRelated failed")?;
    let resp: NeighboursResp = client
        .inner
        .query("Neighbours", &json!({ "title": from_title }))
        .await
        .context("Neighbours failed")?;
    // Provable contract C2 edge_traversal
    assert!(
        resp.nbrs.iter().any(|d| d.title == to_title),
        "C2 edge_traversal: traversing from {from_title} via Related[{kind}] must include {to_title}"
    );
    Ok(resp.nbrs)
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
    let _: serde_json::Value = client
        .inner
        .query("InsertVector", &json!({ "title": title, "vec": embedding }))
        .await
        .context("InsertVector failed")?;
    let resp: VectorSearchResp = client
        .inner
        .query("VectorSearch", &json!({ "vec": embedding, "k": k }))
        .await
        .context("VectorSearch failed")?;
    // Provable contract C3 vector_top_k_contains_self
    assert!(
        !resp.hits.is_empty(),
        "C3 vector_top_k_contains_self: VectorSearch must return at least one hit"
    );
    assert_eq!(
        resp.hits[0].doc_title, title,
        "C3 vector_top_k_contains_self: top-1 must be the source vector tagged {title}"
    );
    Ok(resp.hits)
}

/// C4 upsert_idempotent — `UNIQUE INDEX Title` is the enforcement primitive.
/// First `InsertDocument(title)` succeeds; second one returns a
/// `GRAPH_ERROR` rather than silently duplicating; `CountByTitle(title)`
/// returns exactly 1.
pub async fn upsert_idempotent(client: &HelixClient, title: &str) -> Result<u64> {
    // First insert — we don't care whether the vertex already exists; we
    // only care that after both calls the count is exactly 1.
    let _: serde_json::Value = client
        .inner
        .query("InsertDocument", &json!({ "title": title }))
        .await
        .context("first InsertDocument failed")?;
    // Second insert — expected to surface `GRAPH_ERROR` from the UNIQUE
    // INDEX. helix-rs maps that to `Err(HelixError::RemoteError)`, which is
    // part of the contract: the engine refuses to duplicate.
    let dup_msg = match client
        .inner
        .query::<_, serde_json::Value>("InsertDocument", &json!({ "title": title }))
        .await
    {
        Ok(v) => format!("unexpected Ok({v})"),
        Err(e) => e.to_string(),
    };
    let resp: CountResp = client
        .inner
        .query("CountByTitle", &json!({ "title": title }))
        .await
        .context("CountByTitle failed")?;
    // Provable contract C4 upsert_idempotent
    assert_eq!(
        resp.cnt, 1,
        "C4 upsert_idempotent: inserting the same title twice must leave exactly one vertex \
         (second-insert response was {dup_msg})"
    );
    Ok(resp.cnt)
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
    /// variable name (`{"d": {...}}`). The per-query response structs
    /// deserialize that wrapper directly.
    #[test]
    fn unwraps_return_variable_wrapper() {
        let wrapped: InsertDocumentResp =
            serde_json::from_str(r#"{"d":{"id":"abc","Title":"hello"}}"#).expect("deserialize");
        assert_eq!(wrapped.d.id, "abc");
        assert_eq!(wrapped.d.title, "hello");
    }

    #[test]
    fn helix_client_constructs_from_url() {
        let c = HelixClient::new("http://127.0.0.1:6969");
        // `helix_rs::HelixDB` doesn't expose endpoint/port getters, so we
        // exercise the URL split helper directly to confirm parsing.
        let (host, port) = split_url("http://127.0.0.1:6969");
        assert_eq!(host, "http://127.0.0.1");
        assert_eq!(port, Some(6969));
        // and the client constructed without panicking
        let _ = c.raw();
    }

    #[test]
    fn split_url_handles_host_without_port() {
        let (host, port) = split_url("http://example.com");
        assert_eq!(host, "http://example.com");
        assert_eq!(port, None);
    }
}
