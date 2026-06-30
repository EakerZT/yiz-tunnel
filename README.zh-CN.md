# yiz-tunnel

[English](README.md)

`yiz-tunnel` 是一个使用 Rust 实现的 HTTP tunnel / reverse proxy 项目，目标是先完成 nginx 常用 HTTP 能力的一个简单可用子集。

当前第一版聚焦 HTTP：

- HTTP/1.1 静态文件服务。
- HTTP reverse proxy。
- WebSocket proxy。
- 管理 API。
- JSON 配置持久化。
- 日志输出。
- HTTP 服务热更新。
- upstream 轮询、失败切换和蓝绿替换状态。

第一版暂不包含：

- HTTPS / HTTP/2 / HTTP/3。
- 管理 API 鉴权。
- `tcp-forward`。
- 完整 nginx 配置语法兼容。

## 文档

- 快速开始：[docs/GETTING_STARTED.md](docs/GETTING_STARTED.md)
- 管理 API：[docs/MANAGEMENT_API.md](docs/MANAGEMENT_API.md)
- 设计和进度：[plans/PROJECT_CONTINUATION.md](plans/PROJECT_CONTINUATION.md)

## 构建

```powershell
cargo build
```

构建产物：

```text
target\debug\yiz-tunnel.exe
```

## 启动

指定系统配置文件：

```powershell
target\debug\yiz-tunnel.exe -c .\yiz-tunnel.json
```

不传 `-c` 时，默认使用当前目录下的 `yiz-tunnel.json`。

如果配置文件不存在，程序会生成默认系统配置：

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

查看状态：

```powershell
curl.exe http://127.0.0.1:9000/api/v1/system/status
```

## 配置和数据

系统配置文件只保存系统级配置，例如数据目录、日志目录和管理服务监听地址。

HTTP 规则由管理 API 写入：

```text
data\http-server.json
```

日志文件：

```text
logs\admin.log
logs\access.log
logs\error.log
```

## 已支持能力

### Static File

- prefix / full route 匹配。
- `root` / `alias` 风格文件路径。
- 路径穿越防护。
- 基础 MIME。
- `ETag`。
- `Last-Modified`。
- 单段 `Range`。

### Proxy

- HTTP reverse proxy。
- proxy path rewrite，当前支持 `replacePrefix`。
- WebSocket upgrade 转发。
- `Host`、`X-Real-IP`、`X-Forwarded-For`、`X-Forwarded-Proto`。
- proxy 响应流式转发。
- `Content-Length` 请求体流式转发。
- chunked 请求体解码后转发。
- upstream 连接失败后尝试后续候选。

### Upstream

- 按 `priority` 从小到大选择。
- 同 `priority` 轮询。
- 同 `group + name` 新增触发蓝绿替换。
- 旧 upstream 有活动请求时保留为 `deading`。
- 活动请求归零后变为 `dead`。

### Runtime

- HTTP 服务启停。
- 停止时关闭 listener，不再接收新连接。
- 有活动连接时状态为 `stopping`。
- 活动连接归零后状态为 `stopped`。
- 管理 API 修改配置后局部应用运行时配置。

## 自动验证

单元测试：

```powershell
cargo test
```

当前测试覆盖：

- route 匹配。
- 静态文件响应。
- proxy 转发。
- WebSocket 基础转发路径。
- proxy header。
- proxy path rewrite。
- keep-alive。
- cache / range。
- chunked 请求体。
- `conf` 校验和运行时生效。
- upstream 轮询、失败切换、蓝绿替换状态。
- proxy 请求体和响应流式转发。
- http-server graceful stop。

管理 API 端到端冒烟测试：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-management-api.ps1
```

冒烟脚本会启动独立临时实例，验证：

- `system/status`。
- 创建 HTTP 服务。
- 创建静态文件 route。
- 访问业务端口。
- 同 `group + name` upstream 替换。
- 非法 `conf` 返回 `400`。

## 第一版限制

- 管理 API 暂无鉴权，只建议绑定本机或可信内网。
- `serverName` 当前不是完整虚拟主机匹配能力。
- chunked 请求体暂未流式转发。
- 静态文件未实现 index / try_files。
- 尚未实现 TLS。
- 尚未实现 tcp-forward。

## 开发约定

- 核心 HTTP runtime 尽量少引入外部依赖。
- 管理 API 使用 `axum`。
- 异步运行时使用 `tokio`。
- JSON 使用 `serde_json`。
- TLS 后续计划使用 `rustls`。
