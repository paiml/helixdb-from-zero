<p align="center">
  <img src="assets/hero.svg" alt="helixdb-from-zero — Graph + vector primitives in Rust, four runtime contracts, course #20 in the Rust for Data Engineering specialization" width="1280" />
</p>

[![CI](https://github.com/paiml/helixdb-from-zero/actions/workflows/ci.yml/badge.svg)](https://github.com/paiml/helixdb-from-zero/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.95-orange.svg)](rust-toolchain.toml)
[![Contracts](https://img.shields.io/badge/runtime%20contracts-4-brightgreen.svg)](contracts/helix-rust-v1.yaml)

# HelixDB From Zero — Companion Repo

The runnable companion to the Coursera course **HelixDB From Zero**, course
#20 in the *Rust for Data Engineering* specialization.

This repo ships a Helix schema + queries (`db/schema.hx`, `db/queries.hx`),
a small `helix-core` Rust crate that talks to HelixDB over HTTP, and four
named runtime contracts (`C1`–`C4`) that the demo binary asserts against a
live instance.

## Quick start

```bash
git clone https://github.com/paiml/helixdb-from-zero
cd helixdb-from-zero
make install   # cargo install helix-cli (one-time)
make up        # helix push dev — builds + starts HelixDB on 127.0.0.1:6969
make demo      # cargo run --bin helix-demo — runs all 4 contracts live
```

`make up` is `helix push dev` under the hood: it compiles `db/queries.hx`,
builds a Docker image, and starts the container. `make demo` then exercises
the four contracts end-to-end and prints which assertion each one checked.

## Prerequisites

- Docker (Compose v2 not required — helix-cli drives Docker directly)
- Rust 1.95+ (`rust-toolchain.toml` pins automatically)
- helix-cli (`make install` installs it via `cargo install --git`)

## What's here

```
crates/helix-core/   Rust client + 4 named runtime contracts
contracts/           helix-rust-v1.yaml — formal spec for the four contracts
db/                  schema.hx + queries.hx — pushed by `helix push`
helix.toml           Helix project config (name, queries path, ports)
Makefile             entry points: install / up / demo / test / coverage / pmat / fmt / lint
assets/              hero.svg + hero.png
```

## Provable contracts

Each public async function in `crates/helix-core/src/lib.rs` asserts a
named runtime contract immediately after the HTTP round trip. Formal spec
lives in [`contracts/helix-rust-v1.yaml`](contracts/helix-rust-v1.yaml).

| Contract                          | Asserted in                            | HelixQL query                  |
|-----------------------------------|----------------------------------------|--------------------------------|
| `vertex_round_trip` (C1)          | `vertex_round_trip()`                  | `InsertDocument`, `GetDocumentByTitle` |
| `edge_traversal` (C2)             | `edge_traversal()`                     | `InsertRelated`, `Neighbours`  |
| `vector_top_k_contains_self` (C3) | `vector_top_k_contains_self()`         | `InsertVector`, `VectorSearch` |
| `upsert_idempotent` (C4)          | `upsert_idempotent()`                  | `InsertDocument` (×2), `CountByTitle` |

A breach panics with the contract name in the message; the demo binary
walks all four against the running HelixDB and aborts loudly on any
violation rather than ship corrupt state downstream.

## Distribution model

HelixDB has no published Docker Hub image and no Homebrew tap; the
canonical installer is `cargo install --git
https://github.com/HelixDB/helix-db helix-cli`, which puts the `helix`
binary on PATH. `helix push <instance>` then compiles the local `.hx`
files, builds a Docker image on demand, and starts the container. This
repo's `make up` / `make down` / `make nuke` wrap those commands directly
— there is intentionally no `compose.yml`, since `helix push` *is* the
deployment primitive.

## Commands

```bash
make install    # cargo install helix-cli (one-time, idempotent)
make up         # helix push dev (HelixDB on 127.0.0.1:6969)
make demo       # cargo run --bin helix-demo (4 contracts, live engine)
make test       # cargo test --release (3 lib unit tests)
make coverage   # cargo llvm-cov --release --workspace
make lint       # cargo clippy --all-targets -- -D warnings
make fmt        # cargo fmt --all
make pmat       # pmat quality-gate
make down       # helix stop dev (preserves the volume)
make nuke       # helix delete dev (wipes graph + vector index)
```

## Course Materials

This repo is the companion to the Coursera course **HelixDB From Zero**,
part of the [Rust for Data Engineering](https://github.com/paiml/rust-de-specialization)
specialization. Lessons walk through the four contracts as a path through
HelixDB's primitives (vertices, edges, vectors, idempotent upsert) and the
shape of building a small Rust service that talks to a graph-vector
backend.

## License

Dual-licensed under [MIT](LICENSE-MIT) **OR** [Apache-2.0](LICENSE-APACHE).
HelixDB itself is AGPL-3.0; this repo depends on the published `helix-db`
crate and HTTP API surface, so the AGPL clause does not propagate to this
companion code.
