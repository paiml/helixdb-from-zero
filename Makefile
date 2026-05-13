.PHONY: help up down nuke wait demo test fmt lint coverage pmat clean

help:
	@echo "HelixDB From Zero — companion repo"
	@echo ""
	@echo "  make up        — docker compose up -d (HelixDB on 127.0.0.1:6969)"
	@echo "  make down      — docker compose down (keeps the named volume)"
	@echo "  make nuke      — docker compose down -v (wipes the graph + vector index)"
	@echo "  make wait      — wait for the healthcheck to flip green"
	@echo "  make demo      — cargo run --bin helix-demo (4 contracts, live HelixDB)"
	@echo "  make test      — cargo test --release (lib unit + integration)"
	@echo "  make coverage  — cargo llvm-cov --release --workspace"
	@echo "  make pmat      — pmat quality-gate (entropy excluded — small-repo artifact)"
	@echo "  make fmt lint  — cargo fmt && cargo clippy"
	@echo "  make clean     — cargo clean"

up:
	@docker compose up -d
	@$(MAKE) wait

wait:
	@printf "[wait] helix healthcheck "
	@for i in $$(seq 1 30); do \
		state=$$(docker inspect -f '{{.State.Health.Status}}' helixdb-from-zero 2>/dev/null || echo missing); \
		if [ "$$state" = "healthy" ]; then echo "✓ healthy"; exit 0; fi; \
		printf "."; sleep 1; \
	done; \
	echo " ✗ timed out"; exit 1

down:
	@docker compose down

nuke:
	@docker compose down -v

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
