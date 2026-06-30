# yiz-tunnel

[中文版](README.zh-CN.md)

`yiz-tunnel` is a Rust HTTP tunnel / reverse proxy project. The first milestone is a small, practical subset of common nginx HTTP behavior.

The first version focuses on HTTP:

- HTTP/1.1 static file serving.
- HTTP reverse proxy.
- WebSocket proxy.
- Management API.
- JSON configuration persistence.
- Log output.
- HTTP server hot reload.
- Upstream round-robin, failover, and blue-green replacement state.

The first version does not include:

- HTTPS / HTTP/2 / HTTP/3.
- Management API authentication.
- `tcp-forward`.
- Full nginx configuration syntax compatibility.

## Documentation

- Quick start: [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md)
- Management API: [docs/MANAGEMENT_API.md](docs/MANAGEMENT_API.md)
- Design and progress: [plans/PROJECT_CONTINUATION.md](plans/PROJECT_CONTINUATION.md)

## Build

```powershell
cargo build
```

Build output:

```text
target\debug\yiz-tunnel.exe
```

## Start

Start with an explicit system configuration file:

```powershell
target\debug\yiz-tunnel.exe -c .\yiz-tunnel.json
```

When `-c` is not provided, `yiz-tunnel` uses `yiz-tunnel.json` in the current directory.

If the configuration file does not exist, the program creates it with default values:

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

## Configuration And Data

The system configuration file only stores system-level settings, such as the data directory, log directory, and management service listen address.

HTTP rules are written by the Management API:

```text
data\http-server.json
```

Log files:

```text
logs\admin.log
logs\access.log
logs\error.log
```

## Supported Features

### Static File

- Prefix / full route matching.
- `root` / `alias` style file path resolution.
- Path traversal protection.
- Basic MIME detection.
- `ETag`.
- `Last-Modified`.
- Single byte `Range`.

### Proxy

- HTTP reverse proxy.
- Proxy path rewrite, currently supporting `replacePrefix`.
- WebSocket upgrade forwarding.
- `Host`, `X-Real-IP`, `X-Forwarded-For`, and `X-Forwarded-Proto`.
- Streaming proxy responses.
- Streaming `Content-Length` request bodies.
- Decoding chunked request bodies before forwarding.
- Trying later upstream candidates after upstream connection failure.

### Upstream

- Select by ascending `priority`.
- Round-robin for upstreams with the same `priority`.
- Adding the same `group + name` triggers blue-green replacement.
- Old upstreams with active requests are retained as `deading`.
- Old upstreams become `dead` after active requests drain to zero.

### Runtime

- Start and stop HTTP servers.
- Stop closes the listener and stops accepting new connections.
- Servers with active connections enter `stopping`.
- Servers become `stopped` after active connections drain to zero.
- Management API changes are applied locally to the affected runtime configuration.

## Automated Verification

Unit tests:

```powershell
cargo test
```

Current test coverage:

- Route matching.
- Static file responses.
- Proxy forwarding.
- Basic WebSocket forwarding path.
- Proxy headers.
- Proxy path rewrite.
- Keep-alive.
- Cache / range.
- Chunked request bodies.
- `conf` validation and runtime behavior.
- Upstream round-robin, failover, and blue-green replacement state.
- Proxy request body and response streaming.
- HTTP server graceful stop.

Management API end-to-end smoke test:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-management-api.ps1
```

The smoke script starts an isolated temporary instance and verifies:

- `system/status`.
- Creating an HTTP server.
- Creating a static file route.
- Accessing the runtime listen port.
- Replacing an upstream with the same `group + name`.
- Invalid `conf` returns `400`.

## First Version Limits

- The Management API currently has no authentication. Bind it to localhost or a trusted private network.
- `serverName` is not a complete virtual host matching implementation yet.
- Chunked request bodies are not streamed yet.
- Static file serving does not implement index / try_files yet.
- TLS is not implemented yet.
- `tcp-forward` is not implemented yet.

## Development Notes

- Keep the core HTTP runtime dependency-light.
- The Management API uses `axum`.
- The async runtime uses `tokio`.
- JSON uses `serde_json`.
- TLS is planned to use `rustls`.
