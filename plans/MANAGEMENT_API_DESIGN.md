# 管理 API 设计草案

## 定位

管理 API 是 `yiz-tunnel` 第一阶段的主要配置入口。

第一阶段运行中不考虑手工修改配置文件，`http-server` 和 `tcp-forward` 规则变更主要通过管理 API 完成。

管理 API 修改配置时遵循配置存储设计中的规则：

```text
先落盘，再调整运行时
```

结构错误直接拒绝，不落盘；运行环境错误允许保留用户期望配置，并通过运行状态暴露失败原因。

## API 版本

API 路径带版本号。

第一阶段使用：

```text
/api/v1
```

## 通用响应结构

所有接口统一返回：

```json
{
  "code": 0,
  "message": "ok",
  "data": {}
}
```

字段说明：

- `code`：数字状态码。
- `message`：结果描述。
- `data`：响应数据。

成功时：

- `code = 0`
- `message = "ok"`

失败时：

- `code` 为具体错误码。
- `message` 返回错误原因。
- `data` 根据错误类型返回补充信息，可为空。

示例：

```json
{
  "code": 10001,
  "message": "port 7000 is already in use",
  "data": {
    "host": "0.0.0.0",
    "port": 7000
  }
}
```

## HTTP Server API

### 列举所有服务

```text
GET /api/v1/http-servers
```

返回所有 `http-server` 配置摘要。

### 新增服务

```text
POST /api/v1/http-servers
```

新增一个 `http-server`。

ID 由软件生成，用户不传入。

请求体第一阶段只接受：

- `alias`
- `listen`
- `conf`
- `graceful`

处理流程：

1. 校验请求结构。
2. 生成 `http-server` ID 以及内部子对象 ID。
3. 写入 `http-server.json`。
4. 对该服务执行局部 reload。
5. 返回配置和运行状态。

### 更新服务

```text
PUT /api/v1/http-server/{id}
```

第一阶段该接口只接受：

- `alias`
- `listen`
- `conf`
- `graceful`

不通过该接口批量修改 upstream 和 route，upstream、route 通过子资源接口处理。

### 启停服务

```text
PUT /api/v1/http-server/{id}/enabled
```

用于启用或禁用一个 `http-server`。

请求体示例：

```json
{
  "enabled": true
}
```

处理规则：

- 修改配置中的 `enabled`。
- 事务化写入 `http-server.json`。
- 对该服务执行局部 reload。
- 如果运行时启用失败，配置仍保留 `enabled = true`，状态中返回失败原因。

### 删除服务

```text
DELETE /api/v1/http-server/{id}
```

删除一个 `http-server`。

约束：

- 配置中 `enabled = false` 时，可以期望删除。
- 删除后新请求不再进入该 `http-server`。
- 运行时内部可维护 `delete` 状态，等待已有连接或请求自然结束后再彻底清理。
- 不强制中断已有连接或请求。

### 查看服务配置

```text
GET /api/v1/http-server/{id}
```

返回该 `http-server` 的配置内容。

只返回配置，不返回运行时状态。

### 查看服务状态

```text
GET /api/v1/http-server/{id}/info
```

返回该 `http-server` 的运行时状态。

状态信息可包含：

- 运行状态。
- 监听状态。
- 最近错误。
- 活动请求数。
- 活动连接数。
- upstream 运行时状态。
- route 运行时统计。

## Upstream API

第一阶段 upstream 只支持新增和删除，不支持更新。

路径形式待进一步确认。

建议按 `http-server` 子资源设计：

```text
POST   /api/v1/http-server/{id}/upstreams
DELETE /api/v1/http-server/{id}/upstream/{upstreamId}
GET    /api/v1/http-server/{id}/upstreams
GET    /api/v1/http-server/{id}/upstream/{upstreamId}
```

已确认：

- 不支持更新 upstream。
- 删除 upstream 无额外状态要求。
- 蓝绿发布通过新增同 `group` + 同 `name` upstream 触发。
- upstream 不需要单独的 enabled 接口，配置中也不包含 `enabled` 字段。
- 新增 upstream 时，除 `id` 由系统生成外，`name`、`group`、`host` 必填。
- 其它字段未传入时使用默认值。
- 删除 upstream 后，新请求不再选择该 upstream。
- 已经转发到该 upstream 的旧请求或旧连接继续自然结束，不强制中断。

## Route API

第一阶段 route 只支持新增和删除，不支持更新。

路径形式待进一步确认。

建议按 `http-server` 子资源设计：

```text
POST   /api/v1/http-server/{id}/routes
DELETE /api/v1/http-server/{id}/route/{routeId}
GET    /api/v1/http-server/{id}/routes
GET    /api/v1/http-server/{id}/route/{routeId}
```

已确认：

- 不支持更新 route。
- 删除 route 立即生效。
- route 顺序由系统维护。
- 不存在 route 修改失败语义，因为不提供 route 更新接口。
- 新增 route 时，除 `id` 由系统生成外，`match.type`、`match.path`、`action.type` 必填。
- 当 `action.type = proxy` 时，`action.proxy.upstream` 必填。
- 当 `action.type = file` 时，`action.file.dir` 必填。
- 其它字段未传入时使用默认值。
- 删除 route 后，新请求不再匹配该 route。
- 已经进入处理流程的旧请求继续处理，不强制中断。

## TCP Forward API

`tcp-forward` 管理 API 第一阶段暂不讨论。

后续需要单独设计：

- 列表。
- 新增。
- 更新。
- 启停。
- 删除。
- 配置查看。
- 状态查看。

## System API

系统状态接口第一阶段可以设计。

建议路径：

```text
GET /api/v1/system/status
```

可返回：

- 程序版本。
- 进程运行时间。
- 系统配置文件路径。
- 数据目录。
- 日志目录。
- `http-server` 数量。
- `tcp-forward` 数量。
- 活动连接数。
- 最近错误。

第一版字段：

```json
{
  "version": "string",
  "uptime": 0,
  "systemConfigPath": "string",
  "dataDir": "string",
  "logDir": "string",
  "httpServerCount": 0,
  "tcpForwardCount": 0,
  "activeConnectionCount": 0,
  "lastError": null
}
```

字段说明：

- `version`：程序版本。
- `uptime`：进程运行时间，单位秒。
- `systemConfigPath`：系统配置文件路径。
- `dataDir`：数据目录。
- `logDir`：日志目录。
- `httpServerCount`：HTTP 服务数量。
- `tcpForwardCount`：TCP 转发数量。
- `activeConnectionCount`：当前活动连接数。
- `lastError`：最近错误，没有错误时为 `null`。

## Reload / Retry API

因为运行环境错误会保留配置，所以需要支持重新应用配置。

建议第一阶段支持：

```text
POST /api/v1/http-server/{id}/reload
```

用途：

- 端口占用解除后重新绑定。
- 文件目录权限修复后重新加载。
- 运行时失败后重新应用当前配置。

`tcp-forward` reload 接口后续随 `tcp-forward` API 一起设计。

## 错误码

第一阶段错误码使用数字。

已确认：

- 成功时 `code = 0`。
- 失败时 `code` 为错误码。
- `message` 返回错误原因。

建议后续按模块划分错误码范围，例如：

```text
10000-19999 通用错误
20000-29999 配置错误
30000-39999 http-server 错误
40000-49999 upstream/route 错误
50000-59999 tcp-forward 错误
```

第一版错误码：

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
