# http-server 设计草案

## 定位

`http-server` 对应 nginx 中的 `server` 块，在 `yiz-tunnel` 中表示一个 HTTP 服务。

`http-server` 与 `tcp-forward` 是独立概念：

- `http-server`：处理 HTTP 监听、匹配、路由、文件响应、HTTP 代理等。
- `tcp-forward`：处理 TCP 端口 1 对 1 转发。

## 当前确认点

- `listen` 使用对象结构。
- `ssl` 第一阶段不实现。
- `http2` 已补充 cleartext prior-knowledge 入站能力，TLS + ALPN 后续实现。
- 当前支持 HTTP/1.1 和 HTTP/2 cleartext prior-knowledge。
- serverName 第一阶段只支持固定匹配，不支持通配符和正则。
- `http-server` 添加 `alias` 字段，仅作为别名，无实际运行语义。
- 路由字段使用 `routes`，不使用 `routers`。
- 路由匹配第一阶段只支持 `path`。
- `upstreams.host` 包含完整目标地址，并新增 `name` 字段。
- `http-server` 和 `graceful` 的启用/禁用使用布尔字段 `enabled`，并持久化保存。`status` 只表示实际运行状态，不写入配置文件。
- `upstreams` 不再包含 `enabled` 字段；用户传输的 upstream 都表示期望启用。
- 对外管理接口使用字符串展示 `status`。
- `routes.action.proxy` 使用 upstream 的 `group` 字段引用目标 upstream 组。
- `routes.conf` 允许覆盖顶层 `conf`。
- route 内部移除 `status`。
- `http-server` 顶层添加 `conf` 和 `enabled`。
- `http-server.enabled` 表示配置期望的启用/禁用状态，使用布尔值。
- `http-server` 顶层添加 `graceful`，用于蓝绿发布/优雅下线。
- `graceful.enabled` 表示 graceful 启用/禁用，使用布尔值。
- `graceful.type` 固定为 `0`，表示默认 graceful 规则。
- 管理接口细节先不讨论，后续设计管理接口时再确认。
- WebSocket 需要实现，默认开启。其它 WebSocket 配置先不讨论。
- 顶层 `conf` 和 `routes.conf` 的具体字段先不讨论，等具体实现相关特性时再确认。
- 最小可用版本只支持 `file` 和 `proxy` 两种 action。
- route 匹配规则按 nginx 默认行为设计。
- 未匹配任何 route 时返回 404。
- 静态文件默认按 nginx `root` 语义处理，当前禁止目录访问。
- 静态文件安全和基础响应行为按 nginx 默认行为设计。
- proxy path/query、错误处理等基础行为按 nginx 默认行为设计。
- proxy 默认添加基础转发头。
- upstream 同优先级选择按 nginx 默认行为设计。
- 多个 `http-server` 监听同一端口时，serverName/default server 选择按 nginx 默认行为设计。
- 超时和请求限制默认值按 nginx 默认行为设计，并允许在顶层 `conf` 配置。
- upstream 中添加 `conf`，用于覆盖 server 级配置。
- 配置加载失败时拒绝启动。
- 配置热更新属于最小可用范围。

## 单个 http-server 初版结构

```jsonc
{
  "id": "string",
  "alias": "string",
  "enabled": true,
  "listen": {
    "host": "string",
    "port": 0,
    "serverName": ["string"]
  },
  "graceful": {
    "enabled": true,
    "type": 0
  },
  "conf": {},
  "upstreams": [
    {
      "id": "string",
      "group": "string",
      "name": "string",
      "host": "string",
      "priority": 0,
      "conf": {}
    }
  ],
  "routes": [
    {
      "id": "string",
      "match": {
        "type": 0,
        "path": "string"
      },
      "action": {
        "type": "file",
        "file": {
          "dir": "string",
          "alias": 0
        },
        "proxy": {
          "upstream": "string",
          "websocket": {
            "enabled": true
          }
        }
      },
      "conf": {}
    }
  ]
}
```

## 字段说明

### 顶层字段

- `id`：HTTP 服务唯一标识。
- `alias`：HTTP 服务别名，仅用于展示和识别，无实际运行语义。
- `enabled`：HTTP 服务配置期望的启用/禁用状态，使用布尔值，并持久化保存。`status` 表示实际运行状态，仅存在于运行时和管理接口中。
- `listen`：监听配置对象。
- `graceful`：蓝绿发布/优雅下线配置。
- `conf`：HTTP 服务级配置。
- `upstreams`：该 HTTP 服务可用的上游目标列表。
- `routes`：该 HTTP 服务下的路由规则列表。

### http-server 运行状态

`http-server` 运行时使用独立 `status`，不写入配置文件。

第一阶段状态：

```text
starting
running
stopping
stopped
failed
```

内部删除流程可维护 `delete` 状态。

`delete` 表示配置层已经期望删除或已经删除，但运行时仍可能保留已有连接或请求，等待其自然结束后再清理内部对象。

### listen

- `host`：监听地址，例如 `0.0.0.0`。
- `port`：监听端口，例如 `8080`。
- `serverName`：HTTP Host 匹配名称列表，对应 nginx 的 `server_name`，使用字符串数组。

第一阶段不实现：

- `ssl`
- `http2`
- 其它高级监听参数

### graceful

- `enabled`：是否启用 graceful/蓝绿发布能力，使用布尔值，并持久化保存。`status` 表示实际运行状态，仅存在于运行时和管理接口中。
- `type`：graceful 类型，使用数字。当前固定为 `0`，表示默认规则。

当前只考虑一种 graceful 类型：

- 新请求全部走新服务。
- 旧服务进入 `deading` 状态。
- 旧服务不再接收新请求/连接。
- 旧服务已有请求/连接继续保持，直到自然断开。
- 外部系统通过管理接口查询状态后，自行决定何时关闭旧服务。

### conf

`conf` 表示 HTTP 服务级配置，具体结构后续讨论。

`routes.conf` 允许覆盖顶层 `conf`。

第一阶段先参考 nginx 常用配置项和默认值，后续不够再补充。

`conf` 字段名暂时沿用 nginx 指令名；时间单位使用毫秒，大小单位使用字节。

第一版默认值：

```json
{
  "client_max_body_size": 1048576,
  "client_header_timeout": 60000,
  "client_body_timeout": 60000,
  "send_timeout": 60000,
  "keepalive_timeout": 75000,
  "keepalive_requests": 1000,
  "proxy_connect_timeout": 60000,
  "proxy_send_timeout": 60000,
  "proxy_read_timeout": 60000
}
```

字段说明：

- `client_max_body_size`：请求体最大大小，默认 `1048576` 字节，对应 nginx 默认 `1m`。
- `client_header_timeout`：读取请求头超时，默认 `60000` 毫秒。
- `client_body_timeout`：读取请求体超时，默认 `60000` 毫秒。
- `send_timeout`：向客户端发送响应超时，默认 `60000` 毫秒。
- `keepalive_timeout`：客户端 keep-alive 空闲超时，默认 `75000` 毫秒。
- `keepalive_requests`：单个 keep-alive 连接最大请求数，默认 `1000`。
- `proxy_connect_timeout`：连接 upstream 超时，默认 `60000` 毫秒。
- `proxy_send_timeout`：向 upstream 发送请求超时，默认 `60000` 毫秒。
- `proxy_read_timeout`：读取 upstream 响应超时，默认 `60000` 毫秒。

默认值来源参考本仓库 `nginx-release-1.31.2`：

- `ngx_http_core_module.c` 中 `client_header_timeout` 默认 `60000`。
- `ngx_http_core_module.c` 中 `client_max_body_size` 默认 `1 * 1024 * 1024`。
- `ngx_http_core_module.c` 中 `client_body_timeout` 默认 `60000`。
- `ngx_http_core_module.c` 中 `send_timeout` 默认 `60000`。
- `ngx_http_core_module.c` 中 `keepalive_timeout` 默认 `75000`。
- `ngx_http_core_module.c` 中 `keepalive_requests` 默认 `1000`。
- `ngx_http_proxy_module.c` 中 `proxy_connect_timeout`、`proxy_send_timeout`、`proxy_read_timeout` 默认 `60000`。

### upstreams

- `id`：上游唯一标识。
- `group`：上游分组，`routes.action.proxy.upstream` 使用该字段进行路由目标选择。
- `name`：上游名称，用于标识 upstream 名称。
- `host`：完整上游目标地址，使用字符串，例如 `http://127.0.0.1:3000`。
- `priority`：优先级，数字越小优先级越高。
- `conf`：upstream 级配置，用于覆盖 server 级配置。

`upstreams` 不包含 `enabled` 字段。

用户通过管理 API 新增或传输的 upstream 都表示期望启用。upstream 不提供单独启用/禁用配置；需要停止接收新流量时，通过蓝绿发布进入 `deading`/`dead`，或直接删除 upstream。

新增 upstream 时，除 `id` 由系统生成外，以下字段必填：

- `name`
- `group`
- `host`

其它字段未传入时使用默认值。

## upstream 蓝绿发布状态设计

upstream 运行时 `status` 的设计目标之一，是优化蓝绿发布场景。

场景：

1. 当前 `http-server` 使用旧 upstream 对外提供服务。
2. 用户新增一个 upstream，此时同一个 `http-server` 下存在新旧两个 upstream。
3. 开启蓝绿发布后，旧 upstream 进入 `deading` 状态。
4. `deading` 状态下，旧 upstream 不再接收新的请求/连接。
5. 旧 upstream 上已有的请求/连接继续处理，直到自然结束。
6. 当旧 upstream 的请求/连接全部结束后，第三方通过管理接口查询到状态，并自行关闭旧服务。

该能力与 nginx 的优雅下线能力类似，但这里额外要求系统可观察并暴露 upstream 状态。

nginx 可以实现类似的流量迁移，但外部通常难以直接知道旧 upstream 内部是否已经完全无请求/无连接。本项目需要在 `yiz-tunnel` 内部维护 upstream 状态和连接/请求计数，以便判断旧 upstream 何时可以安全关闭。

### 状态语义

upstream 配置文件不保存 `enabled`。运行时使用独立的 `status`，对外管理接口统一使用字符串状态。

启动时配置中存在的 upstream 初始化为 `active`。`deading` 和 `dead` 不跨进程重启恢复。

状态语义：

- `active`：正常接收新请求/连接。
- `deading`：不再接收新请求/连接，只等待已有请求/连接结束。
- `dead`：已经无活动请求/连接，可以认为下线完成。

运行时内部可使用以下数字状态映射：

```text
0 = active
1 = deading
2 = dead
```

管理接口对外返回字符串，例如：

```json
{
  "status": "deading"
}
```

### 需要维护的运行时数据

为了支持 `deading` 判断，运行时至少需要维护：

- 每个 upstream 当前活动请求数。
- 每个 upstream 当前活动连接数。
- 每个 upstream 是否允许接收新请求。
- 每个 upstream 最近状态变更时间。

### 外部状态获取

第一阶段不做 Webhook、命令执行、消息队列等额外通知能力。

外部系统通过调用管理接口获取 upstream 状态，并自行判断何时关闭旧服务。

管理接口具体字段后续设计管理接口时再讨论。

## routes

- `id`：路由唯一标识。
- `match`：匹配条件。
- `action`：匹配后的处理动作。
- `conf`：路由级配置，可覆盖顶层 `conf`。

route 内部不再设置 `status`。

新增 route 时，除 `id` 由系统生成外，以下字段必填：

- `match.type`
- `match.path`
- `action.type`

当 `action.type = proxy` 时，以下字段必填：

- `action.proxy.upstream`

当 `action.type = file` 时，以下字段必填：

- `action.file.dir`

其它字段未传入时使用默认值。

删除 route 立即对新请求生效。已经进入处理流程的旧请求继续处理，不强制中断。

多个 route 同时匹配时，选择规则按 nginx 默认行为设计。

第一阶段未实现 regex 时，按以下方向理解：

- 精确匹配优先。
- 前缀匹配选择最长匹配。
- 未匹配任何 route 时返回 404。

### match

`match` 包含：

- `type`：匹配类型。
- `path`：匹配路径，具体语义由 `type` 决定。

匹配类型：

```text
0 = full   精确匹配
1 = prefix 前缀匹配
2 = regex  正则匹配
```

第一阶段只实现：

- `0 = full`
- `1 = prefix`

第一阶段不实现：

- `2 = regex`

示例：

```json
{
  "type": 0,
  "path": "/api"
}
```

暂不支持：

- method
- host
- header
- query
- 通配符匹配

### action

`action.type` 初步计划支持：

- `file`：文件响应。
- `proxy`：HTTP 代理。

最小可用版本只支持：

- `file`
- `proxy`

其它类型后续逐步讨论，例如 `rewrite`、`redirect`、`return`。

当 `type` 为 `file` 时，使用 `action.file`。

`action.file` 第一版结构：

```json
{
  "dir": "/var/www/html",
  "alias": 0
}
```

- `dir`：文件目录。
- `alias`：是否开启 alias 语义。

`alias` 取值：

```text
0 = 关闭
1 = 开启
```

当 `alias = 1` 时，效果等同于 nginx 中的 `alias`。

当 `alias = 0` 时，默认按 nginx `root` 语义处理。

当前禁止目录访问。

静态文件安全和基础响应行为按 nginx 默认行为设计。

第一阶段不实现其它文件处理特性，例如 `try_file`。

当 `type` 为 `proxy` 时，使用 `action.proxy`。

`action.proxy` 使用 upstream 的 `group` 字段引用目标 upstream 组。

proxy 默认行为：

- path/query 处理按 nginx 默认行为设计。
- 错误处理按 nginx 默认行为设计。
- 默认添加基础转发头。

示例：

```json
{
  "type": "proxy",
  "proxy": {
    "upstream": "api",
    "websocket": {
      "enabled": true
    }
  }
}
```

其中 `proxy.upstream` 的值是 upstream `group`，不是 upstream `name`。

`action.proxy.websocket`：

- `enabled`：是否启用 WebSocket 转发，使用布尔值。
- WebSocket 第一阶段需要实现，并且默认开启。
- 其它 WebSocket 选项先不讨论，等具体实现 WebSocket 时再确认。

## 待讨论问题

当前剩余待讨论问题：

1. 蓝绿发布 group 方案的约束是否按本文档建议执行。
2. 顶层 `conf`、`routes.conf`、`upstreams.conf` 的具体字段后续随着实现逐步细化。

## 蓝绿发布 group 方案评估

### 提议

- `upstreams` 添加 `group` 字段。
- `routes.action.proxy.upstream` 使用 `group` 作为路由目标。
- 原有 `name` 字段用于标识 upstream 名称。
- 开启蓝绿发布后，如果新添加的 upstream 的 `name` 已经存在，则代表激活蓝绿发布。
- 新添加的 upstream 作为新服务。
- 已经存在的 upstream 作为旧服务，进入 `deading` 状态。

### 优点

- route 只引用 `group`，发布时不需要改 route。
- 同一个 group 下可以表达同一类后端服务。
- 新旧服务切换可以只发生在 upstream 层。
- 适合由管理接口新增 upstream 来触发蓝绿发布。

### 需要约束的问题

该方案可行，但需要增加约束，避免歧义：

1. `id` 必须全局唯一。
2. `group` 应作为 route 的稳定引用目标。
3. `name` 不能再被视为唯一标识；蓝绿发布过程中，同一 `group` 下允许短暂存在相同 `name` 的新旧 upstream。
4. 只有在 `graceful.enabled = true` 时，才允许通过同 `group` + 同 `name` 触发蓝绿发布。
5. 如果 `graceful.enabled = false`，新增同 `group` + 同 `name` 的 upstream 应拒绝，避免误触发。
6. 如果同一 `group` + 同一 `name` 下已经存在多个 active upstream，应拒绝再次触发蓝绿，避免无法判断哪个是旧服务。
7. 旧 upstream 进入 `deading` 后不再接收新请求/连接。
8. 新 upstream 进入 `active` 后接收新请求/连接。
9. 旧 upstream 活动请求/连接归零后进入 `dead`，等待外部通过管理接口确认并关闭旧服务。
10. 管理接口必须暴露 `id`、`group`、`name`、`status`、活动请求数、活动连接数，否则外部无法可靠判断状态。

### 风险

- 如果 `name` 可以重复，管理接口和日志中必须始终带上 `id`，否则定位具体 upstream 会有歧义。
- 如果不同 `group` 下允许相同 `name`，蓝绿触发判断必须限定在同一 `group` 内。
- 如果旧 upstream 进入 `dead` 后不清理，配置文件会长期保留同名历史 upstream，需要后续定义清理策略。

## 最小可用确认项

### route 与匹配

- route 匹配规则按 nginx 默认行为设计。
- 未匹配任何 route 时返回 404。
- `routes.match.type` 支持 `full`、`prefix`、`regex` 三类语义。
- 第一阶段只实现 `full` 和 `prefix`。

### action

- 最小可用版本只支持 `file` 和 `proxy`。
- `file` 默认按 nginx `root` 语义处理。
- 当前禁止访问目录。
- 静态文件安全和基础响应行为按 nginx 默认行为设计。
- `proxy` path/query 和错误处理按 nginx 默认行为设计。
- proxy 默认添加基础转发头。

### upstream

- `upstreams.host` 使用完整字符串地址，例如 `http://127.0.0.1:3000`。
- `upstreams.priority` 数字越小优先级越高。
- 同优先级 upstream 的选择按 nginx 默认行为设计。
- `upstreams.conf` 可覆盖 server 级配置。

### conf

- 超时默认值按 nginx 默认行为设计，并允许在顶层 `conf` 配置。
- 请求大小限制默认值按 nginx 默认行为设计，并允许在顶层 `conf` 配置。
- `upstreams.conf` 可覆盖 server 级配置。

### 日志

最小日志字段：

- 请求时间。
- 请求地址。
- 对应服务。
- 响应时间。

### 配置

- 配置加载失败时拒绝启动。
- 配置热更新属于最小可用范围。
- `http-server` 和 `graceful` 的启用/禁用使用配置字段 `enabled`；upstream 不包含 `enabled`，存在于配置中即表示期望启用；`status` 仅表示实际运行状态，具体异常状态后续按对象分别定义。

