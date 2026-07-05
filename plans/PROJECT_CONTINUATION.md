# 项目续接记录

## 当前状态

- 记录日期：2026-06-29
- 工作目录：`C:\Users\80982\Desktop\eaker\tunnel`
- 仓库状态：当前目录已存在 Git 仓库，当前分支为 `master`，尚无提交。
- 可见项目文件：根目录包含 `.idea`、`plans` 和 `nginx-release-1.31.2`。
- nginx 源码：根目录下存在 `nginx-release-1.31.2`，可作为参考源码。
- 实现状态：根据要求，本轮尚未开始任何实现工作。

## 工作约定

- 在目标、范围、约束和验收标准讨论清楚之前，不开始实现。
- 将计划、进度和未解决问题记录在本文档中，便于后续继续推进。
- 后续如需编辑业务代码或源码，应先在本文档中记录计划变更和讨论结论。

## 初步计划

1. 明确 Rust 重写 nginx 的目标边界。
2. 项目名称确定为 `yiz-tunnel`。
3. 软件级配置命名为 `system-conf`。
4. HTTP 服务配置命名为 `http-server`。
5. TCP 端口转发配置命名为 `tcp-forward`。
6. 项目配置文件采用 JSON 文件。
7. 规则配置存储也采用 JSON 文件，规则模型自行实现并逐步讨论。
8. 第一阶段需要实现 HTTP 代理能力。
9. 第一阶段需要实现 TCP 端口转发能力，但只支持 1 对 1 转发。
10. 第一阶段需要提供一个对外 HTTP 管理服务。
11. 管理服务需要支持管理 `http-server`、管理 `tcp-forward`、查看系统状态。
12. 第一阶段需要实现日志系统。
13. 第一阶段暂不考虑扩展机制。
14. 确认需要参考 nginx 的哪些核心逻辑：配置解析、事件循环、连接处理、HTTP 处理、代理处理、日志等。
15. 明确相较 nginx 需要调整或新增的行为。
16. 确认运行环境、平台、Rust 版本、异步运行时和部署目标。
17. 定义第一个可用版本的核心功能和明确不做的内容。
18. 定义验收标准以及测试/验证要求。
19. 在范围确认后，再检查或初始化实际项目结构。
20. 将实现拆分为小里程碑。
21. 仅在上述事项确认后开始实现。

## 进度记录

### 2026-06-28

- 已检查工作目录根目录。
- 已检查 Git 状态；当前目录不是 Git 仓库。
- 已检查项目文件；未发现源码文件。
- 已创建本文档，用于记录计划、进度和待讨论问题。
- 未实现任何业务逻辑、应用代码、配置或项目结构。
- 已确认项目大方向：使用 Rust 重写 nginx，并在核心逻辑上基本参考 nginx，同时加入一些调整。

### 2026-06-29

- 已复查根目录状态：当前已有 Git 仓库，尚无提交；根目录下存在 `nginx-release-1.31.2`。
- 已确认第一批功能需求：
  - HTTP 代理。
  - TCP 端口转发，仅支持 1 对 1 转发。
  - 对外 HTTP 管理服务。
  - 日志系统。
- 已确认项目名称为 `yiz-tunnel`。
- 已确认软件级配置命名为 `system-conf`。
- 已确认 HTTP 服务配置命名为 `http-server`。
- 已确认 TCP 端口转发配置命名为 `tcp-forward`。
- 已确认项目配置文件采用 JSON 文件。
- 已确认规则配置存储采用 JSON 文件，规则模型自行实现并逐步讨论。
- 已确认第一阶段暂不考虑扩展机制。
- 已确认 `server` 的实际功能对应 nginx 中的 `server` 块，在本系统中表示 HTTP 服务。
- 已确认 `forwards` 在本系统中表示 TCP 端口转发。
- 已确认 HTTP server 与 TCP forward 是两个独立概念。
- 已确认管理服务需要支持管理 HTTP server、管理 TCP forward、查看系统状态。
- 已根据讨论创建 `plans/HTTP_SERVER_DESIGN.md`，记录 `http-server` 初版结构草案。
- 已调整 `http-server.listen` 为对象结构：包含 `host`、`port`、`serverName`，并为 `ssl`、`backlog`、`http2` 等后续参数预留位置。
- 已确认 `upstreams.status` 的重要用途：支持蓝绿发布场景。旧 upstream 进入 `deading` 状态后不再接收新请求/连接，等待已有请求/连接结束后通知外部关闭服务。
- 已确认蓝绿发布第一阶段不做额外外部通知能力，由第三方调用管理接口获取 upstream 状态。
- 已确认 `upstreams.priority` 数字越小优先级越高。
- 已确认系统内部状态可以使用数字，对外管理接口统一使用字符串状态。
- 已确认 `ssl` 第一阶段不实现。
- 早期确认 `http2` 第一阶段不实现；后续已补充 HTTP/2 cleartext prior-knowledge 入站能力，TLS + ALPN 后续实现。
- 已确认 `serverName` 第一阶段只支持固定匹配。
- 已确认 `upstreams` 新增 `name` 字段，用于标识 upstream 名称。
- 已提出蓝绿发布 group 方案：`upstreams` 添加 `group` 字段，`routes.action.proxy.upstream` 使用 `group` 作为路由目标。
- 已确认启用/禁用统一使用布尔字段 `enabled` 并持久化保存；`status` 只表示实际运行状态，不写入配置。
- 已确认 `http-server` 顶层新增 `graceful`、`conf`、`enabled`。
- 已确认路由字段使用 `routes`，不使用 `routers`。
- 已确认 `routes.match` 第一阶段只支持 `path`。
- 已确认 `routes.conf` 允许覆盖顶层 `conf`。
- 已确认 route 内部移除 `status`。
- 管理接口字段细节先不讨论，后续设计管理接口时再确认。
- 已确认 `http-server.enabled` 表示配置期望的启用/禁用状态。
- 已确认 `graceful.enabled` 表示配置期望的启用/禁用状态。
- 已确认 `graceful.type` 固定为 `0`，表示默认 graceful 规则。
- 已确认 `upstreams.host` 使用完整字符串地址，例如 `http://127.0.0.1:3000`。
- 已确认 `routes.match.type`：`0=full` 精确匹配，`1=prefix` 前缀匹配，`2=regex` 正则匹配；第一阶段只实现精确和前缀。
- 已确认 `routes.action.file` 包含 `dir` 和 `alias`，其中 `alias` 使用 `0=关闭, 1=开启`，开启后效果等同 nginx `alias`。
- 已确认第一阶段不实现 `try_file` 等其它文件处理特性。
- 已确认 `routes.action.proxy` 添加 `websocket` 配置，其中 `websocket.enabled` 使用布尔值。
- 已确认顶层 `conf` 和 `routes.conf` 具体字段先不考虑，后续增加其它特性时逐步讨论。
- 已确认 WebSocket 第一版需要实现且默认开启，其它配置先不考虑，后续再讨论。
- 已确认 `conf` 具体字段先不考虑，等具体实现相关特性时再确认。
- 已在 `plans/HTTP_SERVER_DESIGN.md` 中整理 `http-server` 达到最小可用前仍需确认的问题清单。
- 已确认 `http-server` 最小可用版本只支持 `file` 和 `proxy` 两种 action。
- 已确认 route 匹配、静态文件基础行为、proxy 基础行为、serverName/default server、超时和请求限制默认值整体按 nginx 默认行为设计。
- 已确认未匹配任何 route 时返回 404。
- 已确认静态文件默认按 nginx `root` 语义处理，当前禁止目录访问。
- 已确认 proxy 默认添加基础转发头。
- 已确认 `upstreams.conf` 用于覆盖 server 级配置。
- 已确认配置加载失败时拒绝启动。
- 已确认配置热更新属于最小可用范围。
- 已确认最小日志字段：请求时间、请求地址、对应服务、响应时间。
- 已确认 `http-server` 和 `graceful` 采用 `enabled` 与 `status` 分离的模型；upstream 不再包含 `enabled`，存在于配置中即表示期望启用。
- 已确认程序启动时根据 `enabled` 初始化运行状态，`deading` 和 `dead` 不跨进程重启恢复。
- 已开始讨论配置存储问题，并新增 `plans/CONFIG_STORAGE_DESIGN.md`。
- 已确认系统配置文件名为 `yiz-tunnel.json`。
- 已确认程序启动支持通过 `-c` 指定系统配置文件路径，例如 `yiz-tunnel -c /etc/yiz-tunnel.json`。
- 已确认未传入 `-c` 时，默认使用当前目录下的 `yiz-tunnel.json`。
- 已确认系统配置文件不存在时，按默认值生成系统配置文件。
- 已确认 `system-conf` 与规则持久化文件概念不同；`system-conf` 存放系统级配置，例如数据目录位置。
- 已确认 `http-server` 和 `tcp-forward` 规则分别持久化到独立文件中，不写入系统配置文件。
- 已确认规则持久化文件只存配置规则，不存运行数据。
- 已确认管理 API 修改配置采用先落盘、再调整运行时的模型。
- 已确认运行环境错误允许保留用户期望配置，并通过运行状态暴露失败原因。
- 已确认规则持久化写入需要事务化。
- 已确认配置文件需要版本字段。
- 已确认 ID 由软件生成，不由用户传入。
- 已确认结构错误直接报错，不落盘、不处理。
- 已确认管理 API 只允许单个对象操作，因此运行时执行局部 reload。
- 已确认第一阶段不考虑手工修改配置文件；程序启动时读取，运行中变动由管理 API 写入。
- 已确认 `system-conf` 第一版采用最小结构：`version`、`data-dir`、`log-dir`、`admin`、`runtime`。
- 已确认管理 HTTP 服务第一阶段不考虑鉴权。
- 已确认默认数据目录为 `data`，默认日志目录为 `logs`。
- 已确认日志按类型分文件输出。
- 已确认启动时不主动生成 `http-server.json` 和 `tcp-forward.json`，用户通过管理 API 新增规则时再生成。
- 已开始讨论管理 API 设计，并新增 `plans/MANAGEMENT_API_DESIGN.md`。
- 已确认管理 API 路径带版本号，第一阶段使用 `/api/v1`。
- 已确认管理 API 通用响应结构为 `{code:number,message:string,data:...}`。
- 已确认成功响应 `code=0`、`message="ok"`，失败响应 `code` 为错误码、`message` 为错误原因。
- 已确认 `http-server` 管理接口第一阶段包括列表、新增、更新蓝绿发布信息、启停、删除、查看配置、查看状态。
- 已确认删除 `http-server` 必须要求服务处于停止状态。
- 已确认 upstream 和 route 只需要支持新增和删除，不支持更新。
- 已确认 `tcp-forward` 管理 API 暂不讨论。
- 已确认系统状态接口可以进入第一阶段设计。
- 已确认需要 reload/retry 接口，用于重新应用已落盘但运行失败的配置。
- 已确认错误码后续需要设计。
- 已确认 `http-server` 顶层新增 `alias` 字段，仅作为别名，无实际运行语义。
- 已确认移除 `upstreams.enabled` 字段；用户传输的 upstream 都表示期望启用。
- 已确认新增 `http-server` 接口只接受 `alias`、`listen`、`conf`、`graceful`。
- 已确认编辑 `http-server` 接口只接受 `alias`、`listen`、`conf`、`graceful`。
- 已确认 upstream API 只允许新增和删除，不支持更新。
- 已确认删除 upstream 无额外状态要求。
- 已确认蓝绿发布通过新增同 `group` + 同 `name` upstream 触发。
- 已确认不需要 upstream enabled 接口。
- 已确认 route API 只允许新增和删除，不支持更新。
- 已确认删除 route 立即生效。
- 已确认 route 顺序由系统维护。
- 已确认新增 upstream 时，除 `id` 由系统生成外，`name`、`group`、`host` 必填，其它字段使用默认值。
- 已确认新增 route 时，除 `id` 由系统生成外，`match.type`、`match.path`、`action.type` 必填；`proxy` action 必填 `proxy.upstream`；`file` action 必填 `file.dir`；其它字段使用默认值。
- 已确认 `http-server` 运行状态包括 `starting`、`running`、`stopping`、`stopped`、`failed`。
- 已确认 `http-server` 删除时，`enabled=false` 即可期望删除；内部可维护 `delete` 状态等待已有连接或请求自然结束。
- 已确认 `http-server`、route、upstream 删除都遵循同一原则：新请求不再进入，旧请求或旧连接保留并自然结束。
- 已确认 `enabled` 接口请求体只需要传 `enabled`。
- 已确认 reload 接口只重试运行时应用当前已落盘配置，不修改配置文件。
- 已生成第一版管理 API 错误码表。
- 已确认 `conf` 第一阶段先参考 nginx 常用配置项和默认值，后续不够再补充。
- 已确认 `conf` 字段名暂时沿用 nginx 指令名，时间单位使用毫秒，大小单位使用字节。
- 已确认第一版 `conf` 包含 `client_max_body_size`、`client_header_timeout`、`client_body_timeout`、`send_timeout`、`keepalive_timeout`、`keepalive_requests`、`proxy_connect_timeout`、`proxy_send_timeout`、`proxy_read_timeout`。
- 已确认系统状态接口第一版字段：程序版本、进程运行时间、系统配置文件路径、数据目录、日志目录、HTTP 服务数量、TCP 转发数量、活动连接数、最近错误。
- 已确认日志设计后续参考 nginx 再讨论。
- 已开始记录技术选型原则，并新增 `plans/TECHNICAL_DESIGN.md`。
- 已确认核心部分以性能为主，尽量不使用外部依赖。
- 已确认管理 API 性能要求不高，可以使用外部 HTTP 服务库。
- 已确认 `tcp-forward` 先不进入最小可用实现。
- 已新增 `plans/MVP_SCOPE.md`，记录 HTTP MVP 范围。
- 早期确认 HTTP MVP 第一版只支持 HTTP/1.1；后续已补充 HTTP/2 cleartext prior-knowledge 入站支持，HTTPS、TLS + ALPN、HTTP/3 仍未实现。
- 已确认 proxy 除明确要求外参考 nginx；WebSocket 需要实现且默认开启。
- 已确认静态文件第一版行为：404、403、少量内置 MIME、不支持 Range/ETag/Last-Modified/index。
- 已确认 route 匹配规则：full 优先 prefix，prefix 取最长匹配，同一 `http-server` 下禁止重复 `match.type + match.path`。
- 已确认第一版日志文件为 `access.log`、`error.log`、`admin.log`。
- 已确认第一版 JSON 示例可以作为实现和测试依据。
- 已确认事件调度采用 `tokio`。
- 已确认 JSON 解析使用 `serde_json`。
- 已确认管理 API 使用 `axum`。
- 已确认 TLS 后续使用 `rustls`。
- 已确认请求头解析自行实现，可参考 nginx 源码。
- 已开始实现。
- 已在当前目录初始化 Rust 二进制项目 `yiz-tunnel`。
- 已新增 `Cargo.toml` 和 `Cargo.lock`。
- 已确认第一阶段依赖落地：`tokio`、`axum`、`serde`、`serde_json`、`uuid`。
- 已实现启动参数 `-c` 解析。
- 已实现系统配置文件 `yiz-tunnel.json` 不存在时按默认值生成。
- 已实现系统配置加载，并根据配置文件位置解析相对 `data-dir` 和 `log-dir`。
- 已实现数据目录和日志目录启动时创建。
- 已实现 `http-server.json` 缺失时按空规则加载，不主动生成。
- 已实现管理 API 服务启动。
- 已实现 `/api/v1/system/status`。
- 已实现 `http-server` 配置列表、新增、查看、更新、启停、删除。
- 已实现 upstream 新增、列表、查看、删除。
- 已实现 route 新增、列表、查看、删除。
- 已确认管理 API 路径按 RESTful 风格：列表和新增使用复数集合路径，单个资源查看、更新、删除使用资源 ID 路径。
- 已实现 `http-server.json` 事务化写入，包含临时文件、解析校验和备份文件。
- 已实现软件生成 ID，当前使用 UUID v7 并带类型前缀。
- 已实现统一 API 响应结构 `{code,message,data}`。
- 已实现第一版 route 结构校验和重复 `match.type + match.path` 拒绝。
- 已运行 `cargo fmt`、`cargo check`、`cargo build`。
- 已完成一次管理 API 冒烟测试：默认配置生成、system/status、http-server 新增、upstream 新增、route 新增、启停和删除均通过。
- 已开始实现实际 HTTP runtime。
- 已实现 enabled `http-server` 的监听启动。
- 已实现管理 API 创建、更新、启停、删除 `http-server` 时应用 runtime。
- 已实现 upstream 和 route 增删后更新 runtime 配置；监听地址不变时不重绑端口，只替换运行时配置快照。
- 已实现基础 HTTP/1.1 请求头解析。
- 已实现 route 匹配：精确匹配优先，前缀匹配取最长。
- 已实现静态文件响应基础能力。
- 已实现 HTTP proxy 基础能力。
- 已实现 WebSocket upgrade 请求的 TCP 双向转发路径。
- 已实现 `http-server` runtime 状态查询。
- 已新增自动测试：route 精确优先、最长前缀、静态文件路径安全、alias 路径、静态文件真实 I/O、proxy 真实 I/O。
- 已运行 `cargo test`，当前 6 个测试通过。
- 已完成一次 runtime 端到端验证：通过管理 API 创建静态文件 route 后，业务端口可返回文件内容。
- 已开始实现日志系统。
- 已新增 `src/logger.rs`。
- 已实现 JSON Lines 格式日志写入。
- 已实现 `logs/admin.log`，记录管理 API 成功变更操作。
- 已实现 `logs/access.log`，记录 HTTP runtime 请求日志。
- 已实现 `logs/error.log` 写入接口，并在 runtime 连接处理异常时记录错误。
- 已新增日志自动测试，验证 admin log 可落盘。
- 已运行 `cargo test`，当前 7 个测试通过。
- 已完成一次日志端到端验证：管理 API 创建服务和 route 后，`admin.log` 有操作记录；访问业务端口后，`access.log` 有请求记录。
- 已实现 proxy 基础转发头：`Host`、`X-Real-IP`、`X-Forwarded-For`、`X-Forwarded-Proto`。
- 已确认已有 `X-Forwarded-For` 会追加当前客户端地址。
- 已新增 proxy header 自动测试。
- 已运行 `cargo test`，当前 8 个测试通过。
- 已实现基础 HTTP/1.1 keep-alive 循环。
- 已确认 HTTP/1.1 默认保持连接，`Connection: close` 时关闭。
- 已确认 HTTP/1.0 仅在 `Connection: keep-alive` 时保持连接。
- 已确认静态文件和普通错误响应支持 keep-alive；proxy 和 WebSocket 路径处理后关闭当前连接。
- 已新增 keep-alive 真实 I/O 自动测试：同一 TCP 连接连续请求两个静态文件。
- 已运行 `cargo test`，当前 9 个测试通过。
- 已实现静态文件响应 `ETag` 和 `Last-Modified`。
- 已实现静态文件条件请求：`If-None-Match` 命中时返回 `304 Not Modified`。
- 已实现静态文件条件请求：`If-Modified-Since` 未过期时返回 `304 Not Modified`。
- 已新增静态文件缓存行为自动测试。
- 已运行 `cargo test`，当前 10 个测试通过。
- 已实现静态文件单段 byte range。
- 已支持 `Range: bytes=start-end`、`bytes=start-`、`bytes=-suffix`。
- 已实现合法范围返回 `206 Partial Content`，非法范围返回 `416 Range Not Satisfiable`。
- 已新增 Range 解析和真实 I/O 自动测试。
- 已运行 `cargo test`，当前 12 个测试通过。
- 已实现 `Transfer-Encoding: chunked` 请求体解析。
- 已确认 proxy 转发 chunked 请求时会先解码请求体，再使用 `Content-Length` 转发给 upstream，不透传原始 chunked 编码。
- 已新增 chunked 请求体 proxy 转发真实 I/O 自动测试。
- 已运行 `cargo test`，当前 13 个测试通过。
- 已实现第一版 `conf` 运行时生效。
- 已实现 `client_max_body_size`，请求体超过限制时返回 `413 Payload Too Large`。
- 已实现 `client_header_timeout`、`client_body_timeout`、`send_timeout`、`keepalive_timeout`、`keepalive_requests`。
- 已实现 `proxy_connect_timeout`、`proxy_send_timeout`、`proxy_read_timeout`。
- 已确认请求读取阶段使用 `http-server.conf`；route/upstream 阶段支持 `route.conf` 和 `upstreams.conf` 覆盖。
- 已实现 `conf` 写入校验：只允许第一版支持字段，值必须为正整数；非法结构返回 `400` 且不落盘。
- 已实现启动读取 `http-server.json` 时的 `conf` 和 route 基础结构校验，非法持久化文件会拒绝启动。
- 已新增 `client_max_body_size`、`keepalive_requests`、`proxy_read_timeout` 和 `conf` 校验自动测试。
- 已运行 `cargo test`，当前 18 个测试通过。
- 已实现 upstream 选择策略：同 group 内优先选择最小 `priority`，同 priority 使用轮询。
- 已确认轮询状态按 `http-server id + upstream group` 维护，跨连接共享。
- 已新增同 priority upstream 轮询真实 I/O 自动测试。
- 已运行 `cargo test`，当前 19 个测试通过。
- 已调整 HTTP runtime 连接处理：每个请求处理前重新读取最新 `http-server` 配置，避免 keep-alive 连接继续使用旧 route/upstream 配置。
- 已实现 upstream 活动请求计数，proxy 请求处理期间自动增加，处理结束自动减少。
- 已在 upstream 列表和查看接口中返回运行时字段 `status`、`activeRequestCount`。
- 已新增 keep-alive 热更新配置可见性和 upstream 活动请求计数自动测试。
- 已运行 `cargo test`，当前 21 个测试通过。
- 已实现 retired upstream 状态保留：删除或替换旧 upstream 时，如果仍有活动请求，则保留运行时快照。
- 已实现 retired upstream 状态计算：活动请求未归零时返回 `deading`，归零后返回 `dead`。
- 已确认 upstream 列表和查看接口会合并配置态 upstream 与 retired upstream。
- 已新增删除 active upstream 后 `deading` 到 `dead` 的自动测试。
- 已运行 `cargo test`，当前 22 个测试通过。
- 已实现新增同 `group + name` upstream 触发蓝绿替换：配置中移除旧 upstream，只保留新 upstream。
- 已确认被替换旧 upstream 如果仍有活动请求，会由 runtime 保留为 retired upstream 并暴露 `deading/dead` 状态。
- 已新增 storage 自动测试，验证同 `group + name` upstream 新增会替换旧 upstream。
- 已运行 `cargo test`，当前 23 个测试通过。
- 已实现 proxy 响应流式转发：读取到 upstream 响应头后立即转发给客户端，后续 body 分块转发，不再整体 `read_to_end` 后一次性写出。
- 已确认 proxy 响应头未返回前的 upstream 读取超时仍返回 `504`，读取错误仍返回 `502`。
- 已新增 proxy 流式响应自动测试，验证 upstream 未关闭前客户端可读到首段响应体。
- 已运行 `cargo test`，当前 24 个测试通过。
- 已实现 upstream 连接失败切换：按 priority 从小到大尝试，同 priority 仍按轮询顺序尝试。
- 已确认选中的 upstream 连接失败时会继续尝试同 group 的后续候选，全部失败后才返回 `502/504`。
- 已新增 upstream 连接失败后切换到后续 upstream 的真实 I/O 自动测试。
- 已运行 `cargo test`，当前 25 个测试通过。
- 已实现 `Content-Length` 请求体的 proxy 流式转发：读取请求头后不再等待完整 body，先转发已收到 body，再继续边读客户端 body 边写 upstream。
- 已确认 `Transfer-Encoding: chunked` 请求体暂时保持已有完整解码后转发逻辑。
- 已新增 proxy 请求体流式转发自动测试，验证 upstream 在客户端补齐 body 前可收到首段 body。
- 已运行 `cargo test`，当前 26 个测试通过。
- 已新增管理 API 端到端冒烟脚本 `scripts/smoke-management-api.ps1`。
- 冒烟脚本覆盖：独立临时实例启动、system/status、http-server 创建、file route 创建、业务端口静态文件访问、upstream 同 `group + name` 替换、非法 `conf` 返回 400。
- 已运行 `scripts/smoke-management-api.ps1`，验证通过。
- 已新增快速开始文档 `docs/GETTING_STARTED.md`。
- 快速开始文档覆盖：构建、启动、系统配置、目录结构、管理 API 基础、静态文件服务、proxy 服务、蓝绿替换、启停删除、冒烟测试和当前限制。
- 已在 `docs/MANAGEMENT_API.md` 中加入快速开始文档入口。
- 已实现 http-server graceful stop 基础行为：停止时关闭 listener，不再接收新连接；如果仍有活动连接，状态进入 `stopping`。
- 已实现活动连接归零后自动从 `stopping` 切换为 `stopped`。
- 已新增 http-server stop 状态切换自动测试。
- 已运行 `cargo test`，当前 27 个测试通过。
- 已新增根目录 `README.md`，作为项目首页和第一版能力说明入口。
- `README.md` 已覆盖项目定位、文档入口、构建启动、配置目录、已支持能力、自动验证、第一版限制和开发约定。
- 当前尚未实现更完整的 nginx proxy header 行为。
- 已创建管理 API 接口文档 `docs/MANAGEMENT_API.md`。
- 已实现自研 HTTP/2 cleartext prior-knowledge 入站支持，不直接依赖 `h2` / `http` / `bytes` 作为业务 HTTP runtime。
- 已实现基础 HTTP/2 frame 处理：SETTINGS、HEADERS、DATA、PING、CONTINUATION、GOAWAY。
- 已实现基础 HPACK 解码：静态表、连接级动态表、Huffman 字符串解码。
- 已新增 HTTP/2 静态文件、HTTP/2 proxy rewrite、HPACK Huffman 和 HPACK 动态表自动测试。
- 已运行 `cargo test --locked`，当前 33 个测试通过。

## 待讨论问题

1. 目标是完整替代 nginx，还是先实现 nginx 的一个子集？
2. “核心逻辑基本参考 nginx”具体包括哪些部分？
3. JSON 项目配置文件的路径和文件名是什么，例如 `config.json`、`yiz-tunnel.json` 或其它？
4. JSON 顶层结构是否采用 `system-conf`、`http-server`、`tcp-forward` 三个字段？
5. `http-server` 和 `tcp-forward` 在 JSON 中使用单数键名还是数组语义，例如 `"http-server": []`？
6. JSON 配置是否需要支持热重载？
7. 规则配置 JSON 与项目主配置 JSON 是同一个文件，还是拆成独立文件？
8. 规则模型如何定义：`http-server`、`tcp-forward`、route、upstream、listener 等概念是否需要区分？
9. 管理服务修改 `http-server`/`tcp-forward` 后，是立即生效、写入 JSON 后热重载，还是需要手动 reload？
10. HTTP 代理是反向代理、正向代理，还是两者都需要？
11. HTTP 代理第一阶段是否只做 HTTP/1.1，还是同时考虑 HTTP/2、HTTP/3、TLS、WebSocket？
12. HTTP 代理是否需要负载均衡、健康检查、重试、超时、限流、缓存、访问控制、Header 改写等能力？
13. TCP 1 对 1 转发是否需要支持 TLS 透传、连接超时、空闲超时、限速、访问控制？
14. HTTP 管理服务的 `http-server` 管理具体包括哪些操作：新增、删除、启停、更新、查看详情、查看连接数？
15. HTTP 管理服务的 `tcp-forward` 管理具体包括哪些操作：新增、删除、启停、更新、查看详情、查看转发状态？
16. 系统状态需要包含哪些信息：进程状态、运行时间、连接数、吞吐量、错误数、规则数量、监听端口、内存/CPU？
17. HTTP 管理服务是否需要认证、授权、绑定地址限制或 TLS？
18. 日志系统需要哪些日志类型：访问日志、错误日志、管理操作日志、运行指标日志？
19. 日志格式是否需要兼容 nginx，还是使用 JSON/结构化日志？
20. 日志输出目标是文件、控制台、轮转文件，还是后续接入外部日志系统？
21. 需要加入哪些相较 nginx 的调整？
22. 异步运行时倾向使用 `tokio`、`mio`，还是自研事件循环？
23. 是否允许直接阅读和参考 nginx 源码？如参考或移植代码，需要明确许可证和版权处理方式。
24. 是否存在性能、安全、兼容性或部署方面的硬性约束？
25. 完成前需要进行哪些验证，例如单元测试、集成测试、压测、与 nginx 行为对比测试？

## 已确认决策

- 项目方向：使用 Rust 重写 nginx。
- 项目名称：`yiz-tunnel`。
- 参考原则：核心逻辑基本参考 nginx。
- 参考源码：根目录下的 `nginx-release-1.31.2`。
- 变更方向：会在 nginx 基础上加入一些调整，具体调整项待讨论。
- 第一批功能范围：HTTP 代理、TCP 1 对 1 端口转发、对外 HTTP 管理服务、日志系统。
- 配置方式：项目配置文件采用 JSON 文件。
- 软件级配置命名：`system-conf`。
- HTTP 服务配置命名：`http-server`。
- TCP 端口转发配置命名：`tcp-forward`。
- 规则存储：规则配置存储采用 JSON 文件。
- 规则设计：规则模型自行实现，后续一步一步讨论。
- 扩展机制：第一阶段暂不考虑扩展。
- `http-server`：对应 nginx 中的 `server` 块，在本系统中表示 HTTP 服务。
- `tcp-forward`：对应本系统中的 TCP 端口转发。
- 概念边界：`http-server` 与 `tcp-forward` 相互独立。
- 管理服务范围：管理 `http-server`、管理 `tcp-forward`、查看系统状态。
- `http-server` 初版结构草案已记录在 `plans/HTTP_SERVER_DESIGN.md`。
- `upstreams.status` 需要支持蓝绿发布和优雅下线：`deading` 状态不接收新请求/连接，但保留已有请求/连接直到结束，并在清空后通知外部。
- 蓝绿发布状态获取：第一阶段由第三方调用管理接口查询状态，不做 Webhook、命令执行、消息队列等额外通知能力。
- `upstreams.priority`：数字越小优先级越高。
- 状态表达：配置使用布尔字段 `enabled` 表示启用/禁用；运行时使用独立的 `status`；对外管理接口使用字符串状态。
- `ssl`：第一阶段不实现；`http2` 已补充 cleartext prior-knowledge 入站能力，TLS + ALPN 后续实现。
- `serverName`：第一阶段只支持固定匹配。
- `upstreams.name`：用于标识 upstream 名称。
- `upstreams.group`：作为 `routes.action.proxy.upstream` 的路由目标。
- `http-server.graceful`：顶层配置，包含 `enabled:boolean` 和 `type:number`；运行时 `status` 不持久化。
- `routes`：替代原先的 `routers` 命名。
- `routes.match`：第一阶段只支持 `path`。
- `routes.conf`：允许覆盖顶层 `conf`。
- route 内部不再设置 `status`。
- `http-server.enabled` 和 `graceful.enabled`：使用布尔值表达配置期望的启用/禁用状态。
- `http-server.status`、`graceful.status` 和 `upstream.status`：只表达实际运行状态，不写入配置文件。
- `graceful.type`：固定为 `0`，表示默认 graceful 规则。
- `upstreams.host`：完整字符串地址，例如 `http://127.0.0.1:3000`。
- `routes.match.type`：`0=full`，`1=prefix`，`2=regex`；第一阶段只实现 `full` 和 `prefix`。
- `routes.action.file`：包含 `dir` 文件目录和 `alias` 开关，`alias=1` 时效果等同 nginx `alias`。
- `routes.action.proxy.websocket.enabled`：使用布尔值，表示是否启用 WebSocket 转发；其它选项后续讨论。
- `http-server` 最小可用 action：仅 `file` 和 `proxy`。
- route 未匹配行为：返回 404。
- nginx 默认行为：route 匹配、静态文件基础行为、proxy 基础行为、serverName/default server、超时和请求限制默认值均按 nginx 默认行为设计。
- 配置行为：加载失败拒绝启动，热更新属于最小可用范围。
- 配置存储：系统配置文件名为 `yiz-tunnel.json`，启动时可通过 `-c` 指定路径；未指定时使用当前目录下的 `yiz-tunnel.json`。
- 默认配置生成：系统配置文件不存在时，按默认值生成。
- 配置分层：`system-conf` 只存系统级配置，`http-server` 和 `tcp-forward` 规则分别保存为独立持久化文件。
- 规则持久化：规则文件只保存用户期望配置，不保存运行状态。
- 配置写入模型：管理 API 修改配置时先事务化落盘，再对单个对象执行局部 reload。
- 配置错误处理：结构错误拒绝落盘；运行环境错误保留配置并通过运行状态暴露。
- `system-conf` 第一版：包含 `version`、`data-dir`、`log-dir`、`admin`、`runtime`。
- 管理服务鉴权：第一阶段不考虑。
- 默认目录：数据目录为 `data`，日志目录为 `logs`。
- 日志存储：按日志类型分文件输出。
- 规则文件生成：启动时不主动生成规则文件，用户通过管理 API 新增规则时再生成。
- 管理 API：路径带版本号，第一阶段使用 `/api/v1`。
- 管理 API 响应：统一使用 `{code:number,message:string,data:...}`；成功 `code=0`、`message="ok"`。
- HTTP 服务管理：支持列表、新增、更新蓝绿发布信息、启停、删除、查看配置、查看状态。
- HTTP 服务删除约束：必须停止后才能删除。
- 子资源管理：upstream 只支持新增和删除，route 只支持新增和删除。
- HTTP 服务字段：`http-server` 顶层包含 `alias`，仅作为别名，无实际运行语义。
- HTTP 服务新增/编辑接口：只接受 `alias`、`listen`、`conf`、`graceful`。
- Upstream 启用语义：配置中不包含 `enabled`，用户传输的 upstream 都表示期望启用。
- Upstream 蓝绿发布：通过新增同 `group` + 同 `name` upstream 触发。
- Route 管理：不支持更新，删除立即生效，顺序由系统维护。
- 新增 upstream：`name`、`group`、`host` 必填，其它字段默认。
- 新增 route：`match.type`、`match.path`、`action.type` 必填；`proxy.upstream` 或 `file.dir` 按 action 类型必填。
- 删除语义：`http-server`、route、upstream 删除后不处理新请求，旧请求或旧连接继续自然结束。
- `http-server` 状态：`starting`、`running`、`stopping`、`stopped`、`failed`，内部删除流程可维护 `delete`。
- 错误码：第一版错误码表已记录在 `plans/MANAGEMENT_API_DESIGN.md`。
- `conf` 默认值：第一版参考 nginx 常用配置项，时间使用毫秒，大小使用字节。
- 系统状态接口：第一版返回程序版本、运行时间、配置路径、数据目录、日志目录、服务数量、活动连接数和最近错误。
- 技术选型：核心转发部分尽量不使用外部依赖；管理 API 可以使用外部 HTTP 服务库。
- HTTP MVP：`tcp-forward` 暂不进入最小可用实现，先实现 HTTP/1.1、管理 API、静态文件、反向代理、WebSocket、日志和状态查询。
- Proxy：除明确要求外参考 nginx；WebSocket 第一版实现且默认开启。
- Proxy rewrite：已确认 `action.proxy.rewrite` 放在 proxy 下，当前实现 `replacePrefix`，用于转发前替换 path 前缀，query string 保留；未来类型可继续扩展但当前会拒绝未知类型。
- 依赖：事件调度用 `tokio`，JSON 用 `serde_json`，管理 API 用 `axum`，后续 TLS 用 `rustls`，请求头解析自行实现。
- TCP 转发管理 API：第一阶段暂不讨论。
- reload/retry：需要支持重新应用已保存配置。

## 风险与未知项

- 当前工作目录已有 nginx 参考源码，但还没有 `yiz-tunnel` 的 Rust 源码，因此暂时没有本项目的既有代码风格或架构模式可遵循。
- Rust 重写 nginx 涉及网络 IO、事件模型、配置系统、HTTP 协议、代理行为和性能优化，范围较大，需要先收敛第一阶段目标。
- HTTP 代理、TCP 转发、管理服务和日志系统会共享运行时、配置、状态和观测能力，需要先确定边界，避免早期架构频繁返工。
- JSON 配置和规则存储需要设计清晰的 schema，否则管理服务、热更新和运行时状态容易混杂。
- 蓝绿发布需要准确维护 upstream 级别的活动请求数/连接数，否则无法可靠判断 `deading` upstream 是否可以关闭。
- 如果参考 nginx 源码，需要明确许可证、版权声明和代码来源边界，避免不清晰的代码移植风险。
- 在具体调整项未明确前，架构设计仍可能变化。
- 当前 Git 仓库尚无提交，后续应在实现前确认是否先提交现有计划文档和参考源码状态。

## 下一步讨论议程

1. 继续细化 `plans/HTTP_SERVER_DESIGN.md` 中的 `http-server` 字段语义。
2. 确认 HTTP 代理的具体类型和第一阶段协议范围。
3. 确认 TCP 1 对 1 转发的配置方式和连接行为。
4. 设计 JSON 主配置和规则配置的第一版 schema，明确 `system-conf`、`http-server`、`tcp-forward` 的独立结构。
5. 确认 HTTP 管理服务的接口范围和安全要求。
6. 确认 `http-server` 管理、`tcp-forward` 管理和系统状态查看的具体 API。
7. 确认日志系统的日志类型、格式和输出目标。
8. 明确需要调整 nginx 的哪些行为或架构。
9. 确认 Rust 技术选型：异步运行时、TLS 库、HTTP 解析库、日志库、配置解析策略。
10. 确认许可证和源码参考边界。
11. 用具体的用户可感知结果定义第一个里程碑。
12. 将已确认事项整理为实现清单。
13. 在完成上述讨论后再开始实现。
