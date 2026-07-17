.DEFAULT_GOAL := help

CONSOLE_IP ?= 192.168.1.100

.PHONY: help
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-16s\033[0m %s\n", $$1, $$2}'

## Setup

.PHONY: install
install: ## Install frontend dependencies (npm)
	npm install

## CLI

.PHONY: build
build: ## Build the CLI in release mode (target/release/yamaha-rcp-to-osc)
	cargo build --release

.PHONY: build-debug
build-debug: ## Build the CLI in debug mode
	cargo build

.PHONY: run
run: ## Run the CLI from source (CONSOLE_IP=x.x.x.x make run ARGS="--rcp-port 1234")
	cargo run -- --console-ip $(CONSOLE_IP) $(ARGS)

.PHONY: nc
nc: ## Start a fake RCP console (netcat) on port 49280 for local testing
	npm run nc

## GUI

.PHONY: dev
dev: ## Run the GUI in development mode with hot reload (Tauri + Vite)
	npm run tauri dev

.PHONY: frontend-dev
frontend-dev: ## Run only the Vite dev server, without the Tauri shell
	npm run dev

.PHONY: gui-build
gui-build: ## Build the GUI production bundle (src-tauri/target/release/bundle/)
	npm run tauri build

## Quality

.PHONY: test
test: ## Run the Rust test suite
	cargo test

.PHONY: fmt
fmt: ## Format Rust (root + src-tauri) and frontend code
	cargo fmt
	cd src-tauri && cargo fmt
	npm run format

.PHONY: fmt-check
fmt-check: ## Check formatting without making changes (matches CI)
	cargo fmt -- --check
	cd src-tauri && cargo fmt -- --check

.PHONY: clippy
clippy: ## Lint Rust code (root + src-tauri) with warnings denied
	cargo clippy -- -D warnings
	cd src-tauri && cargo clippy -- -D warnings

.PHONY: lint
lint: ## Lint the frontend (ESLint)
	npm run lint

.PHONY: check
check: fmt-check clippy test lint ## Run the full suite CI runs (fmt, clippy, test, lint)

## Release

.PHONY: bump-version
bump-version: ## Set the version across all manifests (make bump-version VERSION=x.y.z)
	@if [ -z "$(VERSION)" ]; then \
		echo "Usage: make bump-version VERSION=x.y.z"; \
		exit 1; \
	fi
	@echo "Bumping version to $(VERSION)..."
	sed -i.bak 's/^version = ".*"/version = "$(VERSION)"/' Cargo.toml && rm Cargo.toml.bak
	sed -i.bak 's/^version = ".*"/version = "$(VERSION)"/' src-tauri/Cargo.toml && rm src-tauri/Cargo.toml.bak
	sed -i.bak 's/"version": ".*"/"version": "$(VERSION)"/' package.json && rm package.json.bak
	sed -i.bak 's/"version": ".*"/"version": "$(VERSION)"/' src-tauri/tauri.conf.json && rm src-tauri/tauri.conf.json.bak
	cargo build --offline > /dev/null 2>&1 || cargo build > /dev/null
	cd src-tauri && (cargo build --offline > /dev/null 2>&1 || cargo build > /dev/null)
	@echo "Done. Review the diff, then: git commit -am 'chore: bump version to $(VERSION)' && git tag v$(VERSION) && git push origin main v$(VERSION)"

## Cleanup

.PHONY: clean
clean: ## Remove build artifacts (cargo + Tauri + Vite)
	cargo clean
	cd src-tauri && cargo clean
	rm -rf dist
