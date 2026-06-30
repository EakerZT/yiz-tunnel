# yiz-tunnel 快速开始

本文档面向第一版简单可用验证，假设当前工作目录是项目根目录。

## 构建

```powershell
cargo build
```

构建产物：

```text
target\debug\yiz-tunnel.exe
```

## 启动

指定系统配置文件启动：

```powershell
target\debug\yiz-tunnel.exe -c .\yiz-tunnel.json
```

不传 `-c` 时，默认使用当前目录下的 `yiz-tunnel.json`。

如果系统配置文件不存在，程序会按默认值生成：

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

第一版管理 API 不鉴权。

## 目录结构

系统配置文件只保存系统级配置，例如数据目录和日志目录。

规则文件由管理 API 生成：

```text
data/http-server.json
```

日志文件：

```text
logs/admin.log
logs/access.log
logs/error.log
```

## 查看状态

```powershell
curl.exe http://127.0.0.1:9000/api/v1/system/status
```

成功响应统一为：

```json
{
  "code": 0,
  "message": "ok",
  "data": {}
}
```

## 创建静态文件服务

准备文件：

```powershell
New-Item -ItemType Directory -Force -Path .\public | Out-Null
Set-Content -Path .\public\hello.txt -Value "hello yiz-tunnel" -NoNewline
```

创建 HTTP 服务：

```powershell
curl.exe -X POST http://127.0.0.1:9000/api/v1/http-servers `
  -H "Content-Type: application/json" `
  -d "{\"alias\":\"static-demo\",\"listen\":{\"host\":\"127.0.0.1\",\"port\":18080,\"serverName\":[\"localhost\"]},\"conf\":{},\"graceful\":{\"enabled\":true,\"type\":0}}"
```

响应里的 `data.id` 是后续接口使用的 `http-server` ID，例如：

```text
hs_xxx
```

创建 file route，将下面的 `{serverId}` 替换成实际 ID：

```powershell
curl.exe -X POST http://127.0.0.1:9000/api/v1/http-server/{serverId}/routes `
  -H "Content-Type: application/json" `
  -d "{\"match\":{\"type\":1,\"path\":\"/\"},\"action\":{\"type\":\"file\",\"file\":{\"dir\":\".\u005c\u005cpublic\",\"alias\":0}},\"conf\":{}}"
```

访问业务端口：

```powershell
curl.exe http://127.0.0.1:18080/hello.txt
```

预期输出：

```text
hello yiz-tunnel
```

## 创建 Proxy 服务

先准备一个 upstream 服务，例如本机 `127.0.0.1:3000` 已经有 HTTP 服务。

新增 upstream：

```powershell
curl.exe -X POST http://127.0.0.1:9000/api/v1/http-server/{serverId}/upstreams `
  -H "Content-Type: application/json" `
  -d "{\"group\":\"api\",\"name\":\"v1\",\"host\":\"http://127.0.0.1:3000\",\"priority\":0,\"conf\":{}}"
```

新增 proxy route：

```powershell
curl.exe -X POST http://127.0.0.1:9000/api/v1/http-server/{serverId}/routes `
  -H "Content-Type: application/json" `
  -d "{\"match\":{\"type\":1,\"path\":\"/api/\"},\"action\":{\"type\":\"proxy\",\"proxy\":{\"upstream\":\"api\",\"websocket\":{\"enabled\":true}}},\"conf\":{}}"
```

访问：

```powershell
curl.exe http://127.0.0.1:18080/api/health
```

## 蓝绿替换 upstream

新增同 `group + name` 的 upstream 会触发替换。

例如旧 upstream：

```json
{
  "group": "api",
  "name": "v1",
  "host": "http://127.0.0.1:3000"
}
```

新增同 `group=api`、`name=v1`，但 host 改成 `127.0.0.1:3001`：

```powershell
curl.exe -X POST http://127.0.0.1:9000/api/v1/http-server/{serverId}/upstreams `
  -H "Content-Type: application/json" `
  -d "{\"group\":\"api\",\"name\":\"v1\",\"host\":\"http://127.0.0.1:3001\",\"priority\":0,\"conf\":{}}"
```

结果：

- 新请求只会选择新 upstream。
- 旧 upstream 如果还有活动请求，会在运行时状态里显示为 `deading`。
- 旧 upstream 活动请求归零后显示为 `dead`。

查看 upstream：

```powershell
curl.exe http://127.0.0.1:9000/api/v1/http-server/{serverId}/upstreams
```

运行时字段：

```json
{
  "status": "running",
  "activeRequestCount": 0
}
```

## 启停和删除 HTTP 服务

停止服务：

```powershell
curl.exe -X PUT http://127.0.0.1:9000/api/v1/http-server/{serverId}/enabled `
  -H "Content-Type: application/json" `
  -d "{\"enabled\":false}"
```

停止行为：

- listener 会停止接收新连接。
- 如果仍有已建立连接，服务状态会先进入 `stopping`。
- 已建立连接自然结束后，状态变为 `stopped`。

查看服务状态：

```powershell
curl.exe http://127.0.0.1:9000/api/v1/http-server/{serverId}/info
```

删除服务必须先停止：

```powershell
curl.exe -X DELETE http://127.0.0.1:9000/api/v1/http-server/{serverId}
```

重新启用：

```powershell
curl.exe -X PUT http://127.0.0.1:9000/api/v1/http-server/{serverId}/enabled `
  -H "Content-Type: application/json" `
  -d "{\"enabled\":true}"
```

## 配置项

`conf` 字段支持以下第一版配置。时间单位为毫秒，大小单位为字节。

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

字段缺失时使用默认值。

未知字段、非正整数或非对象结构会返回 `400`，不会落盘。

## 冒烟测试

项目包含管理 API 端到端冒烟脚本：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-management-api.ps1
```

脚本会启动独立临时实例，验证：

- `system/status`
- 创建 HTTP 服务
- 创建静态文件 route
- 访问业务端口
- 同 `group + name` upstream 替换
- 非法 `conf` 返回 `400`

## 第一版限制

第一版暂不包含：

- 管理 API 鉴权。
- HTTPS、HTTP/2、HTTP/3。
- `tcp-forward`。
- 完整 nginx 配置兼容。
- chunked 请求体流式转发。

当前已支持：

- HTTP/1.1 静态文件服务。
- HTTP proxy。
- WebSocket proxy。
- `Content-Length` 请求体流式 proxy。
- proxy 响应流式转发。
- upstream 轮询、连接失败切换、蓝绿替换状态。
- JSON Lines 日志。
