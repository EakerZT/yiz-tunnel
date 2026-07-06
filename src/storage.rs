use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;
use uuid::Uuid;

use crate::error::ApiError;
use crate::model::{
    CreateHttpServerRequest, CreateRouteRequest, CreateUpstreamRequest, HttpServerConfig,
    HttpServerFile, ListenConfig, ProxyRewrite, RouteConfig, SetEnabledRequest,
    UpdateHttpServerRequest, UpstreamConfig,
};

pub struct HttpServerStorage {
    path: PathBuf,
    data: Mutex<HttpServerFile>,
}

impl HttpServerStorage {
    pub fn load_or_empty(path: PathBuf) -> std::io::Result<Self> {
        let data = if path.exists() {
            let content = fs::read_to_string(&path)?;
            let data = serde_json::from_str(&content)?;
            validate_loaded_file(&data)?;
            data
        } else {
            HttpServerFile::default()
        };

        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }

    pub fn list(&self) -> Result<Vec<HttpServerConfig>, ApiError> {
        Ok(self.lock()?.items.clone())
    }

    pub fn count(&self) -> Result<usize, ApiError> {
        Ok(self.lock()?.items.len())
    }

    pub fn get(&self, id: &str) -> Result<HttpServerConfig, ApiError> {
        self.lock()?
            .items
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| ApiError::not_found(format!("http-server not found: {id}")))
    }

    pub fn create(&self, request: CreateHttpServerRequest) -> Result<HttpServerConfig, ApiError> {
        validate_listen(&request.listen)?;
        validate_graceful_type(request.graceful.r#type)?;
        validate_conf(&request.conf)?;

        let mut data = self.lock()?;
        let item = HttpServerConfig {
            id: new_id("hs"),
            alias: request.alias,
            enabled: true,
            listen: request.listen,
            graceful: request.graceful,
            conf: request.conf,
            upstreams: Vec::new(),
            routes: Vec::new(),
        };

        data.items.push(item.clone());
        self.save(&data)?;
        Ok(item)
    }

    pub fn update(
        &self,
        id: &str,
        request: UpdateHttpServerRequest,
    ) -> Result<HttpServerConfig, ApiError> {
        validate_listen(&request.listen)?;
        validate_graceful_type(request.graceful.r#type)?;
        validate_conf(&request.conf)?;

        let mut data = self.lock()?;
        let item = find_http_server_mut(&mut data, id)?;
        item.alias = request.alias;
        item.listen = request.listen;
        item.conf = request.conf;
        item.graceful = request.graceful;
        let result = item.clone();
        self.save(&data)?;
        Ok(result)
    }

    pub fn set_enabled(
        &self,
        id: &str,
        request: SetEnabledRequest,
    ) -> Result<HttpServerConfig, ApiError> {
        let mut data = self.lock()?;
        let item = find_http_server_mut(&mut data, id)?;
        item.enabled = request.enabled;
        let result = item.clone();
        self.save(&data)?;
        Ok(result)
    }

    pub fn delete(&self, id: &str) -> Result<HttpServerConfig, ApiError> {
        let mut data = self.lock()?;
        let index = data
            .items
            .iter()
            .position(|item| item.id == id)
            .ok_or_else(|| ApiError::not_found(format!("http-server not found: {id}")))?;

        if data.items[index].enabled {
            return Err(ApiError::conflict(
                30002,
                "http-server must be disabled before delete",
            ));
        }

        let removed = data.items.remove(index);
        self.save(&data)?;
        Ok(removed)
    }

    pub fn list_upstreams(&self, id: &str) -> Result<Vec<UpstreamConfig>, ApiError> {
        Ok(self.get(id)?.upstreams)
    }

    pub fn get_upstream(&self, id: &str, upstream_id: &str) -> Result<UpstreamConfig, ApiError> {
        self.get(id)?
            .upstreams
            .into_iter()
            .find(|item| item.id == upstream_id)
            .ok_or_else(|| ApiError::not_found(format!("upstream not found: {upstream_id}")))
    }

    pub fn add_upstream(
        &self,
        id: &str,
        request: CreateUpstreamRequest,
    ) -> Result<UpstreamConfig, ApiError> {
        validate_upstream_request(&request)?;
        validate_conf(&request.conf)?;

        let mut data = self.lock()?;
        let server = find_http_server_mut(&mut data, id)?;

        if !server.graceful.enabled
            && server
                .upstreams
                .iter()
                .any(|upstream| upstream.group == request.group && upstream.name == request.name)
        {
            return Err(ApiError::conflict(
                40003,
                "same group and name upstream replacement requires graceful.enabled=true",
            ));
        }

        server
            .upstreams
            .retain(|upstream| upstream.group != request.group || upstream.name != request.name);

        let upstream = UpstreamConfig {
            id: new_id("up"),
            group: request.group,
            name: request.name,
            host: request.host,
            priority: request.priority,
            conf: request.conf,
        };

        server.upstreams.push(upstream.clone());
        self.save(&data)?;
        Ok(upstream)
    }

    pub fn delete_upstream(&self, id: &str, upstream_id: &str) -> Result<UpstreamConfig, ApiError> {
        let mut data = self.lock()?;
        let server = find_http_server_mut(&mut data, id)?;
        let index = server
            .upstreams
            .iter()
            .position(|item| item.id == upstream_id)
            .ok_or_else(|| ApiError::not_found(format!("upstream not found: {upstream_id}")))?;
        let removed = server.upstreams.remove(index);
        self.save(&data)?;
        Ok(removed)
    }

    pub fn list_routes(&self, id: &str) -> Result<Vec<RouteConfig>, ApiError> {
        Ok(self.get(id)?.routes)
    }

    pub fn get_route(&self, id: &str, route_id: &str) -> Result<RouteConfig, ApiError> {
        self.get(id)?
            .routes
            .into_iter()
            .find(|item| item.id == route_id)
            .ok_or_else(|| ApiError::not_found(format!("route not found: {route_id}")))
    }

    pub fn add_route(
        &self,
        id: &str,
        request: CreateRouteRequest,
    ) -> Result<RouteConfig, ApiError> {
        validate_route(&request)?;
        validate_conf(&request.conf)?;

        let mut data = self.lock()?;
        let server = find_http_server_mut(&mut data, id)?;
        validate_route_upstream_target(server, &request)?;

        if server.routes.iter().any(|route| {
            route.match_rule.r#type == request.match_rule.r#type
                && route.match_rule.path == request.match_rule.path
        }) {
            return Err(ApiError::conflict(
                10003,
                "route with same match.type and match.path already exists",
            ));
        }

        let route = RouteConfig {
            id: new_id("rt"),
            match_rule: request.match_rule,
            action: request.action,
            conf: request.conf,
        };

        server.routes.push(route.clone());
        self.save(&data)?;
        Ok(route)
    }

    pub fn delete_route(&self, id: &str, route_id: &str) -> Result<RouteConfig, ApiError> {
        let mut data = self.lock()?;
        let server = find_http_server_mut(&mut data, id)?;
        let index = server
            .routes
            .iter()
            .position(|item| item.id == route_id)
            .ok_or_else(|| ApiError::not_found(format!("route not found: {route_id}")))?;
        let removed = server.routes.remove(index);
        self.save(&data)?;
        Ok(removed)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HttpServerFile>, ApiError> {
        self.data
            .lock()
            .map_err(|_| ApiError::internal("http-server storage lock poisoned"))
    }

    fn save(&self, data: &HttpServerFile) -> Result<(), ApiError> {
        save_transaction(&self.path, data)
            .map_err(|err| ApiError::internal(format!("failed to save http-server file: {err}")))
    }
}

fn validate_conf(value: &Value) -> Result<(), ApiError> {
    let Some(object) = value.as_object() else {
        return Err(ApiError::invalid_request("conf must be an object"));
    };

    for (name, value) in object {
        if !is_supported_conf_name(name) {
            return Err(ApiError::invalid_request(format!(
                "unsupported conf field: {name}"
            )));
        }

        let Some(number) = value.as_u64() else {
            return Err(ApiError::invalid_request(format!(
                "conf.{name} must be a positive integer"
            )));
        };
        if number == 0 {
            return Err(ApiError::invalid_request(format!(
                "conf.{name} must be a positive integer"
            )));
        }
    }

    Ok(())
}

fn validate_listen(listen: &ListenConfig) -> Result<(), ApiError> {
    if listen.host.trim().is_empty() {
        return Err(ApiError::invalid_request("listen.host must not be empty"));
    }

    if listen.port == 0 {
        return Err(ApiError::invalid_request("listen.port must not be 0"));
    }

    if listen.server_name.iter().any(|name| name.trim().is_empty()) {
        return Err(ApiError::invalid_request(
            "listen.serverName must not contain empty names",
        ));
    }

    Ok(())
}

fn validate_graceful_type(value: u8) -> Result<(), ApiError> {
    if value != 0 {
        return Err(ApiError::invalid_request("graceful.type only supports 0"));
    }

    Ok(())
}

fn validate_upstream_request(request: &CreateUpstreamRequest) -> Result<(), ApiError> {
    validate_upstream_fields(&request.group, &request.name, &request.host)
}

fn validate_upstream_config(upstream: &UpstreamConfig) -> Result<(), ApiError> {
    if upstream.id.trim().is_empty() {
        return Err(ApiError::invalid_request("upstream.id must not be empty"));
    }

    validate_upstream_fields(&upstream.group, &upstream.name, &upstream.host)
}

fn validate_upstream_fields(group: &str, name: &str, host: &str) -> Result<(), ApiError> {
    if group.trim().is_empty() {
        return Err(ApiError::invalid_request(
            "upstream.group must not be empty",
        ));
    }

    if name.trim().is_empty() {
        return Err(ApiError::invalid_request("upstream.name must not be empty"));
    }

    if parse_http_upstream(host).is_none() {
        return Err(ApiError::invalid_request(
            "upstream.host must be an http:// host with a valid port",
        ));
    }

    Ok(())
}

fn parse_http_upstream(value: &str) -> Option<(String, u16)> {
    let rest = value.strip_prefix("http://")?;
    if rest.is_empty() || rest.starts_with('/') {
        return None;
    }

    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.contains('@') || authority.trim().is_empty() {
        return None;
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().ok()?),
        None => (authority.to_string(), 80),
    };

    if host.trim().is_empty() || port == 0 {
        None
    } else {
        Some((host, port))
    }
}

fn is_supported_conf_name(name: &str) -> bool {
    matches!(
        name,
        "client_max_body_size"
            | "client_header_timeout"
            | "client_body_timeout"
            | "send_timeout"
            | "keepalive_timeout"
            | "keepalive_requests"
            | "proxy_connect_timeout"
            | "proxy_send_timeout"
            | "proxy_read_timeout"
    )
}

fn validate_loaded_file(data: &HttpServerFile) -> std::io::Result<()> {
    if data.version != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported http-server file version: {}", data.version),
        ));
    }

    let mut server_ids = HashSet::new();
    for server in &data.items {
        validate_server_config(server).map_err(api_error_to_io)?;
        if !server_ids.insert(server.id.as_str()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("duplicate http-server id: {}", server.id),
            ));
        }

        validate_conf(&server.conf).map_err(api_error_to_io)?;

        let mut upstream_ids = HashSet::new();
        for upstream in &server.upstreams {
            validate_upstream_config(upstream).map_err(api_error_to_io)?;
            if !upstream_ids.insert(upstream.id.as_str()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("duplicate upstream id: {}", upstream.id),
                ));
            }
            validate_conf(&upstream.conf).map_err(api_error_to_io)?;
        }

        let mut route_ids = HashSet::new();
        let mut route_matches = HashSet::new();
        for route in &server.routes {
            validate_route_config(route).map_err(api_error_to_io)?;
            if !route_ids.insert(route.id.as_str()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("duplicate route id: {}", route.id),
                ));
            }
            if !route_matches.insert((route.match_rule.r#type, route.match_rule.path.as_str())) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "duplicate route match.type and match.path: {} {}",
                        route.match_rule.r#type, route.match_rule.path
                    ),
                ));
            }
            validate_conf(&route.conf).map_err(api_error_to_io)?;
        }
    }

    Ok(())
}

fn validate_server_config(server: &HttpServerConfig) -> Result<(), ApiError> {
    if server.id.trim().is_empty() {
        return Err(ApiError::invalid_request(
            "http-server.id must not be empty",
        ));
    }

    validate_listen(&server.listen)?;
    validate_graceful_type(server.graceful.r#type)
}

fn api_error_to_io(error: ApiError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error.message)
}

fn find_http_server_mut<'a>(
    data: &'a mut HttpServerFile,
    id: &str,
) -> Result<&'a mut HttpServerConfig, ApiError> {
    data.items
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| ApiError::not_found(format!("http-server not found: {id}")))
}

fn validate_route_config(route: &RouteConfig) -> Result<(), ApiError> {
    if route.id.trim().is_empty() {
        return Err(ApiError::invalid_request("route.id must not be empty"));
    }

    if route.match_rule.r#type > 1 {
        return Err(ApiError::invalid_request(
            "route match.type only supports 0(full) and 1(prefix)",
        ));
    }

    if route.match_rule.path.is_empty() || !route.match_rule.path.starts_with('/') {
        return Err(ApiError::invalid_request(
            "route match.path must start with /",
        ));
    }

    match route.action.r#type.as_str() {
        "file" if route.action.file.is_some() => {
            validate_file_action(route.action.file.as_ref().unwrap())
        }
        "file" => Err(ApiError::invalid_request(
            "action.file is required when action.type is file",
        )),
        "proxy" if route.action.proxy.is_some() => {
            let proxy = route.action.proxy.as_ref().unwrap();
            validate_proxy_action(proxy.upstream.as_str(), proxy.rewrite.as_ref())
        }
        "proxy" => Err(ApiError::invalid_request(
            "action.proxy is required when action.type is proxy",
        )),
        _ => Err(ApiError::invalid_request(
            "action.type only supports file and proxy",
        )),
    }
}

fn validate_route(request: &CreateRouteRequest) -> Result<(), ApiError> {
    if request.match_rule.r#type > 1 {
        return Err(ApiError::invalid_request(
            "route match.type only supports 0(full) and 1(prefix)",
        ));
    }

    if request.match_rule.path.is_empty() || !request.match_rule.path.starts_with('/') {
        return Err(ApiError::invalid_request(
            "route match.path must start with /",
        ));
    }

    match request.action.r#type.as_str() {
        "file" => {
            let Some(file) = request.action.file.as_ref() else {
                return Err(ApiError::invalid_request(
                    "action.file is required when action.type is file",
                ));
            };
            validate_file_action(file)?;
        }
        "proxy" => {
            let Some(proxy) = request.action.proxy.as_ref() else {
                return Err(ApiError::invalid_request(
                    "action.proxy is required when action.type is proxy",
                ));
            };
            validate_proxy_action(proxy.upstream.as_str(), proxy.rewrite.as_ref())?;
        }
        _ => {
            return Err(ApiError::invalid_request(
                "action.type only supports file and proxy",
            ));
        }
    }

    Ok(())
}

fn validate_route_upstream_target(
    server: &HttpServerConfig,
    request: &CreateRouteRequest,
) -> Result<(), ApiError> {
    if request.action.r#type != "proxy" {
        return Ok(());
    }

    let upstream_group = request
        .action
        .proxy
        .as_ref()
        .map(|proxy| proxy.upstream.as_str())
        .unwrap_or_default();

    if server
        .upstreams
        .iter()
        .any(|upstream| upstream.group == upstream_group)
    {
        Ok(())
    } else {
        Err(ApiError::invalid_request(format!(
            "route proxy.upstream group does not exist: {upstream_group}"
        )))
    }
}

fn validate_file_action(file: &crate::model::FileAction) -> Result<(), ApiError> {
    if file.dir.trim().is_empty() {
        return Err(ApiError::invalid_request(
            "action.file.dir must not be empty",
        ));
    }

    if file.alias > 1 {
        return Err(ApiError::invalid_request(
            "action.file.alias only supports 0 and 1",
        ));
    }

    Ok(())
}

fn validate_proxy_action(upstream: &str, rewrite: Option<&ProxyRewrite>) -> Result<(), ApiError> {
    if upstream.trim().is_empty() {
        return Err(ApiError::invalid_request(
            "action.proxy.upstream must not be empty",
        ));
    }

    validate_proxy_rewrite(rewrite)
}

fn validate_proxy_rewrite(rewrite: Option<&ProxyRewrite>) -> Result<(), ApiError> {
    let Some(rewrite) = rewrite else {
        return Ok(());
    };

    if rewrite.r#type != "replacePrefix" {
        return Err(ApiError::invalid_request(
            "action.proxy.rewrite.type only supports replacePrefix",
        ));
    }

    if rewrite.from.is_empty() || !rewrite.from.starts_with('/') {
        return Err(ApiError::invalid_request(
            "action.proxy.rewrite.from must start with /",
        ));
    }

    if rewrite.to.is_empty() || !rewrite.to.starts_with('/') {
        return Err(ApiError::invalid_request(
            "action.proxy.rewrite.to must start with /",
        ));
    }

    Ok(())
}

fn save_transaction(path: &Path, data: &HttpServerFile) -> std::io::Result<()> {
    validate_loaded_file(data)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let bak_path = path.with_extension("json.bak");
    let content = serde_json::to_string_pretty(data)?;

    fs::write(&tmp_path, content)?;
    let tmp_content = fs::read_to_string(&tmp_path)?;
    let _: HttpServerFile = serde_json::from_str(&tmp_content)?;

    if path.exists() {
        let _ = fs::copy(path, bak_path)?;
    }

    fs::rename(tmp_path, path)?;
    Ok(())
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::now_v7().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        FileAction, GracefulConfig, ListenConfig, ProxyAction, RouteAction, RouteMatch,
        WebSocketConfig,
    };
    use serde_json::json;

    #[test]
    fn validate_conf_accepts_supported_positive_integer_fields() {
        let conf = json!({
            "client_max_body_size": 1048576,
            "keepalive_requests": 1000,
            "proxy_read_timeout": 60000
        });

        assert!(validate_conf(&conf).is_ok());
    }

    #[test]
    fn validate_conf_rejects_unknown_or_invalid_fields() {
        assert!(validate_conf(&json!({"unknown": 1})).is_err());
        assert!(validate_conf(&json!({"send_timeout": "1000"})).is_err());
        assert!(validate_conf(&json!({"keepalive_requests": 0})).is_err());
        assert!(validate_conf(&json!(null)).is_err());
    }

    #[test]
    fn add_upstream_replaces_same_group_and_name() {
        let (storage, path) = test_storage(true);

        let first = storage
            .add_upstream(
                "hs_test",
                CreateUpstreamRequest {
                    group: "api".to_string(),
                    name: "v1".to_string(),
                    host: "http://127.0.0.1:3000".to_string(),
                    priority: 0,
                    conf: json!({}),
                },
            )
            .unwrap();
        let second = storage
            .add_upstream(
                "hs_test",
                CreateUpstreamRequest {
                    group: "api".to_string(),
                    name: "v1".to_string(),
                    host: "http://127.0.0.1:3001".to_string(),
                    priority: 0,
                    conf: json!({}),
                },
            )
            .unwrap();

        let upstreams = storage.list_upstreams("hs_test").unwrap();
        assert_eq!(upstreams.len(), 1);
        assert_eq!(upstreams[0].id, second.id);
        assert_ne!(upstreams[0].id, first.id);
        assert_eq!(upstreams[0].host, "http://127.0.0.1:3001");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn add_upstream_rejects_invalid_host() {
        let (storage, path) = test_storage(true);

        let result = storage.add_upstream(
            "hs_test",
            CreateUpstreamRequest {
                group: "api".to_string(),
                name: "v1".to_string(),
                host: "https://127.0.0.1:3000".to_string(),
                priority: 0,
                conf: json!({}),
            },
        );

        assert!(result.is_err());
        assert!(storage.list_upstreams("hs_test").unwrap().is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn add_upstream_rejects_replacement_when_graceful_is_disabled() {
        let (storage, path) = test_storage(false);

        storage
            .add_upstream(
                "hs_test",
                CreateUpstreamRequest {
                    group: "api".to_string(),
                    name: "v1".to_string(),
                    host: "http://127.0.0.1:3000".to_string(),
                    priority: 0,
                    conf: json!({}),
                },
            )
            .unwrap();

        let result = storage.add_upstream(
            "hs_test",
            CreateUpstreamRequest {
                group: "api".to_string(),
                name: "v1".to_string(),
                host: "http://127.0.0.1:3001".to_string(),
                priority: 0,
                conf: json!({}),
            },
        );

        assert!(result.is_err());
        let upstreams = storage.list_upstreams("hs_test").unwrap();
        assert_eq!(upstreams.len(), 1);
        assert_eq!(upstreams[0].host, "http://127.0.0.1:3000");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn add_route_rejects_missing_proxy_upstream_group() {
        let (storage, path) = test_storage(true);

        let result = storage.add_route(
            "hs_test",
            CreateRouteRequest {
                match_rule: RouteMatch {
                    r#type: 1,
                    path: "/api/".to_string(),
                },
                action: RouteAction {
                    r#type: "proxy".to_string(),
                    file: None,
                    proxy: Some(ProxyAction {
                        upstream: "api".to_string(),
                        websocket: WebSocketConfig::default(),
                        rewrite: None,
                    }),
                },
                conf: json!({}),
            },
        );

        assert!(result.is_err());
        assert!(storage.list_routes("hs_test").unwrap().is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn add_route_rejects_invalid_file_alias() {
        let (storage, path) = test_storage(true);

        let result = storage.add_route(
            "hs_test",
            CreateRouteRequest {
                match_rule: RouteMatch {
                    r#type: 1,
                    path: "/static/".to_string(),
                },
                action: RouteAction {
                    r#type: "file".to_string(),
                    file: Some(FileAction {
                        dir: "./public".to_string(),
                        alias: 2,
                    }),
                    proxy: None,
                },
                conf: json!({}),
            },
        );

        assert!(result.is_err());
        assert!(storage.list_routes("hs_test").unwrap().is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_rejects_unsupported_http_server_file_version() {
        let path = std::env::temp_dir().join(format!(
            "yiz-tunnel-storage-version-{}.json",
            Uuid::now_v7().simple()
        ));
        std::fs::write(&path, r#"{"version":2,"items":[]}"#).unwrap();

        assert!(HttpServerStorage::load_or_empty(path.clone()).is_err());

        let _ = std::fs::remove_file(path);
    }

    fn test_storage(graceful_enabled: bool) -> (HttpServerStorage, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "yiz-tunnel-storage-{}.json",
            Uuid::now_v7().simple()
        ));
        let storage = HttpServerStorage {
            path: path.clone(),
            data: Mutex::new(HttpServerFile {
                version: 1,
                items: vec![HttpServerConfig {
                    id: "hs_test".to_string(),
                    alias: "test".to_string(),
                    enabled: true,
                    listen: ListenConfig {
                        host: "127.0.0.1".to_string(),
                        port: 18080,
                        server_name: Vec::new(),
                    },
                    graceful: GracefulConfig {
                        enabled: graceful_enabled,
                        r#type: 0,
                    },
                    conf: json!({}),
                    upstreams: Vec::new(),
                    routes: Vec::new(),
                }],
            }),
        };

        (storage, path)
    }
}
