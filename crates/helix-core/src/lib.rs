//! HelixDB client wrapper plus four named runtime contracts (C1–C4)
//! covering the primitives the course teaches: vertex round trip, edge
//! traversal, vector search, and idempotent upsert.
//!
//! Each public async fn enforces its contract via an `assert!` immediately
//! after the HTTP round trip — the same pattern duckdb-from-zero and
//! valkey-from-zero use, adapted for HelixDB's REST surface.
//!
//! The HTTP transport is provided by the official HelixDB Rust SDK,
//! [`helix_rs::HelixDB`]. The four contracts are *generic over*
//! [`helix_rs::HelixDBClient`] so the same code drives both the real
//! engine and an in-memory mock used by the unit tests — that mock is
//! what carries unit-test coverage of the HTTP-touching code paths to
//! 100% without needing a live `helix push dev` instance.
//!
//! Formal spec: contracts/helix-rust-v1.yaml.

use anyhow::{Context, Result};
use helix_rs::{HelixDB, HelixDBClient};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const DEFAULT_URL: &str = "http://127.0.0.1";
pub const DEFAULT_PORT: u16 = 6969;

/// Thin convenience wrapper around [`helix_rs::HelixDB`]. Built from a
/// URL via [`HelixClient::new`]; the contracts and [`run`] take any
/// `&impl HelixDBClient`, including the wrapped engine via
/// [`HelixClient::raw`].
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
    /// Useful for callers that need to pass an `api_key`.
    pub fn from_helix_db(db: HelixDB) -> Self {
        Self { inner: db }
    }

    /// Borrow the underlying `helix_rs::HelixDB`. `HelixDB` implements
    /// `HelixDBClient` directly, so this is what gets passed into
    /// [`run`] and the four contract functions.
    pub fn raw(&self) -> &HelixDB {
        &self.inner
    }
}

fn split_url(url: &str) -> (String, Option<u16>) {
    let (scheme, rest) = if let Some(r) = url.strip_prefix("https://") {
        ("https://", r)
    } else if let Some(r) = url.strip_prefix("http://") {
        ("http://", r)
    } else {
        return (url.to_string(), None);
    };
    if let Some((host, port_str)) = rest.rsplit_once(':') {
        if let Ok(port) = port_str.parse::<u16>() {
            return (format!("{scheme}{host}"), Some(port));
        }
    }
    (format!("{scheme}{rest}"), None)
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
pub async fn vertex_round_trip<C>(client: &C, title: &str) -> Result<Document>
where
    C: HelixDBClient + Sync,
    C::Err: std::error::Error + Send + Sync + 'static,
{
    let ins: InsertDocumentResp = client
        .query("InsertDocument", &json!({ "title": title }))
        .await
        .context("InsertDocument failed")?;
    let got: GetDocumentResp = client
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
pub async fn edge_traversal<C>(
    client: &C,
    from_title: &str,
    to_title: &str,
    kind: &str,
) -> Result<Vec<Document>>
where
    C: HelixDBClient + Sync,
    C::Err: std::error::Error + Send + Sync + 'static,
{
    let _: serde_json::Value = client
        .query(
            "InsertRelated",
            &json!({ "from_title": from_title, "to_title": to_title, "kind": kind }),
        )
        .await
        .context("InsertRelated failed")?;
    let resp: NeighboursResp = client
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

/// One hit returned by `VectorSearch` — id + similarity score + the
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
pub async fn vector_top_k_contains_self<C>(
    client: &C,
    title: &str,
    embedding: &[f64],
    k: i32,
) -> Result<Vec<VectorHit>>
where
    C: HelixDBClient + Sync,
    C::Err: std::error::Error + Send + Sync + 'static,
{
    let _: serde_json::Value = client
        .query("InsertVector", &json!({ "title": title, "vec": embedding }))
        .await
        .context("InsertVector failed")?;
    let resp: VectorSearchResp = client
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
/// `GRAPH_ERROR` (surfaced by helix-rs as `Err(HelixError::RemoteError)`)
/// OR silently succeeds (tolerant backend). Either way `CountByTitle(title)`
/// must return exactly 1 — that is the source of truth the assertion
/// checks.
pub async fn upsert_idempotent<C>(client: &C, title: &str) -> Result<u64>
where
    C: HelixDBClient + Sync,
    C::Err: std::error::Error + Send + Sync + 'static,
{
    let _: serde_json::Value = client
        .query("InsertDocument", &json!({ "title": title }))
        .await
        .context("first InsertDocument failed")?;
    let dup_msg = match client
        .query::<_, serde_json::Value>("InsertDocument", &json!({ "title": title }))
        .await
    {
        Ok(v) => format!("unexpected Ok({v})"),
        Err(e) => e.to_string(),
    };
    let resp: CountResp = client
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

/// End-to-end driver that walks all four contracts. The `suffix` is mixed
/// into vertex titles + the embedding so repeated runs against a persisted
/// volume don't collide on `UNIQUE INDEX Title` and the C3 top-1 is
/// guaranteed to be *this* run's insert. Used by the `helix-demo` binary
/// against a live engine and by the unit tests against `MockHelix`.
pub async fn run<C>(client: &C, suffix: &str) -> Result<()>
where
    C: HelixDBClient + Sync,
    C::Err: std::error::Error + Send + Sync + 'static,
{
    let alpha = format!("alpha-{suffix}");
    let beta = format!("beta-{suffix}");
    let gamma = format!("gamma-{suffix}");

    println!("\nC1 vertex_round_trip:");
    let back = vertex_round_trip(client, &alpha).await?;
    println!("  got id={} title={} ✓", back.id, back.title);

    let _ = vertex_round_trip(client, &beta).await?;

    println!("\nC2 edge_traversal:");
    let nbrs = edge_traversal(client, &alpha, &beta, "cites").await?;
    println!(
        "  {} → cites → {:?} ✓",
        alpha,
        nbrs.iter().map(|d| &d.title).collect::<Vec<_>>()
    );

    println!("\nC3 vector_top_k_contains_self:");
    let seed: f64 = (suffix
        .chars()
        .filter_map(|c| c.to_digit(10))
        .fold(0u32, |acc, d| acc.wrapping_mul(10).wrapping_add(d))
        % 1_000_000) as f64
        / 1_000_000.0;
    let embedding = [seed, seed + 0.1, seed + 0.2, seed + 0.3];
    let hits = vector_top_k_contains_self(client, &alpha, &embedding, 5).await?;
    println!(
        "  top-{} hits = {:?} ✓",
        hits.len(),
        hits.iter().map(|h| &h.doc_title).collect::<Vec<_>>()
    );

    println!("\nC4 upsert_idempotent:");
    let count = upsert_idempotent(client, &gamma).await?;
    println!("  count_by_title({gamma}) = {count} ✓");

    println!("\n4 / 4 contracts asserted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use helix_rs::HelixError;
    use std::collections::HashMap;
    use std::panic::AssertUnwindSafe;
    use std::sync::Mutex;

    /// In-memory `HelixDBClient` impl used by the unit tests. Configurable
    /// behaviour flags simulate buggy backends so the contract panics can
    /// be exercised without a live engine.
    #[derive(Default)]
    struct MockHelix {
        state: Mutex<MockState>,
        config: MockConfig,
    }

    struct MockEdge {
        from_title: String,
        to_title: String,
        /// Stored for fidelity with the real `Related.Kind` property even
        /// though `Neighbours` doesn't filter by it.
        #[allow(dead_code)]
        kind: String,
    }

    struct MockVector {
        id: String,
        doc_title: String,
        embedding: Vec<f64>,
    }

    #[derive(Default)]
    struct MockState {
        documents: HashMap<String, Document>,
        edges: Vec<MockEdge>,
        vectors: Vec<MockVector>,
        next_id: u64,
    }

    #[derive(Default, Clone, Copy)]
    struct MockConfig {
        /// If true, `InsertDocument` always succeeds (no UNIQUE INDEX).
        tolerant_inserts: bool,
        /// If true, `Neighbours` always returns an empty list.
        lie_about_neighbours: bool,
        /// If true, `VectorSearch` always returns an empty list.
        empty_vector_search: bool,
        /// If true, `CountByTitle` always returns 2.
        always_count_two: bool,
    }

    impl MockHelix {
        fn with_config(config: MockConfig) -> Self {
            Self {
                state: Mutex::new(MockState::default()),
                config,
            }
        }

        fn dispatch(
            &self,
            endpoint: &str,
            req: serde_json::Value,
        ) -> Result<serde_json::Value, HelixError> {
            let mut st = self.state.lock().unwrap();
            match endpoint {
                "InsertDocument" => {
                    let title = req["title"].as_str().unwrap().to_string();
                    if !self.config.tolerant_inserts && st.documents.contains_key(&title) {
                        return Err(HelixError::RemoteError {
                            details: "Duplicate key on unique index: Title".into(),
                        });
                    }
                    st.next_id += 1;
                    let doc = Document {
                        id: format!("mock-doc-{}", st.next_id),
                        title: title.clone(),
                    };
                    st.documents.insert(title.clone(), doc.clone());
                    Ok(json!({ "d": { "id": doc.id, "Title": doc.title } }))
                }
                "GetDocumentByTitle" => {
                    let title = req["title"].as_str().unwrap();
                    let doc = st.documents.get(title).cloned().unwrap();
                    Ok(json!({ "d": { "id": doc.id, "Title": doc.title } }))
                }
                "CountByTitle" => {
                    let cnt = if self.config.always_count_two {
                        2u64
                    } else {
                        let title = req["title"].as_str().unwrap();
                        u64::from(st.documents.contains_key(title))
                    };
                    Ok(json!({ "cnt": cnt }))
                }
                "InsertRelated" => {
                    let from_title = req["from_title"].as_str().unwrap().to_string();
                    let to_title = req["to_title"].as_str().unwrap().to_string();
                    let kind = req["kind"].as_str().unwrap().to_string();
                    st.edges.push(MockEdge {
                        from_title,
                        to_title,
                        kind,
                    });
                    Ok(json!({ "e": { "id": "mock-edge" } }))
                }
                "Neighbours" => {
                    if self.config.lie_about_neighbours {
                        return Ok(json!({ "nbrs": [] }));
                    }
                    let title = req["title"].as_str().unwrap().to_string();
                    let nbrs: Vec<serde_json::Value> = st
                        .edges
                        .iter()
                        .filter(|e| e.from_title == title)
                        .filter_map(|e| st.documents.get(&e.to_title))
                        .map(|d| json!({ "id": d.id, "Title": d.title }))
                        .collect();
                    Ok(json!({ "nbrs": nbrs }))
                }
                "InsertVector" => {
                    let doc_title = req["title"].as_str().unwrap().to_string();
                    let embedding: Vec<f64> = serde_json::from_value(req["vec"].clone()).unwrap();
                    st.next_id += 1;
                    let id = format!("mock-vec-{}", st.next_id);
                    st.vectors.push(MockVector {
                        id: id.clone(),
                        doc_title,
                        embedding,
                    });
                    Ok(json!({ "v": { "id": id } }))
                }
                "VectorSearch" => {
                    if self.config.empty_vector_search {
                        return Ok(json!({ "hits": [] }));
                    }
                    let q: Vec<f64> = serde_json::from_value(req["vec"].clone()).unwrap();
                    let k = req["k"].as_i64().unwrap().max(0) as usize;
                    let mut hits: Vec<(f64, &MockVector)> = st
                        .vectors
                        .iter()
                        .map(|v| {
                            let d: f64 = v
                                .embedding
                                .iter()
                                .zip(q.iter())
                                .map(|(a, b)| (a - b).powi(2))
                                .sum();
                            (d, v)
                        })
                        .collect();
                    hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    hits.truncate(k);
                    let payload: Vec<serde_json::Value> = hits
                        .into_iter()
                        .map(|(score, v)| {
                            json!({ "id": v.id, "DocTitle": v.doc_title, "score": score })
                        })
                        .collect();
                    Ok(json!({ "hits": payload }))
                }
                other => Err(HelixError::RemoteError {
                    details: format!("unknown endpoint {other}"),
                }),
            }
        }
    }

    impl HelixDBClient for MockHelix {
        type Err = HelixError;

        fn new(_: Option<&str>, _: Option<u16>, _: Option<&str>) -> Self {
            Self::default()
        }

        async fn query<T, R>(&self, endpoint: &str, data: &T) -> Result<R, HelixError>
        where
            T: Serialize + Sync,
            R: for<'de> Deserialize<'de>,
        {
            let req = serde_json::to_value(data).unwrap();
            let resp = self.dispatch(endpoint, req)?;
            Ok(serde_json::from_value(resp).unwrap())
        }
    }

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

    #[test]
    fn vector_hit_json_round_trip() {
        let h = VectorHit {
            id: "v-1".into(),
            doc_title: "hello".into(),
            score: 0.5,
        };
        let s = serde_json::to_string(&h).expect("serialize");
        let back: VectorHit = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back.id, "v-1");
        assert_eq!(back.doc_title, "hello");
        assert_eq!(back.score, 0.5);
    }

    #[test]
    fn split_url_http_with_port() {
        let (host, port) = split_url("http://127.0.0.1:6969");
        assert_eq!(host, "http://127.0.0.1");
        assert_eq!(port, Some(6969));
    }

    #[test]
    fn split_url_https_with_port() {
        let (host, port) = split_url("https://example.com:443");
        assert_eq!(host, "https://example.com");
        assert_eq!(port, Some(443));
    }

    #[test]
    fn split_url_no_port() {
        let (host, port) = split_url("http://example.com");
        assert_eq!(host, "http://example.com");
        assert_eq!(port, None);
    }

    #[test]
    fn split_url_no_scheme() {
        let (host, port) = split_url("example.com:1234");
        assert_eq!(host, "example.com:1234");
        assert_eq!(port, None);
    }

    #[test]
    fn split_url_bad_port_falls_back_to_no_port() {
        let (host, port) = split_url("http://host:not-a-port");
        assert_eq!(host, "http://host:not-a-port");
        assert_eq!(port, None);
    }

    #[test]
    fn helix_client_constructors() {
        let c = HelixClient::new("http://127.0.0.1:6969");
        let _: &HelixDB = c.raw();
        let bare = HelixDB::new(Some("http://127.0.0.1"), Some(6969), None);
        let c2 = HelixClient::from_helix_db(bare);
        let _: &HelixDB = c2.raw();
        let cloned = c.clone();
        let _: &HelixDB = cloned.raw();
        // Debug impl coverage
        let _ = format!("{c:?}");
    }

    #[test]
    fn mock_constructible_via_trait_new() {
        let m = <MockHelix as HelixDBClient>::new(None, None, None);
        // smoke: default state is empty
        assert!(m.state.lock().unwrap().documents.is_empty());
    }

    #[tokio::test]
    async fn mock_returns_unknown_endpoint_error() {
        let m = MockHelix::default();
        let result: Result<serde_json::Value, _> = m.query("DoesNotExist", &json!({})).await;
        let err = result.expect_err("unknown endpoint should error");
        assert!(err.to_string().contains("unknown endpoint"));
    }

    #[tokio::test]
    async fn c1_vertex_round_trip_against_mock() {
        let mock = MockHelix::default();
        let got = vertex_round_trip(&mock, "hello").await.unwrap();
        assert_eq!(got.title, "hello");
        assert!(got.id.starts_with("mock-doc-"));
    }

    #[tokio::test]
    async fn c2_edge_traversal_against_mock() {
        let mock = MockHelix::default();
        let _ = vertex_round_trip(&mock, "a").await.unwrap();
        let _ = vertex_round_trip(&mock, "b").await.unwrap();
        let nbrs = edge_traversal(&mock, "a", "b", "cites").await.unwrap();
        assert!(nbrs.iter().any(|d| d.title == "b"));
    }

    #[tokio::test]
    async fn c3_vector_top_k_contains_self_against_mock() {
        let mock = MockHelix::default();
        let _ = vertex_round_trip(&mock, "x").await.unwrap();
        let _ = vertex_round_trip(&mock, "y").await.unwrap();
        let _ = vector_top_k_contains_self(&mock, "y", &[10.0, 10.0, 10.0, 10.0], 5)
            .await
            .unwrap();
        let hits = vector_top_k_contains_self(&mock, "x", &[0.0, 0.0, 0.0, 0.0], 5)
            .await
            .unwrap();
        assert_eq!(hits[0].doc_title, "x");
    }

    #[tokio::test]
    async fn c4_upsert_idempotent_against_mock() {
        let mock = MockHelix::default();
        let count = upsert_idempotent(&mock, "only-one").await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn c4_hits_ok_arm_when_backend_is_tolerant() {
        // Tolerant mock: second InsertDocument returns Ok. Count is still
        // 1 (the mock dedups by title in the documents map). C4 assertion
        // holds, and the `Ok(v) => format!("unexpected Ok({v})")` arm of
        // the duplicate-insert match is covered.
        let mock = MockHelix::with_config(MockConfig {
            tolerant_inserts: true,
            ..MockConfig::default()
        });
        let count = upsert_idempotent(&mock, "tolerant").await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn run_drives_all_four_contracts_against_mock() {
        let mock = MockHelix::default();
        run(&mock, "12345").await.expect("4/4 should pass on mock");
    }

    #[tokio::test]
    async fn run_handles_non_numeric_suffix() {
        // Exercises the `to_digit(10)` filter path in `run`: a suffix
        // with no digits collapses to seed=0.
        let mock = MockHelix::default();
        run(&mock, "alpha").await.expect("non-numeric suffix");
    }

    #[tokio::test]
    async fn c2_panics_when_neighbour_missing() {
        // Buggy backend that ACKs InsertRelated but never persists the
        // edge: Neighbours always returns an empty list. C2 assert fires.
        let mock = MockHelix::with_config(MockConfig {
            lie_about_neighbours: true,
            ..MockConfig::default()
        });
        let _ = vertex_round_trip(&mock, "a").await.unwrap();
        let _ = vertex_round_trip(&mock, "b").await.unwrap();
        let result = AssertUnwindSafe(edge_traversal(&mock, "a", "b", "cites"))
            .catch_unwind()
            .await;
        assert!(result.is_err(), "C2 assert should have panicked");
    }

    #[tokio::test]
    async fn c3_panics_when_top_k_is_empty() {
        // Backend that ACKs InsertVector but VectorSearch returns nothing.
        let mock = MockHelix::with_config(MockConfig {
            empty_vector_search: true,
            ..MockConfig::default()
        });
        let result = AssertUnwindSafe(vector_top_k_contains_self(
            &mock,
            "t",
            &[1.0, 2.0, 3.0, 4.0],
            5,
        ))
        .catch_unwind()
        .await;
        assert!(result.is_err(), "C3 empty-hits assert should have panicked");
    }

    #[tokio::test]
    async fn errors_propagate_through_question_mark() {
        // Mock that errors on every query. Each contract function (and
        // `run`) must surface the error as `Err` — not panic — exercising
        // the `?` unwind arm on every `.await?` site in lib.rs.
        struct ErroringMock;
        impl HelixDBClient for ErroringMock {
            type Err = HelixError;
            fn new(_: Option<&str>, _: Option<u16>, _: Option<&str>) -> Self {
                Self
            }
            async fn query<T, R>(&self, _: &str, _: &T) -> Result<R, HelixError>
            where
                T: Serialize + Sync,
                R: for<'de> Deserialize<'de>,
            {
                Err(HelixError::RemoteError {
                    details: "intentional failure".into(),
                })
            }
        }
        // Exercise the trait constructor once so coverage hits the body.
        let _ = <ErroringMock as HelixDBClient>::new(None, None, None);
        assert!(vertex_round_trip(&ErroringMock, "x").await.is_err());
        assert!(edge_traversal(&ErroringMock, "a", "b", "k").await.is_err());
        assert!(vector_top_k_contains_self(&ErroringMock, "x", &[0.0], 5)
            .await
            .is_err());
        assert!(upsert_idempotent(&ErroringMock, "x").await.is_err());
        assert!(run(&ErroringMock, "1").await.is_err());
    }

    #[tokio::test]
    async fn c4_panics_when_count_mismatches() {
        // Tolerant inserts AND a count-of-2 source of truth: C4 assert fires.
        let mock = MockHelix::with_config(MockConfig {
            tolerant_inserts: true,
            always_count_two: true,
            ..MockConfig::default()
        });
        let result = AssertUnwindSafe(upsert_idempotent(&mock, "double"))
            .catch_unwind()
            .await;
        assert!(
            result.is_err(),
            "C4 count mismatch assert should have panicked"
        );
    }
}
