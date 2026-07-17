# AGENTS.md

## Project Overview

Yamaha RCP to OSC Bridge — translates Yamaha's proprietary RCP (Remote Control Protocol, TCP) into OSC (Open Sound Control, UDP) messages so Yamaha mixing consoles can be controlled from OSC-compatible software.

Two front-ends share one Rust core:

- **CLI** — `src/main.rs` (clap argument parsing), thin wrapper around the library.
- **GUI** — Tauri v2 + React 19 + TailwindCSS v4 desktop app. Frontend lives in `src/*.tsx`, backend in `src-tauri/`.
- **Core library** — `src/lib.rs` (`yamaha_rcp_to_osc`): `BridgeConfig`, `run_bridge`, `run_bridge_with_logger`, and the RCP↔OSC conversion logic. All bridge behavior belongs here, not in the CLI or Tauri layers.

Key crates: `tokio` (async runtime), `rosc` (OSC), `clap`, `serde`, `socket2` (SO_REUSEADDR/SO_REUSEPORT socket setup).

Note the unusual layout: `src/` contains **both** the Rust crate (`lib.rs`, `main.rs`) and the React frontend (`App.tsx`, `main.tsx`, `index.css`). The Tauri backend is a separate crate in `src-tauri/` with its own `Cargo.toml` and lockfile.

## Setup Commands

- Rust toolchain (stable) required for CLI/library.
- Node.js 18+ and npm required for the GUI.
- Install frontend dependencies: `npm install`
- Build CLI: `cargo build`
- Run CLI: `cargo run -- --console-ip <ip>` (see `--help` for port/address flags; defaults: RCP 49280, OSC out 127.0.0.1:3999, OSC in 0.0.0.0:4000)

## Development Workflow

- GUI dev (Vite + Tauri, hot reload): `npm run tauri dev`
- Frontend only: `npm run dev`
- GUI production build: `npm run tauri build` (output in `src-tauri/target/release/bundle/`)
- Fake console for local testing: `npm run nc` (listens on TCP 49280)

## Testing Instructions

- Run all Rust tests: `cargo test`
- Integration tests live in `tests/` (e.g. `tests/conversion_tests.rs` covers RCP↔OSC conversion).
- CI (`.github/workflows/tests.yml`) has two jobs: `test` runs on Linux/macOS/Windows (`cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt -- --check` for the root crate), and `gui` runs on Linux (`npm run lint`, `npm run build`, plus `cargo build`/`clippy`/`fmt --check` for `src-tauri`). All must pass.
- Add or update tests in `tests/` when changing conversion or bridge logic.

## Code Style

- **Rust**: rustfmt-formatted (`cargo fmt`), clippy-clean with warnings denied (`cargo clippy -- -D warnings`). Edition 2024.
- **TypeScript/React**: ESLint flat config (`eslint.config.mjs`) with Prettier integration.
  - Lint: `npm run lint`
  - Auto-fix: `npm run lint:fix`
  - Format: `npm run format`
- Keep the core bridge logic UI-agnostic in `src/lib.rs`; the Tauri backend (`src-tauri/src/main.rs`) and CLI should stay thin wrappers.
- Use `run_bridge_with_logger` when output needs to go somewhere other than stdout (the GUI does this).

## Build and Release

- CLI release build: `cargo build --release`
- Releases are cut by pushing a `v*` tag; `.github/workflows/release.yml` runs the test suite then builds macOS and Windows CLI binaries plus a macOS/Windows GUI bundle (Tauri), attaching all of them to the GitHub release. Both the macOS CLI binary and the macOS GUI bundle are signed with the Developer ID Application cert and notarized via GitHub Actions secrets (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_CONTENT`) — see the release workflow for how they're used. The macOS CLI ships as a `.zip` (was `.tar.gz`) since Apple's notary service only accepts zip/dmg/pkg, and the notarization ticket can't be stapled to a raw binary, so it needs network access on first launch to clear Gatekeeper.
- Bump all four version fields (`Cargo.toml`, `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`) together with `make bump-version VERSION=x.y.z` before tagging a release.
- The tests workflow triggers on changes to `src/**`, `tests/**`, `Cargo.toml`, `Cargo.lock`, `src-tauri/**`, `package.json`, `package-lock.json`, `tsconfig*.json`, `vite.config.ts`, or `eslint.config.mjs`.

## Security / Repo Hygiene

- Do not commit secrets. `.env`, Apple signing material (`*.p8`, `*.cer`, `*.certSigningRequest`, `*.p12`) may exist locally in the working tree — never stage or commit these files.

## Pull Request Guidelines

- Before committing: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `npm run lint` if frontend files changed.
- Keep commits scoped; conventional-commit-style prefixes (`feat:`, `ci:`, `fix:`) are used in history.

## Gotchas

- The OSC input socket sets SO_REUSEADDR (and SO_REUSEPORT on Unix) so the bridge can restart quickly; be careful when touching the socket setup in `src/lib.rs` — it uses raw `libc` calls on Unix.
- Yamaha RCP is not officially documented for real-time use; the bridge works around `sscurrent_ex` notifications lacking detail by issuing a follow-up `ssinfo_ex` query. See README references for protocol docs.
- `src-tauri/` has its own `Cargo.lock`; the root crate and the Tauri crate build independently.
