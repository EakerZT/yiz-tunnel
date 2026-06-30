# HTTP MVP 范围

## 定位

`tcp-forward` 第一阶段先不进入最小可用实现。

当前最小可用版本聚焦 HTTP 能力：

- 系统配置加载。
- 管理 API。
- `http-server` 管理。
- route 管理。
- upstream 管理。
- 静态文件响应。
- HTTP 反向代理。
- WebSocket 代理。
- 系统状态查询。
- 日志输出。

## HTTP 协议范围

第一版只支持：

- HTTP/1.1。
- 常见 method。
- path。
- query。
- headers。
- body。
- keep-alive。

第一版不支持：

- HTTPS。
- HTTP/2。
- HTTP/3。

`ssl` 和 `http2` 配置第一阶段不实现。

## Proxy 行为

第一版 proxy 只做反向代理。

除本项目已明确要求的行为外，其它 proxy 基础行为参考 nginx。

已确认：

- upstream 连接失败返回 `502`。
- upstream 超时返回 `504`。
- 多 upstream 同优先级选择先使用轮询。
- WebSocket 需要实现。
- WebSocket 默认开启。

## 静态文件行为

第一版静态文件行为：

- 文件不存在返回 `404`。
- 目录访问返回 `403`。
- 权限不足返回 `403`。
- MIME type 先内置少量常见类型，不依赖库。
- 不支持 Range。
- 不支持 ETag。
- 不支持 Last-Modified。
- 不支持 index 文件，路径必须匹配具体文件。

除本项目已明确要求的行为外，其它静态文件基础行为参考 nginx。

## Route 匹配

第一版 route 匹配：

- 支持 `full`。
- 支持 `prefix`。
- 不支持 `regex`。
- `full` 优先于 `prefix`。
- `prefix` 选择最长匹配。
- 同一个 `http-server` 下禁止创建相同 `match.type + match.path` 的 route。
- route 顺序由系统内部维护，第一版匹配不依赖用户排序。

## 日志

第一版日志文件：

```text
logs/access.log
logs/error.log
logs/admin.log
```

### access.log

字段：

- request time。
- remote address。
- http-server id。
- http-server alias。
- method。
- path。
- status。
- response time。
- upstream id，如果有。
- upstream name，如果有。

### error.log

字段：

- time。
- level。
- module。
- message。
- error detail。

### admin.log

字段：

- time。
- operation。
- target type。
- target id。
- result。
- message。

日志格式后续实现时可先采用 JSON Lines，便于程序解析和排查。

## JSON 示例

### yiz-tunnel.json

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

### data/http-server.json

```json
{
  "version": 1,
  "items": [
    {
      "id": "hs_example",
      "alias": "local-api",
      "enabled": true,
      "listen": {
        "host": "0.0.0.0",
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
      "upstreams": [
        {
          "id": "up_example",
          "group": "api",
          "name": "v1",
          "host": "http://127.0.0.1:3000",
          "priority": 0,
          "conf": {}
        }
      ],
      "routes": [
        {
          "id": "rt_file",
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
        },
        {
          "id": "rt_proxy",
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
        }
      ]
    }
  ]
}
```

`id` 由系统生成，示例中的 `id` 仅用于展示结构。

