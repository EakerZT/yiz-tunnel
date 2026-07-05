# yiz-tunnel 与 nginx 能力对比

本文按 nginx 常见能力重新梳理 `yiz-tunnel` 当前已实现、部分实现和未实现的范围。

状态说明：

- 已实现：当前代码已经具备可验证能力。
- 部分实现：已有基础路径，但与 nginx 完整能力仍有明显差距。
- 未实现：当前没有对应运行时能力。
- 差异设计：不是直接复制 nginx，而是本项目刻意采用不同模型。

## 总体定位

| 维度 | nginx | yiz-tunnel 当前状态 | 结论 |
| --- | --- | --- | --- |
| 项目定位 | 通用 Web server、反向代理、负载均衡、stream 代理、缓存、网关能力 | API 管理的轻量 HTTP tunnel / reverse proxy | 部分实现 |
| 配置方式 | 文本配置文件，支持 include、上下文、指令语法 | JSON 系统配置 + 管理 API 写入规则文件 | 差异设计 |
| 运行时管理 | reload 配置、master/worker 信号控制 | 管理 API 创建、更新、启停、删除、reload 单个 HTTP 服务 | 差异设计 |
| 核心目标 | 高性能通用服务器 | 先做 HTTP 管理、静态文件、HTTP proxy、WebSocket、基础 HTTP/2 | 部分实现 |

## 配置与持久化

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| `nginx -c <path>` 指定配置 | 已实现 | `yiz-tunnel -c <path>`；不传时使用当前目录 `yiz-tunnel.json`。 |
| 配置文件不存在时报错 | 差异设计 | yiz-tunnel 会按默认值生成系统配置文件。 |
| 文本配置语法、上下文、指令解析 | 未实现 | 当前不兼容 nginx 配置语法。 |
| `include` 多文件配置 | 未实现 | 当前系统配置和 HTTP 规则文件固定分工。 |
| 系统配置与业务规则分离 | 已实现 | 系统配置保存 data/log/admin；HTTP 规则保存到 data 下。 |
| 配置热 reload | 部分实现 | 管理 API 操作会落盘并应用；支持单个 HTTP server reload。 |
| 配置事务写入 | 已实现 | HTTP 规则写入包含临时文件、校验和备份。 |

## 进程与事件模型

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| master/worker 多进程模型 | 未实现 | 当前是单进程 tokio runtime。 |
| worker 进程平滑重启 | 未实现 | 当前没有 nginx 式 worker replacement。 |
| 事件驱动网络 IO | 已实现 | 基于 tokio。 |
| `accept_mutex`、多 worker 连接分配 | 未实现 | 当前没有多 worker 竞争模型。 |
| 信号控制 reload/quit/reopen | 未实现 | 当前主要通过管理 API 控制。 |
| graceful stop | 部分实现 | 停止 listener，不接收新连接；已有连接自然结束后进入 stopped。 |

## 管理与控制面

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| 原生命令行和信号管理 | 部分实现 | 只实现 `-c` 和 Ctrl-C 退出。 |
| 原生管理 API | 差异设计 | nginx 开源版没有同等内置管理 API；yiz-tunnel 以 API 管理为核心。 |
| HTTP server 增删改查 | 已实现 | `/api/v1/http-servers`。 |
| upstream 增删查 | 已实现 | 支持新增、列表、查看、删除。 |
| route 增删查 | 已实现 | 当前无 route update。 |
| 统一响应结构 | 已实现 | `{ code, message, data }`。 |
| 管理 API 鉴权 | 未实现 | 当前第一阶段不鉴权。 |

## HTTP/1.x 协议

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| HTTP/1.0 / HTTP/1.1 请求处理 | 部分实现 | 支持当前静态文件和 proxy 所需解析路径。 |
| keep-alive | 已实现 | 支持 HTTP/1.1 默认 keep-alive 和请求数限制。 |
| 请求头解析 | 部分实现 | 自实现基础解析，未覆盖 nginx 全部边界。 |
| `Content-Length` 请求体 | 已实现 | proxy 可流式转发。 |
| chunked 请求体 | 部分实现 | 可解析并转发，但当前会先完整解码后再转 upstream。 |
| chunked 请求体直接流式 proxy | 未实现 | 后续可补。 |
| trailer | 未实现 | 当前未处理 trailer。 |
| 请求头大小/缓冲区完整配置 | 未实现 | 只有当前 `conf` 中的超时和 body size。 |

## HTTP/2

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| HTTP/2 over TLS + ALPN | 未实现 | 当前没有 TLS。 |
| HTTP/2 cleartext prior-knowledge | 已实现 | 自实现入站识别和 frame 处理。 |
| HTTP/1.1 `Upgrade: h2c` | 未实现 | 当前只支持 prior-knowledge。 |
| SETTINGS / HEADERS / DATA / CONTINUATION | 已实现 | 支持基础请求路径。 |
| PING / GOAWAY | 部分实现 | PING ACK 和 GOAWAY 退出已支持。 |
| RST_STREAM | 未实现 | 尚未实现 stream reset。 |
| PRIORITY | 未实现 | 当前只跳过 HEADERS priority 字段，不实现调度。 |
| WINDOW_UPDATE / 流控 | 未实现 | 尚未实现连接级和 stream 级流控。 |
| 完整 stream 状态机 | 未实现 | 当前是最小请求处理状态。 |
| HPACK 静态表 | 已实现 | 支持静态表索引。 |
| HPACK 动态表 | 已实现 | 支持连接级动态表和 incremental indexing。 |
| HPACK Huffman | 已实现 | 支持字符串 Huffman 解码。 |
| HTTP/2 server push | 未实现 | nginx 相关能力也已逐步弱化；本项目不作为当前目标。 |

## TLS / HTTPS

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| HTTPS listener | 未实现 | 已预留后续使用 rustls 的方向。 |
| 证书配置 | 未实现 | 当前无证书模型。 |
| SNI | 未实现 | 当前无 TLS。 |
| ALPN | 未实现 | 当前 HTTP/2 不走 TLS。 |
| TLS session / cipher 配置 | 未实现 | 当前无对应能力。 |

## 静态文件

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| `root` 语义 | 已实现 | `alias=0`。 |
| `alias` 语义 | 已实现 | `alias=1`。 |
| 路径穿越防护 | 已实现 | 拒绝 `..` 等不安全路径。 |
| MIME type | 部分实现 | 内置少量常见类型。 |
| `ETag` | 已实现 | 支持。 |
| `Last-Modified` | 已实现 | 支持。 |
| `If-None-Match` | 已实现 | 命中返回 304。 |
| `If-Modified-Since` | 已实现 | 未过期返回 304。 |
| `Range` | 部分实现 | 支持单段 byte range。 |
| 多段 range | 未实现 | 当前不支持 multipart range。 |
| `index` | 未实现 | 目录访问当前返回 403。 |
| `try_files` | 未实现 | 当前无等价能力。 |
| autoindex | 未实现 | 当前无目录列表。 |
| sendfile/aio/directio | 未实现 | 当前没有 nginx 式文件发送优化配置。 |
| open file cache | 未实现 | 当前无文件缓存。 |

## 路由与虚拟主机

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| `server` 块 | 部分实现 | 对应 `http-server`。 |
| `listen` | 部分实现 | 支持 host/port；不支持 backlog、reuseport、ssl 等细项。 |
| `server_name` | 部分实现 | 配置字段存在，但完整虚拟主机选择未实现。 |
| location 精确匹配 | 已实现 | `match.type=0`。 |
| location 前缀匹配 | 已实现 | `match.type=1`，最长前缀优先。 |
| location regex | 未实现 | `match.type=2` 规划中，当前未实现。 |
| named location | 未实现 | 当前无等价模型。 |
| route 顺序管理 | 差异设计 | 顺序由系统维护，API 不允许手动排序。 |

## Rewrite

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| `rewrite` 正则改写 | 未实现 | 当前没有正则 rewrite。 |
| proxy path 前缀替换 | 已实现 | `replacePrefix`。 |
| query string 保留 | 已实现 | `replacePrefix` 会保留 query string。 |
| return / error_page | 未实现 | 当前无等价配置。 |
| internal redirect | 未实现 | 当前无内部跳转。 |

## 反向代理

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| HTTP upstream proxy | 已实现 | 当前支持 `http://` upstream。 |
| HTTPS upstream proxy | 未实现 | 当前不支持 `https://` upstream。 |
| HTTP/2 入站转 HTTP/1.1 upstream | 已实现 | 入站 h2 请求可转发到 HTTP/1.1 upstream。 |
| HTTP/2 upstream | 未实现 | 当前 upstream 只走 HTTP/1.1。 |
| WebSocket proxy | 已实现 | 支持 upgrade 后双向转发。 |
| proxy request headers | 部分实现 | 添加 `Host`、`X-Real-IP`、`X-Forwarded-For`、`X-Forwarded-Proto`。 |
| proxy header 完整指令集 | 未实现 | 无 `proxy_set_header` 等通用配置。 |
| proxy response streaming | 已实现 | 收到响应头后立即转发，body 分块转发。 |
| `Content-Length` request body streaming | 已实现 | 可边读边写 upstream。 |
| chunked request body streaming | 未实现 | 当前先完整解码再转发。 |
| proxy buffering | 未实现 | 当前无 nginx 式 buffer 配置。 |
| proxy cache | 未实现 | 当前无缓存。 |
| proxy redirect / cookie rewrite | 未实现 | 当前无对应能力。 |
| proxy timeout | 部分实现 | 支持 connect/send/read timeout。 |
| upstream 失败切换 | 部分实现 | 连接失败会尝试后续候选；没有 nginx 完整 `proxy_next_upstream` 语义。 |

## Upstream 与负载均衡

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| upstream group | 已实现 | route 通过 upstream `group` 选择。 |
| round-robin | 已实现 | 相同 priority 内轮询。 |
| weight | 未实现 | 当前使用 priority，不是 nginx weight。 |
| least_conn / ip_hash / hash | 未实现 | 当前无其它算法。 |
| max_fails / fail_timeout | 未实现 | 当前无健康失败统计。 |
| active health check | 未实现 | 当前无主动健康检查。 |
| backup server | 未实现 | 当前无 backup 标记。 |
| keepalive upstream connection pool | 未实现 | 当前无 upstream 连接池。 |
| 蓝绿替换 | 差异设计 | 新增相同 `group + name` 替换旧 upstream，旧请求可自然结束，并暴露 `deading/dead`。 |
| upstream 状态查询 | 差异设计 | API 暴露 `status` 和 `activeRequestCount`。 |

## 日志

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| access log | 已实现 | JSON Lines。 |
| error log | 已实现 | JSON Lines。 |
| 管理操作日志 | 差异设计 | nginx 无同等内置管理 API；yiz-tunnel 有 admin log。 |
| 自定义 log_format | 未实现 | 当前格式固定。 |
| 日志级别配置 | 未实现 | 当前能力有限。 |
| 日志轮转 | 未实现 | 当前不负责轮转。 |
| reopen log | 未实现 | 当前没有 nginx 式信号 reopen。 |

## TCP / Stream

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| stream TCP proxy | 未实现 | 已明确先不进入当前最小可用。 |
| UDP proxy | 未实现 | 当前无计划内实现。 |
| TLS passthrough | 未实现 | 当前无 stream 模块。 |
| stream upstream load balancing | 未实现 | 当前无 stream 模块。 |

## 缓存、压缩与安全能力

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| gzip / brotli | 未实现 | 当前无压缩。 |
| request/response decompression | 未实现 | 当前无。 |
| proxy cache / fastcgi cache | 未实现 | 当前无缓存。 |
| rate limit | 未实现 | 当前无限流。 |
| connection limit | 未实现 | 当前无按 key 限制。 |
| auth basic / access allow-deny | 未实现 | 当前无访问控制。 |
| CORS 辅助配置 | 未实现 | 当前无专门配置。 |
| WAF / request filtering | 未实现 | 当前无。 |

## 模块与扩展

| nginx 能力 | yiz-tunnel 状态 | 说明 |
| --- | --- | --- |
| 静态/动态模块体系 | 未实现 | 当前没有模块插件系统。 |
| Lua/njs 等脚本扩展 | 未实现 | 当前无脚本扩展。 |
| 第三方模块生态 | 未实现 | 当前不兼容 nginx 模块。 |

## 当前最接近 nginx 的部分

- HTTP server / route / upstream 的概念映射。
- 静态文件 `root` / `alias` 基础语义。
- 精确匹配和最长前缀匹配。
- HTTP reverse proxy 基础路径。
- WebSocket upgrade 转发。
- keep-alive、请求体大小、超时等常用配置。
- `ETag`、`Last-Modified`、单段 `Range`。
- upstream 轮询和连接失败切换的基础行为。

## 当前与 nginx 最大的差距

- 没有 nginx 配置语法、include、上下文和完整指令体系。
- 没有 master/worker 多进程模型和信号控制。
- 没有 TLS / HTTPS / ALPN。
- HTTP/2 仍是最小可用路径，缺少完整 stream 状态机、流控、RST、priority。
- 静态文件缺少 `index`、`try_files`、autoindex、多段 range、sendfile/open_file_cache 等。
- proxy 缺少完整 header 改写、buffer、cache、redirect/cookie 改写、HTTPS upstream、HTTP/2 upstream。
- upstream 缺少 weight、least_conn、hash、健康检查、连接池。
- 没有 stream TCP/UDP proxy。
- 没有压缩、限流、访问控制、metrics、模块系统。

## 建议推进顺序

如果目标是“简单可用但更接近 nginx”，建议优先级如下：

1. 补齐 HTTP/2 基础协议正确性：RST_STREAM、WINDOW_UPDATE、SETTINGS 限制、stream 状态机。
2. 补静态文件常用行为：`index`、`try_files`、更多 MIME、多段 range。
3. 补 proxy 常用配置：可配置 request/response header、HTTPS upstream、chunked request body 流式转发。
4. 补 upstream 能力：weight、失败统计、健康检查、连接池。
5. 补 TLS：rustls listener、证书配置、ALPN HTTP/2。
6. 补管理面安全：鉴权、绑定限制、操作审计增强。
7. 再考虑 TCP stream、限流、压缩、metrics 和更完整的 nginx 行为兼容。
