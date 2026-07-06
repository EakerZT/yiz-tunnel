# yiz-tunnel

[English](README.md)

`yiz-tunnel` 是一个使用 Rust 实现的 HTTP tunnel / reverse proxy 项目。它的目标是提供一个轻量、通过 API 管理的网关，用于本地服务代理、静态文件服务、upstream 路由和运行时配置变更。

项目使用 JSON 配置和管理 API，而不是 nginx 风格的文本配置语言。系统配置文件负责定义数据目录、日志目录和管理 API 监听地址；HTTP 服务规则通过管理 API 创建和更新。

## 项目用途

`yiz-tunnel` 可以运行一个或多个 HTTP 服务，每个 HTTP 服务可以拥有自己的监听地址、upstream 分组、route 和运行状态。

典型用途包括：

- 从本地目录提供静态文件服务。
- 将 HTTP 请求转发到本地或远程 upstream 服务。
- 转发 WebSocket upgrade 连接。
- 通过管理 API 管理 HTTP 服务、route 和 upstream 配置。
- 替换 upstream 目标，同时允许旧请求自然结束。
- 输出管理日志、访问日志和错误日志。

## 已实现功能

### 配置

- 支持通过 `-c <path>` 指定系统配置文件。
- 系统配置文件不存在时自动生成默认 `yiz-tunnel.json`。
- 系统配置支持数据目录、日志目录和管理 API 监听地址。
- HTTP 规则独立持久化到配置的数据目录下。
- HTTP 规则事务式写入。
- 启动时校验已持久化配置。

### 管理 API

- 版本化 API 前缀：`/api/v1`。
- 统一响应结构：`{ "code": number, "message": string, "data": ... }`。
- 系统状态接口。
- HTTP 服务新增、列表、查看、更新、启停、删除、reload 和运行状态接口。
- upstream 新增、列表、查看和删除接口。
- route 新增、列表、查看和删除接口。
- 支持 `conf` 字段和 route action 结构校验。

### HTTP 运行时

- 当前能力范围内的 HTTP/1.1 请求解析。
- 自实现 HTTP/2 cleartext prior-knowledge 入站连接支持。
- 最小 HTTP/2 frame 处理，支持 SETTINGS、HEADERS、DATA、PING、CONTINUATION 和 GOAWAY。
- 基础 HPACK 支持，包括静态表查询、动态表索引和 Huffman 解码。
- 多 HTTP 服务运行时。
- 多个 HTTP 服务可以共享同一监听地址，并按固定 `serverName` / `Host` 匹配选择服务，未命中时使用第一个服务作为 default server。
- 单个 HTTP 服务的运行时应用和 reload。
- 服务状态：`starting`、`running`、`stopping`、`stopped`、`failed`。
- keep-alive 处理。
- 参考 nginx 常用配置项的请求大小和超时配置。

### 静态文件

- full 和 prefix route 匹配。
- `root` 和 `alias` 风格文件路径行为。
- 路径穿越防护。
- 基础 MIME 判断。
- `ETag`。
- `Last-Modified`。
- `If-None-Match`。
- `If-Modified-Since`。
- 单段字节 `Range` 请求。

### 反向代理

- 转发到 `http://` upstream。
- HTTP/2 入站请求可以转发到 HTTP/1.1 upstream。
- WebSocket upgrade 转发。
- proxy header：`Host`、`X-Real-IP`、`X-Forwarded-For`、`X-Forwarded-Proto`。
- upstream 响应流式转发。
- `Content-Length` 请求体流式转发。
- chunked 请求体解码后转发。
- upstream 连接、发送和读取超时。
- upstream 连接失败后尝试后续候选。
- 支持 `replacePrefix` path rewrite。

### Upstream 路由

- route 通过 upstream group 选择目标。
- 按 priority 选择 upstream。
- 相同 priority 的 upstream 轮询。
- 新增相同 `group + name` 触发蓝绿替换。
- 旧 upstream 仍有活动请求时状态为 `deading`。
- 旧 upstream 活动请求归零后状态为 `dead`。

### 日志

- 管理日志。
- 访问日志。
- 错误日志。
- JSON Lines 日志格式。

### 测试和脚本

- 单元测试覆盖 route 匹配、静态文件、proxy、keep-alive、range/cache、chunked 请求体、upstream 替换、graceful stop 和配置校验。
- 管理 API 端到端冒烟测试脚本。

## 未实现功能

- TLS / HTTPS。
- 基于 TLS + ALPN 的 HTTP/2。
- HTTP/1.1 `Upgrade: h2c`。
- 完整 HTTP/2 stream 状态机、流控、priority 和 reset 处理。
- HTTP/3。
- TCP 转发。
- 管理 API 鉴权和授权。
- 完整 nginx 配置语法兼容。
- regex route。
- `replacePrefix` 之外的高级 rewrite 模式。
- chunked 请求体直接流式转发到 upstream。
- 静态文件 `index` 和 `try_files` 行为。
- `serverName` 通配符和正则匹配。
- priority 和 round-robin 之外的负载均衡算法。
- upstream 健康检查。
- 请求/响应压缩。
- 限流。
- metrics 指标接口。

## 构建

```powershell
cargo build
```

构建产物：

```text
target\debug\yiz-tunnel.exe
```

## 运行

指定系统配置文件启动：

```powershell
target\debug\yiz-tunnel.exe -c .\yiz-tunnel.json
```

不传 `-c` 时，默认使用当前目录下的 `yiz-tunnel.json`。

如果配置文件不存在，程序会按默认值创建：

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

默认管理 API：

```text
http://127.0.0.1:9000/api/v1
```

查看系统状态：

```powershell
curl.exe http://127.0.0.1:9000/api/v1/system/status
```

## 文档

- 快速开始：[docs/GETTING_STARTED.md](docs/GETTING_STARTED.md)
- 管理 API：[中文](docs/MANAGEMENT_API.md) / [English](docs/MANAGEMENT_API.en.md)
- nginx 能力对比：[docs/NGINX_COMPARISON.md](docs/NGINX_COMPARISON.md)
- 设计和进度记录：[plans/PROJECT_CONTINUATION.md](plans/PROJECT_CONTINUATION.md)

## 验证

运行单元测试：

```powershell
cargo test
```

运行管理 API 冒烟测试：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-management-api.ps1
```

## GitHub Actions

仓库包含：

- `.github/workflows/build.yml`：在 push 和 pull request 时运行格式检查、测试和 release 模式构建。
- `.github/workflows/release.yml`：推送 `v0.1.0` 这类 tag 时，构建各平台产物并创建 GitHub Release。

创建 release：

```powershell
git tag v0.1.0
git push origin v0.1.0
```
