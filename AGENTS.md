# Repository Guidelines

## Project Overview

`yiz-tunnel` is a Rust HTTP tunnel and reverse proxy with an Axum-based management API. The binary reads or creates a system JSON config, persists HTTP server rules under the configured data directory, starts enabled HTTP server runtimes, and serves admin endpoints under `/api/v1`.

## Important Files

- `src/main.rs`: process startup, config loading, runtime creation, admin listener.
- `src/admin.rs`: Axum routes and management API handlers.
- `src/config.rs`: system config path parsing and default config generation.
- `src/storage.rs`: persisted HTTP server config storage, validation, and mutation helpers.
- `src/runtime.rs`: custom HTTP/1.1 and h2c runtime, static files, proxying, upstream selection, graceful state tracking, and most unit tests.
- `src/model.rs`: request/response/config data structures.
- `src/logger.rs`: JSON Lines admin/access/error logging.
- `docs/`: user-facing API and getting-started documentation.
- `plans/`: design notes and project continuation context.
- `scripts/smoke-management-api.ps1`: end-to-end management API smoke test.

## Build And Test

Use PowerShell from the repository root.

```powershell
cargo test
cargo build
cargo fmt --check
```

Run the management API smoke test after building when changes affect startup, persistence, or admin endpoints:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-management-api.ps1
```

## Development Notes

- Keep changes scoped. The runtime is intentionally custom and has many protocol details in `src/runtime.rs`; prefer adding focused helpers/tests over broad rewrites.
- Preserve the existing JSON field naming and response shape: management responses use `{ "code": number, "message": string, "data": ... }`.
- Validate persisted config changes through `src/storage.rs`; avoid bypassing storage helpers from API handlers.
- When adding config fields, update the model, storage validation, docs, and tests together.
- Runtime state is shared through `Arc`, `Mutex`, atomics, and `RwLock`; be careful not to hold blocking locks across `.await`.
- The project currently has no TLS, auth, regex routes, health checks, metrics, or HTTP/3 support. Do not imply those features exist in docs or tests unless implementing them.
- Do not commit generated runtime data or logs. The default local config creates `data/` and `logs/` when the binary is run.

## Verification Baseline

At initialization, `cargo test` passed with 33 tests.
