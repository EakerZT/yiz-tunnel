use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub code: u32,
    pub message: String,
    pub data: T,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpServerFile {
    pub version: u32,
    pub items: Vec<HttpServerConfig>,
}

impl Default for HttpServerFile {
    fn default() -> Self {
        Self {
            version: 1,
            items: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpServerConfig {
    pub id: String,
    #[serde(default)]
    pub alias: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub listen: ListenConfig,
    #[serde(default)]
    pub graceful: GracefulConfig,
    #[serde(default = "empty_object")]
    pub conf: Value,
    #[serde(default)]
    pub upstreams: Vec<UpstreamConfig>,
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateHttpServerRequest {
    #[serde(default)]
    pub alias: String,
    pub listen: ListenConfig,
    #[serde(default = "empty_object")]
    pub conf: Value,
    #[serde(default)]
    pub graceful: GracefulConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHttpServerRequest {
    #[serde(default)]
    pub alias: String,
    pub listen: ListenConfig,
    #[serde(default = "empty_object")]
    pub conf: Value,
    #[serde(default)]
    pub graceful: GracefulConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetEnabledRequest {
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub server_name: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GracefulConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub r#type: u8,
}

impl Default for GracefulConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            r#type: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpstreamConfig {
    pub id: String,
    pub group: String,
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "empty_object")]
    pub conf: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamInfo {
    pub id: String,
    pub group: String,
    pub name: String,
    pub host: String,
    pub priority: i32,
    pub conf: Value,
    pub status: String,
    pub active_request_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateUpstreamRequest {
    pub group: String,
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "empty_object")]
    pub conf: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RouteConfig {
    pub id: String,
    #[serde(rename = "match")]
    pub match_rule: RouteMatch,
    pub action: RouteAction,
    #[serde(default = "empty_object")]
    pub conf: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateRouteRequest {
    #[serde(rename = "match")]
    pub match_rule: RouteMatch,
    pub action: RouteAction,
    #[serde(default = "empty_object")]
    pub conf: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RouteMatch {
    pub r#type: u8,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RouteAction {
    pub r#type: String,
    #[serde(default)]
    pub file: Option<FileAction>,
    #[serde(default)]
    pub proxy: Option<ProxyAction>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileAction {
    pub dir: String,
    #[serde(default)]
    pub alias: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProxyAction {
    pub upstream: String,
    #[serde(default)]
    pub websocket: WebSocketConfig,
    #[serde(default)]
    pub rewrite: Option<ProxyRewrite>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProxyRewrite {
    pub r#type: String,
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WebSocketConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpServerInfo {
    pub id: String,
    pub alias: String,
    pub enabled: bool,
    pub status: String,
    pub active_connection_count: usize,
    pub active_request_count: usize,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub version: String,
    pub uptime: u64,
    pub system_config_path: String,
    pub data_dir: String,
    pub log_dir: String,
    pub http_server_count: usize,
    pub tcp_forward_count: usize,
    pub active_connection_count: usize,
    pub last_error: Option<String>,
}

pub fn default_true() -> bool {
    true
}

pub fn empty_object() -> Value {
    Value::Object(Default::default())
}
