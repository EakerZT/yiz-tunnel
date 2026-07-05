# yiz-tunnel Management API

[中文版](MANAGEMENT_API.md)

For quick start and full command examples, see [GETTING_STARTED.md](GETTING_STARTED.md).

## Basics

Default management service address:

```text
http://127.0.0.1:9000
```

API version prefix:

```text
/api/v1
```

Authentication is not implemented yet. Bind the management service to localhost or a trusted private network.

## Response Shape

All endpoints return the same response envelope:

```json
{
  "code": 0,
  "message": "ok",
  "data": {}
}
```

Fields:

- `code`: numeric result code. Success is `0`.
- `message`: result message. Success is `ok`.
- `data`: business payload. Failures usually return `null`.

Failure example:

```json
{
  "code": 10002,
  "message": "http-server not found: hs_xxx",
  "data": null
}
```

## System

### Get System Status

```http
GET /api/v1/system/status
```

Response:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "version": "0.1.0",
    "uptime": 10,
    "systemConfigPath": "target/dev-run/yiz-tunnel.json",
    "dataDir": "target/dev-run/data",
    "logDir": "target/dev-run/logs",
    "httpServerCount": 0,
    "tcpForwardCount": 0,
    "activeConnectionCount": 0,
    "lastError": null
  }
}
```

## HTTP Server

### List HTTP Servers

```http
GET /api/v1/http-servers
```

Response:

```json
{
  "code": 0,
  "message": "ok",
  "data": []
}
```

### Create HTTP Server

```http
POST /api/v1/http-servers
Content-Type: application/json
```

Accepted request fields:

- `alias`
- `listen`
- `conf`
- `graceful`

`conf` supports the following fields. Time values are in milliseconds. Size values are in bytes. Missing fields use defaults. Unknown fields, non-positive integers, or non-object `conf` values return `400` and are not persisted.

| Field | Default | Description |
| --- | ---: | --- |
| `client_max_body_size` | `1048576` | Maximum request body size. Exceeding it returns `413`. |
| `client_header_timeout` | `60000` | Timeout for reading request headers. |
| `client_body_timeout` | `60000` | Timeout for reading request bodies. |
| `send_timeout` | `60000` | Timeout for writing responses to clients. |
| `keepalive_timeout` | `75000` | Timeout for waiting for the next keep-alive request. |
| `keepalive_requests` | `1000` | Maximum requests per connection. |
| `proxy_connect_timeout` | `60000` | Timeout for connecting to upstream. |
| `proxy_send_timeout` | `60000` | Timeout for writing requests to upstream. |
| `proxy_read_timeout` | `60000` | Timeout for reading upstream responses. |

Request reading uses `http-server.conf`. Route and upstream processing can override the same fields through `route.conf` and `upstream.conf`.

Request example:

```json
{
  "alias": "local-api",
  "listen": {
    "host": "127.0.0.1",
    "port": 8080,
    "serverName": [
      "localhost"
    ]
  },
  "conf": {},
  "graceful": {
    "enabled": true,
    "type": 0
  }
}
```

Response example:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": "hs_019f131d9fd375a09417c2792719cae8",
    "alias": "local-api",
    "enabled": true,
    "listen": {
      "host": "127.0.0.1",
      "port": 8080,
      "serverName": [
        "localhost"
      ]
    },
    "graceful": {
      "enabled": true,
      "type": 0
    },
    "conf": {},
    "upstreams": [],
    "routes": []
  }
}
```

### Get HTTP Server Configuration

```http
GET /api/v1/http-server/{id}
```

Returns the persisted HTTP server configuration, not runtime state.

### Update HTTP Server

```http
PUT /api/v1/http-server/{id}
Content-Type: application/json
```

Accepted request fields:

- `alias`
- `listen`
- `conf`
- `graceful`

Request example:

```json
{
  "alias": "local-api-new",
  "listen": {
    "host": "127.0.0.1",
    "port": 8081,
    "serverName": [
      "localhost"
    ]
  },
  "conf": {},
  "graceful": {
    "enabled": true,
    "type": 0
  }
}
```

### Enable Or Disable HTTP Server

```http
PUT /api/v1/http-server/{id}/enabled
Content-Type: application/json
```

Request body:

```json
{
  "enabled": false
}
```

Notes:

- `enabled = true` means the expected state is enabled.
- `enabled = false` means the expected state is disabled.
- An HTTP server must be disabled before it can be deleted.

### Delete HTTP Server

```http
DELETE /api/v1/http-server/{id}
```

Constraints:

- Deletion is allowed only when persisted `enabled = false`.
- New requests will no longer enter the deleted server.
- Existing requests or connections finish naturally in runtime.

### Get HTTP Server Runtime Info

```http
GET /api/v1/http-server/{id}/info
```

Current response:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": "hs_xxx",
    "alias": "local-api",
    "enabled": true,
    "status": "running",
    "activeConnectionCount": 0,
    "activeRequestCount": 0,
    "lastError": null
  }
}
```

### Reload HTTP Server Configuration

```http
POST /api/v1/http-server/{id}/reload
```

Notes:

- Retries the currently persisted configuration.
- Does not modify configuration files.
- Re-applies the actual HTTP runtime.

Response:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": "hs_xxx",
    "reloaded": true
  }
}
```

## Upstream

### List Upstreams

```http
GET /api/v1/http-server/{id}/upstreams
```

### Create Upstream

```http
POST /api/v1/http-server/{id}/upstreams
Content-Type: application/json
```

Required fields:

- `name`
- `group`
- `host`

Other fields use defaults when omitted.

Request example:

```json
{
  "name": "v1",
  "group": "api",
  "host": "http://127.0.0.1:3000",
  "priority": 0,
  "conf": {}
}
```

Response example:

```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "id": "up_xxx",
    "group": "api",
    "name": "v1",
    "host": "http://127.0.0.1:3000",
    "priority": 0,
    "conf": {},
    "status": "running",
    "activeRequestCount": 0
  }
}
```

Notes:

- Upstreams do not have an `enabled` field.
- Created upstreams always represent expected enabled configuration.
- Blue-green replacement is triggered by creating another upstream with the same `group + name`.
- When creating the same `group + name`, only the new upstream remains in persisted configuration. The old upstream can remain visible as runtime state `deading/dead` if it still has active requests.
- List and get endpoints include runtime fields: `status` and `activeRequestCount`.
- Configured upstreams have `status = running`.
- Deleted or replaced upstreams with active requests continue to appear as `deading`.
- Once active requests drain to zero, deleted old upstreams become `dead`.

### Get Upstream

```http
GET /api/v1/http-server/{id}/upstream/{upstreamId}
```

### Delete Upstream

```http
DELETE /api/v1/http-server/{id}/upstream/{upstreamId}
```

Notes:

- Deleting an upstream has no extra state requirement.
- New requests no longer select the deleted upstream.
- Existing requests or connections already forwarded to the upstream finish naturally.
- If the upstream still has active requests, it can still be observed through upstream list/get endpoints as `deading/dead`.

## Route

### List Routes

```http
GET /api/v1/http-server/{id}/routes
```

### Create Route

```http
POST /api/v1/http-server/{id}/routes
Content-Type: application/json
```

Required fields:

- `match.type`
- `match.path`
- `action.type`

When `action.type = proxy`, required:

- `action.proxy.upstream`

When `action.type = file`, required:

- `action.file.dir`

#### Create Proxy Route

```json
{
  "match": {
    "type": 1,
    "path": "/api/"
  },
  "action": {
    "type": "proxy",
    "proxy": {
      "upstream": "api",
      "websocket": {
        "enabled": true
      },
      "rewrite": {
        "type": "replacePrefix",
        "from": "/api/",
        "to": "/"
      }
    }
  },
  "conf": {}
}
```

`action.proxy.rewrite` is optional. Currently supported:

```json
{
  "type": "replacePrefix",
  "from": "/123456789012345/",
  "to": "/"
}
```

It replaces the configured path prefix before forwarding to upstream. Query string is preserved:

```text
/123456789012345/a.png?x=1 -> /a.png?x=1
```

Notes:

- Rewrite only affects the request path sent to upstream. It does not affect route matching.
- `from` and `to` must start with `/`.
- Future types may include `replaceFull`, `stripPrefix`, `addPrefix`, and `regexReplace`, but currently these values return `400`.

#### Create File Route

```json
{
  "match": {
    "type": 1,
    "path": "/static/"
  },
  "action": {
    "type": "file",
    "file": {
      "dir": "./public",
      "alias": 0
    }
  },
  "conf": {}
}
```

Notes:

- `match.type = 0` means full exact match.
- `match.type = 1` means prefix match.
- `match.type = 2` regex is not implemented yet.
- The same HTTP server cannot have duplicate `match.type + match.path` routes.
- Route order is maintained by the system.

### Get Route

```http
GET /api/v1/http-server/{id}/route/{routeId}
```

### Delete Route

```http
DELETE /api/v1/http-server/{id}/route/{routeId}
```

Notes:

- Deleting a route takes effect immediately for new requests.
- Requests already in progress continue processing.
- Updating routes is not supported.

## Error Codes

```text
0     OK

10000 UNKNOWN_ERROR
10001 INVALID_REQUEST
10002 NOT_FOUND
10003 CONFLICT
10004 INTERNAL_ERROR

20000 CONFIG_PARSE_FAILED
20001 CONFIG_VALIDATE_FAILED
20002 CONFIG_READ_FAILED
20003 CONFIG_WRITE_FAILED
20004 CONFIG_VERSION_UNSUPPORTED
20005 CONFIG_TRANSACTION_FAILED

30000 HTTP_SERVER_NOT_FOUND
30001 HTTP_SERVER_ALREADY_EXISTS
30002 HTTP_SERVER_NOT_DISABLED
30003 HTTP_SERVER_RELOAD_FAILED
30004 HTTP_SERVER_STATUS_INVALID
30005 HTTP_SERVER_DELETE_PENDING

31000 LISTEN_BIND_FAILED
31001 PORT_IN_USE
31002 LISTEN_HOST_INVALID

40000 UPSTREAM_NOT_FOUND
40001 UPSTREAM_HOST_INVALID
40002 UPSTREAM_GROUP_NOT_FOUND
40003 UPSTREAM_BLUE_GREEN_CONFLICT
40004 UPSTREAM_NO_AVAILABLE_TARGET

41000 ROUTE_NOT_FOUND
41001 ROUTE_MATCH_INVALID
41002 ROUTE_ACTION_INVALID
41003 ROUTE_UPSTREAM_NOT_FOUND
41004 ROUTE_FILE_DIR_INVALID

50000 TCP_FORWARD_NOT_FOUND
50001 TCP_FORWARD_RELOAD_FAILED
50002 TCP_FORWARD_BIND_FAILED
```

## curl Examples

### Get System Status

```bash
curl http://127.0.0.1:9000/api/v1/system/status
```

### Create HTTP Server

```bash
curl -X POST http://127.0.0.1:9000/api/v1/http-servers \
  -H "Content-Type: application/json" \
  -d '{
    "alias": "local-api",
    "listen": {
      "host": "127.0.0.1",
      "port": 8080,
      "serverName": ["localhost"]
    },
    "conf": {},
    "graceful": {
      "enabled": true,
      "type": 0
    }
  }'
```

### Create Upstream

```bash
curl -X POST http://127.0.0.1:9000/api/v1/http-server/{id}/upstreams \
  -H "Content-Type: application/json" \
  -d '{
    "name": "v1",
    "group": "api",
    "host": "http://127.0.0.1:3000"
  }'
```

### Create Proxy Route

```bash
curl -X POST http://127.0.0.1:9000/api/v1/http-server/{id}/routes \
  -H "Content-Type: application/json" \
  -d '{
    "match": {
      "type": 1,
      "path": "/api/"
    },
    "action": {
      "type": "proxy",
      "proxy": {
        "upstream": "api",
        "websocket": {
          "enabled": true
        }
      }
    },
    "conf": {}
  }'
```
