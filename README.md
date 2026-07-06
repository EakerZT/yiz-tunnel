# yiz-tunnel

[中文版](README.zh-CN.md)

`yiz-tunnel` is a Rust-based HTTP tunnel and reverse proxy. It is designed to provide a lightweight, API-managed gateway for local services, static files, upstream routing, and runtime configuration changes.

The project uses JSON configuration and a management API instead of an nginx-style text configuration language. The system configuration file defines where data and logs live, while HTTP server rules are created and updated through the management API.

## What It Does

`yiz-tunnel` can run one or more HTTP servers, each with its own listen address, upstream groups, routes, and runtime state.

Typical use cases include:

- Serving static files from local directories.
- Forwarding HTTP requests to local or remote upstream services.
- Forwarding WebSocket upgrade connections.
- Managing HTTP server, route, and upstream configuration through an API.
- Replacing upstream targets while allowing existing requests to drain.
- Keeping runtime logs for admin actions, access records, and errors.

## Implemented

### Configuration

- `-c <path>` support for specifying the system configuration file.
- Default `yiz-tunnel.json` generation when the system configuration file does not exist.
- System configuration for data directory, log directory, and management API listen address.
- Separate persisted HTTP rule file under the configured data directory.
- Transaction-style writes for persisted HTTP rules.
- Startup validation for persisted configuration.

### Management API

- Versioned API prefix: `/api/v1`.
- Unified response shape: `{ "code": number, "message": string, "data": ... }`.
- System status endpoint.
- HTTP server create, list, read, update, enable/disable, delete, reload, and runtime info endpoints.
- Upstream create, list, read, and delete endpoints.
- Route create, list, read, and delete endpoints.
- Validation for supported `conf` fields and route action structures.

### HTTP Runtime

- HTTP/1.1 request parsing for the current supported feature set.
- Self-implemented HTTP/2 cleartext prior-knowledge support for inbound client connections.
- Minimal HTTP/2 frame handling for SETTINGS, HEADERS, DATA, PING, CONTINUATION, and GOAWAY.
- Basic HPACK support, including static table lookup, dynamic table indexing, and Huffman decoding.
- Multiple HTTP server runtimes.
- Multiple HTTP servers can share the same listen address and are selected by fixed `serverName` / `Host` matching, with the first server as the default.
- Runtime apply/reload for a single HTTP server.
- Server states: `starting`, `running`, `stopping`, `stopped`, `failed`.
- Keep-alive handling.
- Configurable request size and timeout fields inspired by common nginx options.

### Static File

- Full and prefix route matching.
- `root` and `alias` style file path behavior.
- Path traversal protection.
- Basic MIME detection.
- `ETag`.
- `Last-Modified`.
- `If-None-Match`.
- `If-Modified-Since`.
- Single byte `Range` requests.

### Reverse Proxy

- HTTP reverse proxy to `http://` upstreams.
- HTTP/2 inbound requests can be proxied to HTTP/1.1 upstreams.
- WebSocket upgrade forwarding.
- Proxy headers: `Host`, `X-Real-IP`, `X-Forwarded-For`, `X-Forwarded-Proto`.
- Streaming upstream responses.
- Streaming `Content-Length` request bodies.
- Decoding chunked request bodies before forwarding.
- Upstream connect timeout, send timeout, and read timeout.
- Fallback to later upstream candidates when connecting to an upstream fails.
- Path rewrite with `replacePrefix`.

### Upstream Routing

- Upstream groups selected by route configuration.
- Priority-based upstream selection.
- Round-robin selection for upstreams with the same priority.
- Blue-green style replacement by adding the same `group + name`.
- Old upstream state tracking as `deading` while active requests are still using it.
- Old upstream state changes to `dead` after active requests drain.

### Logs

- Admin log.
- Access log.
- Error log.
- JSON Lines log format.

### Tests And Scripts

- Unit tests for route matching, static files, proxy behavior, keep-alive, range/cache behavior, chunked bodies, upstream replacement, graceful stop, and config validation.
- End-to-end management API smoke test script.

## Not Implemented

- TLS / HTTPS.
- HTTP/2 over TLS with ALPN.
- HTTP/1.1 `Upgrade: h2c`.
- Complete HTTP/2 stream state machine, flow control, priority, and reset handling.
- HTTP/3.
- TCP forwarding.
- Management API authentication and authorization.
- Complete nginx configuration syntax compatibility.
- Regex routes.
- Advanced rewrite modes beyond `replacePrefix`.
- Streaming chunked request bodies directly to upstreams.
- Static file `index` and `try_files` behavior.
- Wildcard and regex `serverName` matching.
- Load balancing algorithms beyond priority and round-robin.
- Health checks for upstreams.
- Request/response compression.
- Rate limiting.
- Metrics endpoint.

## Build

```powershell
cargo build
```

Build output:

```text
target\debug\yiz-tunnel.exe
```

## Run

Start with an explicit system configuration file:

```powershell
target\debug\yiz-tunnel.exe -c .\yiz-tunnel.json
```

When `-c` is not provided, `yiz-tunnel` uses `yiz-tunnel.json` in the current directory.

If the configuration file does not exist, it is created with default values:

```json
{
  "version": 1,
  "data-dir": "./data",
  "log-dir": "./logs",
  "admin": {
    "host": "127.0.0.1",
    "port": 9000
  },
  "runtime": {}
}
```

Default Management API:

```text
http://127.0.0.1:9000/api/v1
```

Check system status:

```powershell
curl.exe http://127.0.0.1:9000/api/v1/system/status
```

## Documentation

- Quick start: [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md)
- Management API: [English](docs/MANAGEMENT_API.en.md) / [中文](docs/MANAGEMENT_API.md)
- nginx capability comparison: [中文](docs/NGINX_COMPARISON.md)
- Design and progress notes: [plans/PROJECT_CONTINUATION.md](plans/PROJECT_CONTINUATION.md)

## Verification

Run unit tests:

```powershell
cargo test
```

Run the management API smoke test:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-management-api.ps1
```

## GitHub Actions

The repository includes:

- `.github/workflows/build.yml`: runs formatting checks, tests, and release-mode builds on push and pull request.
- `.github/workflows/release.yml`: builds platform artifacts and creates a GitHub Release when a tag such as `v0.1.0` is pushed.

To create a release:

```powershell
git tag v0.1.0
git push origin v0.1.0
```
