use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;
use uuid::Uuid;

use crate::error::ApiError;
use crate::model::{
    CreateHttpServerRequest, CreateRouteRequest, CreateUpstreamRequest, HttpServerConfig,
    HttpServerFile, RouteConfig, SetEnabledRequest, UpdateHttpServerRequest, UpstreamConfig,
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
        validate_conf(&request.conf)?;

        let mut data = self.lock()?;
        let server = find_http_server_mut(&mut data, id)?;
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
    for server in &data.items {
        validate_conf(&server.conf).map_err(api_error_to_io)?;

        for upstream in &server.upstreams {
            validate_conf(&upstream.conf).map_err(api_error_to_io)?;
        }

        for route in &server.routes {
            validate_route_config(route).map_err(api_error_to_io)?;
            validate_conf(&route.conf).map_err(api_error_to_io)?;
        }
    }

    Ok(())
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
        "file" if route.action.file.is_some() => Ok(()),
        "file" => Err(ApiError::invalid_request(
            "action.file is required when action.type is file",
        )),
        "proxy" if route.action.proxy.is_some() => Ok(()),
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
            if request.action.file.is_none() {
                return Err(ApiError::invalid_request(
                    "action.file is required when action.type is file",
                ));
            }
        }
        "proxy" => {
            if request.action.proxy.is_none() {
                return Err(ApiError::invalid_request(
                    "action.proxy is required when action.type is proxy",
                ));
            }
        }
        _ => {
            return Err(ApiError::invalid_request(
                "action.type only supports file and proxy",
            ));
        }
    }

    Ok(())
}

fn save_transaction(path: &Path, data: &HttpServerFile) -> std::io::Result<()> {
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
    use crate::model::{GracefulConfig, ListenConfig};
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
                        port: 0,
                        server_name: Vec::new(),
                    },
                    graceful: GracefulConfig::default(),
                    conf: json!({}),
                    upstreams: Vec::new(),
                    routes: Vec::new(),
                }],
            }),
        };

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
}
