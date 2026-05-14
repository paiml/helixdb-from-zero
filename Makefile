.PHONY: help install up down nuke demo test fmt lint coverage pmat clean

HELIX ?= helix
INSTANCE ?= dev

help:
	@echo "HelixDB From Zero — companion repo"
	@echo ""
	@echo "  make install   — cargo install --git https://github.com/HelixDB/helix-db helix-cli"
	@echo "  make up        — helix push $(INSTANCE) (builds + starts the local image)"
	@echo "  make down      — helix stop $(INSTANCE) (keeps the data volume)"
	@echo "  make nuke      — helix delete $(INSTANCE) (wipes the graph + vector index)"
	@echo "  make demo      — cargo run --release --bin helix-demo (4 contracts, live HelixDB)"
	@echo "  make test      — cargo test --release (3 lib unit tests)"
	@echo "  make coverage  — cargo llvm-cov --release --workspace"
	@echo "  make pmat      — pmat quality-gate (entropy excluded — small-repo artifact)"
	@echo "  make fmt lint  — cargo fmt && cargo clippy"
	@echo "  make clean     — cargo clean"

install:
	@command -v $(HELIX) >/dev/null 2>&1 \
		&& echo "[install] helix-cli already on PATH ($$($(HELIX) --version))" \
		|| cargo install --git https://github.com/HelixDB/helix-db helix-cli

up:
	@$(HELIX) push $(INSTANCE)

down:
	@$(HELIX) stop $(INSTANCE)

nuke:
	@# helix-cli sometimes exits non-zero on a cosmetic volume-cleanup step
	@# (the Docker container writes its data dir as root; helix-cli running as
	@# the host user cannot rm it). The container + image are still removed,
	@# and the next `make up` recreates the volume cleanly. We pin success to
	@# "no helix container left" rather than helix-cli's own exit code.
	@yes y | $(HELIX) delete $(INSTANCE) || true
	@test -z "$$(docker ps -aq --filter name=helix-$$(basename $$(pwd))-$(INSTANCE))" \
		|| { echo "[nuke] container still present — failing" >&2; exit 1; }
	@echo "[nuke] instance $(INSTANCE) deleted"

demo:
	@cargo run --release --bin helix-demo

test:
	@cargo test --release

coverage:
	@cargo llvm-cov --release --workspace --show-missing-lines

pmat:
	@pmat quality-gate --checks dead-code,complexity,coverage,sections,satd,security,duplicates,provability

fmt:
	@cargo fmt --all

lint:
	@cargo clippy --all-targets -- -D warnings

clean:
	@cargo clean
