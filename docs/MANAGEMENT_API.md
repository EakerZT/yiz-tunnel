# yiz-tunnel 管理 API 文档

快速启动和完整调用示例见 [GETTING_STARTED.md](GETTING_STARTED.md)。

## 基础信息

默认管理服务地址：

```text
http://127.0.0.1:9000
```

API 版本前缀：

```text
/api/v1
```

第一阶段不考虑鉴权。

## 通用响应

所有接口统一返回：

```json
{
  "code": 0,
  "message": "ok",
  "data": {}
}
```

字段说明：

- `code`：数字状态码。成功为 `0`。
- `message`：结果说明。成功为 `ok`。
- `data`：业务数据。失败时通常为 `null`。

失败示例：

```json
{
  "code": 10002,
  "message": "http-server not found: hs_xxx",
  "data": null
}
```

## System

### 查看系统状态

```http
GET /api/v1/system/status
```

响应：

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

### 列举 HTTP 服务

```http
GET /api/v1/http-servers
```

响应：

```json
{
  "code": 0,
  "message": "ok",
  "data": []
}
```

### 新增 HTTP 服务

```http
POST /api/v1/http-servers
Content-Type: application/json
```

请求体只接受：

- `alias`
- `listen`
- `conf`
- `graceful`

`conf` 第一版支持以下字段，时间单位为毫秒，大小单位为字节。字段缺失时使用默认值；未知字段、非正整数或非对象结构会返回 `400`，且不会落盘。

| 字段 | 默认值 | 说明 |
| --- | ---: | --- |
| `client_max_body_size` | `1048576` | 请求体最大大小，超过后返回 `413` |
| `client_header_timeout` | `60000` | 读取请求头超时 |
| `client_body_timeout` | `60000` | 读取请求体超时 |
| `send_timeout` | `60000` | 向客户端写响应超时 |
| `keepalive_timeout` | `75000` | keep-alive 连接等待下一请求的超时 |
| `keepalive_requests` | `1000` | 单连接最大请求数 |
| `proxy_connect_timeout` | `60000` | 连接 upstream 超时 |
| `proxy_send_timeout` | `60000` | 向 upstream 写请求超时 |
| `proxy_read_timeout` | `60000` | 读取 upstream 响应超时 |

请求读取阶段使用 `http-server.conf`；route/upstream 处理阶段支持 `route.conf` 和 `upstream.conf` 覆盖同名字段。

请求示例：

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

响应示例：

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

### 查看 HTTP 服务配置

```http
GET /api/v1/http-server/{id}
```

返回该 HTTP 服务的配置，不返回运行时状态。

### 编辑 HTTP 服务

```http
PUT /api/v1/http-server/{id}
Content-Type: application/json
```

请求体只接受：

- `alias`
- `listen`
- `conf`
- `graceful`

请求示例：

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

### 启停 HTTP 服务

```http
PUT /api/v1/http-server/{id}/enabled
Content-Type: application/json
```

请求体：

```json
{
  "enabled": false
}
```

说明：

- `enabled = true` 表示期望启用。
- `enabled = false` 表示期望停用。
- 删除 HTTP 服务前需要先设置 `enabled = false`。

### 删除 HTTP 服务

```http
DELETE /api/v1/http-server/{id}
```

约束：

- 只有配置中的 `enabled = false` 时才允许删除。
- 删除后新请求不再进入该服务。
- 已有请求或连接由运行时自然结束。

### 查看 HTTP 服务状态

```http
GET /api/v1/http-server/{id}/info
```

当前实现返回基础状态：

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

### 重新应用 HTTP 服务配置

```http
POST /api/v1/http-server/{id}/reload
```

说明：

- 只重试当前已落盘配置。
- 不修改配置文件。
- 当前实现会重新应用实际 HTTP runtime。

响应：

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

### 列举 upstream

```http
GET /api/v1/http-server/{id}/upstreams
```

### 新增 upstream

```http
POST /api/v1/http-server/{id}/upstreams
Content-Type: application/json
```

必填字段：

- `name`
- `group`
- `host`

其它字段不传则使用默认值。

请求示例：

```json
{
  "name": "v1",
  "group": "api",
  "host": "http://127.0.0.1:3000",
  "priority": 0,
  "conf": {}
}
```

响应示例：

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

说明：

- upstream 没有 `enabled` 字段。
- 用户新增的 upstream 都表示期望启用。
- 蓝绿发布通过新增同 `group` + 同 `name` upstream 触发。
- 新增同 `group` + 同 `name` upstream 时，配置中只保留新 upstream；旧 upstream 如果仍有活动请求，会进入 `deading/dead` 运行时状态。
- 列表和查看 upstream 会返回运行时字段：`status` 和 `activeRequestCount`。
- 当前已配置 upstream 的 `status` 为 `running`。
- 删除或被替换的旧 upstream 如果仍有活动请求，会继续出现在列表和查看接口中，`status` 为 `deading`。
- 已删除旧 upstream 的活动请求归零后，`status` 会变为 `dead`。

### 查看 upstream

```http
GET /api/v1/http-server/{id}/upstream/{upstreamId}
```

### 删除 upstream

```http
DELETE /api/v1/http-server/{id}/upstream/{upstreamId}
```

说明：

- 删除 upstream 无额外状态要求。
- 删除后新请求不再选择该 upstream。
- 已经转发到该 upstream 的旧请求或旧连接继续自然结束。
- 如果删除时该 upstream 仍有活动请求，可以继续通过 upstream 列表或查看接口观察 `deading/dead` 状态。

## Route

### 列举 route

```http
GET /api/v1/http-server/{id}/routes
```

### 新增 route

```http
POST /api/v1/http-server/{id}/routes
Content-Type: application/json
```

必填字段：

- `match.type`
- `match.path`
- `action.type`

当 `action.type = proxy` 时，必填：

- `action.proxy.upstream`

当 `action.type = file` 时，必填：

- `action.file.dir`

#### 新增 proxy route

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

`action.proxy.rewrite` 为可选字段。当前仅支持：

```json
{
  "type": "replacePrefix",
  "from": "/123456789012345/",
  "to": "/"
}
```

含义是转发前把 path 的指定前缀替换掉，query string 保持不变。例如：

```text
/123456789012345/a.png?x=1 -> /a.png?x=1
```

说明：

- rewrite 只影响发送给 upstream 的请求 path，不影响 route 匹配。
- `from` 和 `to` 必须以 `/` 开头。
- 未来可扩展类型可以考虑 `replaceFull`、`stripPrefix`、`addPrefix`、`regexReplace`，但当前传入这些类型会返回 `400`。

#### 新增 file route

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

说明：

- `match.type = 0` 表示 full 精确匹配。
- `match.type = 1` 表示 prefix 前缀匹配。
- 第一阶段不支持 `match.type = 2` regex。
- 同一个 HTTP 服务下禁止创建相同 `match.type + match.path` 的 route。
- route 顺序由系统维护。

### 查看 route

```http
GET /api/v1/http-server/{id}/route/{routeId}
```

### 删除 route

```http
DELETE /api/v1/http-server/{id}/route/{routeId}
```

说明：

- 删除 route 立即对新请求生效。
- 已经进入处理流程的旧请求继续处理。
- 不支持更新 route。

## 错误码

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

## curl 示例

### 查看系统状态

```bash
curl http://127.0.0.1:9000/api/v1/system/status
```

### 新增 HTTP 服务

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

### 新增 upstream

```bash
curl -X POST http://127.0.0.1:9000/api/v1/http-server/{id}/upstreams \
  -H "Content-Type: application/json" \
  -d '{
    "name": "v1",
    "group": "api",
    "host": "http://127.0.0.1:3000"
  }'
```

### 新增 proxy route

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
