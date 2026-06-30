use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use axum::http::StatusCode;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use crate::error::ApiError;
use crate::logger::{AccessLogEntry, LogManager};
use crate::model::{
    FileAction, HttpServerConfig, HttpServerInfo, ProxyAction, ProxyRewrite, RouteConfig,
    UpstreamConfig, UpstreamInfo,
};

#[derive(Clone)]
pub struct HttpRuntime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    handles: Mutex<HashMap<String, ServerHandle>>,
    infos: Mutex<HashMap<String, RuntimeInfo>>,
    proxy_state: ProxyState,
    active_connections: AtomicUsize,
    logger: LogManager,
}

struct ServerHandle {
    task: JoinHandle<()>,
    listen: String,
    config: Arc<RwLock<HttpServerConfig>>,
}

#[derive(Clone, Debug)]
struct RuntimeInfo {
    status: String,
    active_connection_count: usize,
    active_request_count: usize,
    last_error: Option<String>,
}

struct HttpRequest {
    method: String,
    target: String,
    version: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    content_length: Option<usize>,
    body_complete: bool,
}

struct AccessOutcome {
    method: String,
    path: String,
    status: u16,
    response_time_ms: u128,
    upstream_id: Option<String>,
    upstream_name: Option<String>,
}

#[derive(Clone, Default)]
struct ProxyState {
    round_robin: Arc<Mutex<HashMap<String, usize>>>,
    active_upstream_requests: Arc<Mutex<HashMap<String, usize>>>,
    retired_upstreams: Arc<Mutex<HashMap<String, RetiredUpstream>>>,
}

#[derive(Clone)]
struct RetiredUpstream {
    server_id: String,
    upstream: UpstreamConfig,
}

struct UpstreamRequestGuard {
    proxy_state: ProxyState,
    upstream_id: String,
}

impl Drop for UpstreamRequestGuard {
    fn drop(&mut self) {
        self.proxy_state
            .decrement_upstream_request(&self.upstream_id);
    }
}

impl ProxyState {
    fn increment_upstream_request(&self, upstream_id: &str) -> UpstreamRequestGuard {
        if let Ok(mut active_requests) = self.active_upstream_requests.lock() {
            *active_requests.entry(upstream_id.to_string()).or_insert(0) += 1;
        }

        UpstreamRequestGuard {
            proxy_state: self.clone(),
            upstream_id: upstream_id.to_string(),
        }
    }

    fn decrement_upstream_request(&self, upstream_id: &str) {
        if let Ok(mut active_requests) = self.active_upstream_requests.lock() {
            let Some(count) = active_requests.get_mut(upstream_id) else {
                return;
            };
            *count = count.saturating_sub(1);
            if *count == 0 {
                active_requests.remove(upstream_id);
            }
        }
    }

    fn active_upstream_request_count(&self, upstream_id: &str) -> usize {
        self.active_upstream_requests
            .lock()
            .ok()
            .and_then(|active_requests| active_requests.get(upstream_id).copied())
            .unwrap_or_default()
    }

    fn reconcile_upstreams(
        &self,
        server_id: &str,
        old_upstreams: &[UpstreamConfig],
        new_upstreams: &[UpstreamConfig],
    ) {
        let new_ids = new_upstreams
            .iter()
            .map(|upstream| upstream.id.as_str())
            .collect::<std::collections::HashSet<_>>();

        if let Ok(mut retired) = self.retired_upstreams.lock() {
            for old_upstream in old_upstreams {
                if new_ids.contains(old_upstream.id.as_str()) {
                    retired.remove(&old_upstream.id);
                    continue;
                }

                if self.active_upstream_request_count(&old_upstream.id) > 0 {
                    retired.insert(
                        old_upstream.id.clone(),
                        RetiredUpstream {
                            server_id: server_id.to_string(),
                            upstream: old_upstream.clone(),
                        },
                    );
                }
            }
        }
    }

    fn upstream_info(&self, upstream: &UpstreamConfig, status: &str) -> UpstreamInfo {
        UpstreamInfo {
            id: upstream.id.clone(),
            group: upstream.group.clone(),
            name: upstream.name.clone(),
            host: upstream.host.clone(),
            priority: upstream.priority,
            conf: upstream.conf.clone(),
            status: status.to_string(),
            active_request_count: self.active_upstream_request_count(&upstream.id),
        }
    }

    fn configured_upstream_info(&self, upstream: &UpstreamConfig) -> UpstreamInfo {
        self.upstream_info(upstream, "running")
    }

    fn retired_upstream_info(&self, server_id: &str, upstream_id: &str) -> Option<UpstreamInfo> {
        let retired = self.retired_upstreams.lock().ok()?;
        let retired = retired.get(upstream_id)?;
        if retired.server_id != server_id {
            return None;
        }
        Some(self.upstream_info(
            &retired.upstream,
            if self.active_upstream_request_count(upstream_id) > 0 {
                "deading"
            } else {
                "dead"
            },
        ))
    }

    fn retired_upstream_infos(&self, server_id: &str) -> Vec<UpstreamInfo> {
        self.retired_upstreams
            .lock()
            .map(|retired| {
                retired
                    .values()
                    .filter(|retired| retired.server_id == server_id)
                    .map(|retired| {
                        self.upstream_info(
                            &retired.upstream,
                            if self.active_upstream_request_count(&retired.upstream.id) > 0 {
                                "deading"
                            } else {
                                "dead"
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug)]
struct RuntimeConf {
    client_max_body_size: usize,
    client_header_timeout: Duration,
    client_body_timeout: Duration,
    send_timeout: Duration,
    keepalive_timeout: Duration,
    keepalive_requests: usize,
    proxy_connect_timeout: Duration,
    proxy_send_timeout: Duration,
    proxy_read_timeout: Duration,
}

impl Default for RuntimeConf {
    fn default() -> Self {
        Self {
            client_max_body_size: 1024 * 1024,
            client_header_timeout: Duration::from_millis(60_000),
            client_body_timeout: Duration::from_millis(60_000),
            send_timeout: Duration::from_millis(60_000),
            keepalive_timeout: Duration::from_millis(75_000),
            keepalive_requests: 1000,
            proxy_connect_timeout: Duration::from_millis(60_000),
            proxy_send_timeout: Duration::from_millis(60_000),
            proxy_read_timeout: Duration::from_millis(60_000),
        }
    }
}

impl RuntimeConf {
    fn from_value(value: &Value) -> Self {
        Self::default().merge(value)
    }

    fn for_route(server_conf: &Value, route_conf: &Value) -> Self {
        Self::from_value(server_conf).merge(route_conf)
    }

    fn for_upstream(self, upstream_conf: &Value) -> Self {
        self.merge(upstream_conf)
    }

    fn merge(mut self, value: &Value) -> Self {
        self.client_max_body_size =
            conf_usize(value, "client_max_body_size").unwrap_or(self.client_max_body_size);
        self.client_header_timeout =
            conf_duration(value, "client_header_timeout").unwrap_or(self.client_header_timeout);
        self.client_body_timeout =
            conf_duration(value, "client_body_timeout").unwrap_or(self.client_body_timeout);
        self.send_timeout = conf_duration(value, "send_timeout").unwrap_or(self.send_timeout);
        self.keepalive_timeout =
            conf_duration(value, "keepalive_timeout").unwrap_or(self.keepalive_timeout);
        self.keepalive_requests =
            conf_usize(value, "keepalive_requests").unwrap_or(self.keepalive_requests);
        self.proxy_connect_timeout =
            conf_duration(value, "proxy_connect_timeout").unwrap_or(self.proxy_connect_timeout);
        self.proxy_send_timeout =
            conf_duration(value, "proxy_send_timeout").unwrap_or(self.proxy_send_timeout);
        self.proxy_read_timeout =
            conf_duration(value, "proxy_read_timeout").unwrap_or(self.proxy_read_timeout);
        self
    }
}

impl HttpRuntime {
    pub fn new(logger: LogManager) -> Self {
        Self {
            inner: Arc::new(RuntimeInner {
                handles: Mutex::new(HashMap::new()),
                infos: Mutex::new(HashMap::new()),
                proxy_state: ProxyState::default(),
                active_connections: AtomicUsize::new(0),
                logger,
            }),
        }
    }

    pub async fn apply(&self, server: HttpServerConfig) -> Result<(), ApiError> {
        let addr = format!("{}:{}", server.listen.host, server.listen.port);

        let existing_config = {
            let handles = self
                .inner
                .handles
                .lock()
                .map_err(|_| ApiError::internal("runtime handle lock poisoned"))?;
            handles.get(&server.id).and_then(|handle| {
                if handle.listen == addr {
                    Some(Arc::clone(&handle.config))
                } else {
                    None
                }
            })
        };

        if let Some(config) = existing_config {
            let old_server = config.read().await.clone();
            self.inner.proxy_state.reconcile_upstreams(
                &server.id,
                &old_server.upstreams,
                &server.upstreams,
            );
            *config.write().await = server.clone();
            let current = self.info_snapshot(&server.id)?;
            self.set_info(
                &server.id,
                RuntimeInfo {
                    status: "running".to_string(),
                    active_connection_count: current.active_connection_count,
                    active_request_count: current.active_request_count,
                    last_error: None,
                },
            )?;
            return Ok(());
        }

        self.stop(&server.id)?;
        self.set_info(
            &server.id,
            RuntimeInfo {
                status: "starting".to_string(),
                active_connection_count: 0,
                active_request_count: 0,
                last_error: None,
            },
        )?;

        let listener = match TcpListener::bind(&addr).await {
            Ok(listener) => listener,
            Err(err) => {
                self.set_info(
                    &server.id,
                    RuntimeInfo {
                        status: "failed".to_string(),
                        active_connection_count: 0,
                        active_request_count: 0,
                        last_error: Some(err.to_string()),
                    },
                )?;
                return Err(ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    31000,
                    format!("failed to bind {addr}: {err}"),
                ));
            }
        };

        let server_id = server.id.clone();
        let task_server_id = server_id.clone();
        let config = Arc::new(RwLock::new(server));
        let inner = Arc::clone(&self.inner);
        let task_config = Arc::clone(&config);
        let task = tokio::spawn(async move {
            accept_loop(inner, task_server_id, task_config, listener).await;
        });

        self.inner
            .handles
            .lock()
            .map_err(|_| ApiError::internal("runtime handle lock poisoned"))?
            .insert(
                server_id.clone(),
                ServerHandle {
                    task,
                    listen: addr,
                    config,
                },
            );

        self.set_info(
            &server_id,
            RuntimeInfo {
                status: "running".to_string(),
                active_connection_count: 0,
                active_request_count: 0,
                last_error: None,
            },
        )?;

        Ok(())
    }

    pub fn stop(&self, id: &str) -> Result<(), ApiError> {
        if let Some(handle) = self
            .inner
            .handles
            .lock()
            .map_err(|_| ApiError::internal("runtime handle lock poisoned"))?
            .remove(id)
        {
            handle.task.abort();
        }

        let current = self.info_snapshot(id)?;
        let status = if current.active_connection_count > 0 {
            "stopping"
        } else {
            "stopped"
        };
        self.set_info(
            id,
            RuntimeInfo {
                status: status.to_string(),
                active_connection_count: current.active_connection_count,
                active_request_count: current.active_request_count,
                last_error: None,
            },
        )?;

        Ok(())
    }

    pub fn info(&self, server: &HttpServerConfig) -> Result<HttpServerInfo, ApiError> {
        let info = self
            .inner
            .infos
            .lock()
            .map_err(|_| ApiError::internal("runtime info lock poisoned"))?
            .get(&server.id)
            .cloned()
            .unwrap_or_else(|| RuntimeInfo {
                status: if server.enabled { "stopped" } else { "stopped" }.to_string(),
                active_connection_count: 0,
                active_request_count: 0,
                last_error: None,
            });

        Ok(HttpServerInfo {
            id: server.id.clone(),
            alias: server.alias.clone(),
            enabled: server.enabled,
            status: info.status,
            active_connection_count: info.active_connection_count,
            active_request_count: info.active_request_count,
            last_error: info.last_error,
        })
    }

    pub fn upstream_info(&self, upstream: &UpstreamConfig) -> UpstreamInfo {
        self.inner.proxy_state.configured_upstream_info(upstream)
    }

    pub fn retired_upstream_info(
        &self,
        server_id: &str,
        upstream_id: &str,
    ) -> Option<UpstreamInfo> {
        self.inner
            .proxy_state
            .retired_upstream_info(server_id, upstream_id)
    }

    pub fn retired_upstream_infos(&self, server_id: &str) -> Vec<UpstreamInfo> {
        self.inner.proxy_state.retired_upstream_infos(server_id)
    }

    pub fn active_connection_count(&self) -> Result<usize, ApiError> {
        Ok(self.inner.active_connections.load(Ordering::Relaxed))
    }

    pub fn last_error(&self) -> Result<Option<String>, ApiError> {
        Ok(self
            .inner
            .infos
            .lock()
            .map_err(|_| ApiError::internal("runtime info lock poisoned"))?
            .values()
            .filter_map(|info| info.last_error.clone())
            .last())
    }

    fn set_info(&self, id: &str, info: RuntimeInfo) -> Result<(), ApiError> {
        self.inner
            .infos
            .lock()
            .map_err(|_| ApiError::internal("runtime info lock poisoned"))?
            .insert(id.to_string(), info);
        Ok(())
    }

    fn info_snapshot(&self, id: &str) -> Result<RuntimeInfo, ApiError> {
        Ok(self
            .inner
            .infos
            .lock()
            .map_err(|_| ApiError::internal("runtime info lock poisoned"))?
            .get(id)
            .cloned()
            .unwrap_or_else(|| RuntimeInfo {
                status: "stopped".to_string(),
                active_connection_count: 0,
                active_request_count: 0,
                last_error: None,
            }))
    }
}

async fn accept_loop(
    inner: Arc<RuntimeInner>,
    server_id: String,
    config: Arc<RwLock<HttpServerConfig>>,
    listener: TcpListener,
) {
    while let Ok((stream, remote_addr)) = listener.accept().await {
        let config = Arc::clone(&config);
        let inner = Arc::clone(&inner);
        let server_id = server_id.clone();
        tokio::spawn(async move {
            increment_connection(&inner, &server_id);
            match handle_connection(
                stream,
                Arc::clone(&config),
                remote_addr.to_string(),
                inner.proxy_state.clone(),
            )
            .await
            {
                Ok(outcomes) => {
                    let alias = config.read().await.alias.clone();
                    for outcome in outcomes {
                        inner.logger.access(AccessLogEntry {
                            remote_address: remote_addr.to_string(),
                            http_server_id: server_id.clone(),
                            http_server_alias: alias.clone(),
                            method: outcome.method,
                            path: outcome.path,
                            status: outcome.status,
                            response_time_ms: outcome.response_time_ms,
                            upstream_id: outcome.upstream_id,
                            upstream_name: outcome.upstream_name,
                        });
                    }
                }
                Err(err) => {
                    inner.logger.error(
                        "error",
                        "runtime",
                        "connection handling failed",
                        Some(err.to_string()),
                    );
                }
            }
            decrement_connection(&inner, &server_id);
        });
    }
}

fn increment_connection(inner: &RuntimeInner, id: &str) {
    inner.active_connections.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut infos) = inner.infos.lock() {
        if let Some(info) = infos.get_mut(id) {
            info.active_connection_count += 1;
            info.active_request_count += 1;
        }
    }
}

fn decrement_connection(inner: &RuntimeInner, id: &str) {
    inner.active_connections.fetch_sub(1, Ordering::Relaxed);
    if let Ok(mut infos) = inner.infos.lock() {
        if let Some(info) = infos.get_mut(id) {
            info.active_connection_count = info.active_connection_count.saturating_sub(1);
            info.active_request_count = info.active_request_count.saturating_sub(1);
            if info.status == "stopping" && info.active_connection_count == 0 {
                info.status = "stopped".to_string();
            }
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    config: Arc<RwLock<HttpServerConfig>>,
    remote_address: String,
    proxy_state: ProxyState,
) -> io::Result<Vec<AccessOutcome>> {
    let mut outcomes = Vec::new();
    let mut handled = 0_usize;

    loop {
        let read_conf = {
            let server = config.read().await;
            RuntimeConf::from_value(&server.conf)
        };
        let header_timeout = if handled == 0 {
            read_conf.client_header_timeout
        } else {
            read_conf.keepalive_timeout
        };
        let request = match read_http_request(
            &mut stream,
            header_timeout,
            read_conf.client_body_timeout,
            read_conf.client_max_body_size,
        )
        .await
        {
            Ok(Some(request)) => request,
            Ok(None) => break,
            Err(err)
                if err.kind() == io::ErrorKind::InvalidData
                    && err.to_string() == "request body too large" =>
            {
                write_simple_response(
                    &mut stream,
                    413,
                    "Payload Too Large",
                    b"payload too large",
                    "text/plain",
                    false,
                    read_conf.send_timeout,
                )
                .await?;
                break;
            }
            Err(err) if err.kind() == io::ErrorKind::TimedOut => {
                break;
            }
            Err(err) => return Err(err),
        };

        let server = config.read().await.clone();
        let server_conf = RuntimeConf::from_value(&server.conf);
        handled += 1;
        let start = std::time::Instant::now();
        let path = request_path(&request.target);
        let keep_alive =
            request_keep_alive(&request) && handled < server_conf.keepalive_requests.max(1);
        let mut close_after_response = !keep_alive;
        if !request.body_complete {
            close_after_response = true;
        }

        let result = if let Some(route) = select_route(&server.routes, path) {
            let route_conf = RuntimeConf::for_route(&server.conf, &route.conf);
            match route.action.r#type.as_str() {
                "file" => {
                    if let Some(file) = &route.action.file {
                        let status = serve_file(
                            &mut stream,
                            route,
                            file,
                            path,
                            keep_alive,
                            &request,
                            &route_conf,
                        )
                        .await?;
                        outcome(
                            &request,
                            path,
                            status,
                            start.elapsed().as_millis(),
                            None,
                            None,
                        )
                    } else {
                        write_simple_response(
                            &mut stream,
                            500,
                            "Internal Server Error",
                            b"file action missing",
                            "text/plain",
                            keep_alive,
                            route_conf.send_timeout,
                        )
                        .await?;
                        outcome(&request, path, 500, start.elapsed().as_millis(), None, None)
                    }
                }
                "proxy" => {
                    close_after_response = true;
                    if let Some(proxy) = &route.action.proxy {
                        let (status, upstream_id, upstream_name) = serve_proxy(
                            &mut stream,
                            &server,
                            proxy,
                            &request,
                            &remote_address,
                            &route_conf,
                            &proxy_state,
                        )
                        .await?;
                        outcome(
                            &request,
                            path,
                            status,
                            start.elapsed().as_millis(),
                            upstream_id,
                            upstream_name,
                        )
                    } else {
                        write_simple_response(
                            &mut stream,
                            500,
                            "Internal Server Error",
                            b"proxy action missing",
                            "text/plain",
                            false,
                            route_conf.send_timeout,
                        )
                        .await?;
                        outcome(&request, path, 500, start.elapsed().as_millis(), None, None)
                    }
                }
                _ => {
                    write_simple_response(
                        &mut stream,
                        500,
                        "Internal Server Error",
                        b"unsupported action",
                        "text/plain",
                        keep_alive,
                        route_conf.send_timeout,
                    )
                    .await?;
                    outcome(&request, path, 500, start.elapsed().as_millis(), None, None)
                }
            }
        } else {
            write_simple_response(
                &mut stream,
                404,
                "Not Found",
                b"not found",
                "text/plain",
                keep_alive,
                server_conf.send_timeout,
            )
            .await?;
            outcome(&request, path, 404, start.elapsed().as_millis(), None, None)
        };

        outcomes.push(result);

        if close_after_response {
            break;
        }
    }

    Ok(outcomes)
}

async fn read_http_request(
    stream: &mut TcpStream,
    header_timeout: Duration,
    body_timeout: Duration,
    max_body_size: usize,
) -> io::Result<Option<HttpRequest>> {
    let mut buffer = Vec::with_capacity(4096);
    let header_end;

    loop {
        let mut chunk = [0_u8; 1024];
        let read = timed_read(stream, &mut chunk, header_timeout).await?;
        if read == 0 {
            if buffer.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before headers completed",
            ));
        }

        buffer.extend_from_slice(&chunk[..read]);
        if let Some(index) = find_header_end(&buffer) {
            header_end = index;
            break;
        }

        if buffer.len() > 64 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request headers too large",
            ));
        }
    }

    let headers_raw = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = headers_raw.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"))?
        .to_string();
    let target = request_parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing target"))?
        .to_string();
    let version = request_parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing version"))?
        .to_string();

    let mut headers = Vec::new();
    let mut content_length = None;
    let mut chunked = false;

    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_string();
        let value = value.trim().to_string();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(value.parse().unwrap_or(0));
        }
        if name.eq_ignore_ascii_case("transfer-encoding")
            && value.to_ascii_lowercase().contains("chunked")
        {
            chunked = true;
        }
        headers.push((name, value));
    }

    let body_start = header_end + 4;
    let (body, content_length, body_complete) = if chunked {
        let body = read_chunked_body(
            stream,
            buffer[body_start..].to_vec(),
            body_timeout,
            max_body_size,
        )
        .await?;
        let content_length = if body.is_empty() {
            None
        } else {
            Some(body.len())
        };
        (body, content_length, true)
    } else {
        let content_length = content_length.unwrap_or(0);
        if content_length > max_body_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request body too large",
            ));
        }
        let mut body = buffer[body_start..].to_vec();
        if body.len() > content_length {
            body.truncate(content_length);
        }
        let body_complete = body.len() >= content_length;
        let content_length = if content_length == 0 {
            None
        } else {
            Some(content_length)
        };
        (body, content_length, body_complete)
    };

    Ok(Some(HttpRequest {
        method,
        target,
        version,
        headers,
        body,
        content_length,
        body_complete,
    }))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn timed_read(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    read_timeout: Duration,
) -> io::Result<usize> {
    match timeout(read_timeout, stream.read(buffer)).await {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "read timed out")),
    }
}

async fn timed_write_all(
    stream: &mut TcpStream,
    buffer: &[u8],
    write_timeout: Duration,
) -> io::Result<()> {
    match timeout(write_timeout, stream.write_all(buffer)).await {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "write timed out")),
    }
}

async fn read_chunked_body(
    stream: &mut TcpStream,
    mut buffer: Vec<u8>,
    body_timeout: Duration,
    max_body_size: usize,
) -> io::Result<Vec<u8>> {
    let mut position = 0_usize;
    let mut body = Vec::new();

    loop {
        let size_line = read_buffer_line(stream, &mut buffer, &mut position, body_timeout).await?;
        let size_text = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid chunk size"))?;

        if size == 0 {
            loop {
                let trailer =
                    read_buffer_line(stream, &mut buffer, &mut position, body_timeout).await?;
                if trailer.is_empty() {
                    break;
                }
            }
            break;
        }

        if body.len().saturating_add(size) > max_body_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request body too large",
            ));
        }

        ensure_buffer(stream, &mut buffer, position + size + 2, body_timeout).await?;
        body.extend_from_slice(&buffer[position..position + size]);
        position += size;

        if buffer.get(position..position + 2) != Some(b"\r\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "chunk missing trailing CRLF",
            ));
        }
        position += 2;
    }

    Ok(body)
}

async fn read_buffer_line(
    stream: &mut TcpStream,
    buffer: &mut Vec<u8>,
    position: &mut usize,
    read_timeout: Duration,
) -> io::Result<String> {
    loop {
        if let Some(relative) = buffer[*position..]
            .windows(2)
            .position(|window| window == b"\r\n")
        {
            let start = *position;
            let end = *position + relative;
            *position = end + 2;
            return Ok(String::from_utf8_lossy(&buffer[start..end]).into_owned());
        }

        let current_len = buffer.len();
        ensure_buffer(stream, buffer, current_len + 1, read_timeout).await?;
    }
}

async fn ensure_buffer(
    stream: &mut TcpStream,
    buffer: &mut Vec<u8>,
    required_len: usize,
    read_timeout: Duration,
) -> io::Result<()> {
    while buffer.len() < required_len {
        let mut chunk = [0_u8; 1024];
        let read = timed_read(stream, &mut chunk, read_timeout).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed while reading chunked body",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    Ok(())
}

fn request_path(target: &str) -> &str {
    target
        .split_once('?')
        .map(|(path, _)| path)
        .unwrap_or(target)
}

pub fn select_route<'a>(routes: &'a [RouteConfig], path: &str) -> Option<&'a RouteConfig> {
    if let Some(route) = routes
        .iter()
        .find(|route| route.match_rule.r#type == 0 && route.match_rule.path == path)
    {
        return Some(route);
    }

    routes
        .iter()
        .filter(|route| route.match_rule.r#type == 1 && path.starts_with(&route.match_rule.path))
        .max_by_key(|route| route.match_rule.path.len())
}

async fn serve_file(
    stream: &mut TcpStream,
    route: &RouteConfig,
    file: &FileAction,
    path: &str,
    keep_alive: bool,
    request: &HttpRequest,
    conf: &RuntimeConf,
) -> io::Result<u16> {
    let Some(file_path) = resolve_file_path(route, file, path) else {
        write_simple_response(
            stream,
            403,
            "Forbidden",
            b"forbidden",
            "text/plain",
            keep_alive,
            conf.send_timeout,
        )
        .await?;
        return Ok(403);
    };

    let metadata = match tokio::fs::metadata(&file_path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            write_simple_response(
                stream,
                404,
                "Not Found",
                b"not found",
                "text/plain",
                keep_alive,
                conf.send_timeout,
            )
            .await?;
            return Ok(404);
        }
        Err(_) => {
            write_simple_response(
                stream,
                403,
                "Forbidden",
                b"forbidden",
                "text/plain",
                keep_alive,
                conf.send_timeout,
            )
            .await?;
            return Ok(403);
        }
    };

    if metadata.is_dir() {
        write_simple_response(
            stream,
            403,
            "Forbidden",
            b"forbidden",
            "text/plain",
            keep_alive,
            conf.send_timeout,
        )
        .await?;
        return Ok(403);
    }

    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let etag = file_etag(metadata.len(), modified);
    let last_modified = httpdate::fmt_http_date(modified);

    if request
        .headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("if-none-match") && value.trim() == etag)
    {
        write_response(
            stream,
            304,
            "Not Modified",
            b"",
            "text/plain",
            keep_alive,
            &[
                ("ETag", etag.as_str()),
                ("Last-Modified", last_modified.as_str()),
                ("Accept-Ranges", "bytes"),
            ],
            conf.send_timeout,
        )
        .await?;
        return Ok(304);
    }

    if request.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("if-modified-since")
            && httpdate::parse_http_date(value)
                .map(|since| modified <= since)
                .unwrap_or(false)
    }) {
        write_response(
            stream,
            304,
            "Not Modified",
            b"",
            "text/plain",
            keep_alive,
            &[
                ("ETag", etag.as_str()),
                ("Last-Modified", last_modified.as_str()),
                ("Accept-Ranges", "bytes"),
            ],
            conf.send_timeout,
        )
        .await?;
        return Ok(304);
    }

    let content = match tokio::fs::read(&file_path).await {
        Ok(content) => content,
        Err(_) => {
            write_simple_response(
                stream,
                403,
                "Forbidden",
                b"forbidden",
                "text/plain",
                keep_alive,
                conf.send_timeout,
            )
            .await?;
            return Ok(403);
        }
    };

    let mime = mime_type(&file_path);
    if let Some(range_header) = request_header(request, "range") {
        match parse_byte_range(range_header, content.len() as u64) {
            Some((start, end)) => {
                let content_range = format!("bytes {start}-{end}/{}", content.len());
                let body = &content[start as usize..=end as usize];
                write_response(
                    stream,
                    206,
                    "Partial Content",
                    body,
                    mime,
                    keep_alive,
                    &[
                        ("ETag", etag.as_str()),
                        ("Last-Modified", last_modified.as_str()),
                        ("Accept-Ranges", "bytes"),
                        ("Content-Range", content_range.as_str()),
                    ],
                    conf.send_timeout,
                )
                .await?;
                return Ok(206);
            }
            None => {
                let content_range = format!("bytes */{}", content.len());
                write_response(
                    stream,
                    416,
                    "Range Not Satisfiable",
                    b"range not satisfiable",
                    "text/plain",
                    keep_alive,
                    &[
                        ("ETag", etag.as_str()),
                        ("Last-Modified", last_modified.as_str()),
                        ("Accept-Ranges", "bytes"),
                        ("Content-Range", content_range.as_str()),
                    ],
                    conf.send_timeout,
                )
                .await?;
                return Ok(416);
            }
        }
    }

    write_response(
        stream,
        200,
        "OK",
        &content,
        mime,
        keep_alive,
        &[
            ("ETag", etag.as_str()),
            ("Last-Modified", last_modified.as_str()),
            ("Accept-Ranges", "bytes"),
        ],
        conf.send_timeout,
    )
    .await?;
    Ok(200)
}

fn resolve_file_path(route: &RouteConfig, file: &FileAction, path: &str) -> Option<PathBuf> {
    let suffix = if file.alias == 1 {
        path.strip_prefix(&route.match_rule.path).unwrap_or(path)
    } else {
        path
    };

    let mut result = PathBuf::from(&file.dir);
    for segment in suffix.trim_start_matches('/').split('/') {
        if segment.is_empty() {
            continue;
        }
        let component_path = Path::new(segment);
        if component_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return None;
        }
        result.push(segment);
    }

    Some(result)
}

fn mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
    {
        "css" => "text/css",
        "gif" => "image/gif",
        "html" | "htm" => "text/html",
        "jpeg" | "jpg" => "image/jpeg",
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

fn select_upstream_candidates<'a>(
    server_id: &str,
    group: &str,
    upstreams: &'a [UpstreamConfig],
    proxy_state: &ProxyState,
) -> Vec<&'a UpstreamConfig> {
    let mut candidates = upstreams
        .iter()
        .filter(|upstream| upstream.group == group)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|upstream| upstream.priority);

    let mut result = Vec::with_capacity(candidates.len());
    let mut start = 0_usize;
    while start < candidates.len() {
        let priority = candidates[start].priority;
        let mut end = start + 1;
        while end < candidates.len() && candidates[end].priority == priority {
            end += 1;
        }

        let group_candidates = &candidates[start..end];
        if group_candidates.len() <= 1 {
            result.extend_from_slice(group_candidates);
        } else {
            let key = format!("{server_id}:{group}:{priority}");
            let offset = proxy_state
                .round_robin
                .lock()
                .map(|mut round_robin| {
                    let cursor = round_robin.entry(key).or_insert(0);
                    let offset = *cursor % group_candidates.len();
                    *cursor = cursor.wrapping_add(1);
                    offset
                })
                .unwrap_or(0);

            result.extend(
                group_candidates
                    .iter()
                    .cycle()
                    .skip(offset)
                    .take(group_candidates.len())
                    .copied(),
            );
        }
        start = end;
    }

    result
}

async fn serve_proxy(
    client: &mut TcpStream,
    server: &HttpServerConfig,
    proxy: &ProxyAction,
    request: &HttpRequest,
    remote_address: &str,
    route_conf: &RuntimeConf,
    proxy_state: &ProxyState,
) -> io::Result<(u16, Option<String>, Option<String>)> {
    let candidates =
        select_upstream_candidates(&server.id, &proxy.upstream, &server.upstreams, proxy_state);
    if candidates.is_empty() {
        write_simple_response(
            client,
            502,
            "Bad Gateway",
            b"bad gateway",
            "text/plain",
            false,
            route_conf.send_timeout,
        )
        .await?;
        return Ok((502, None, None));
    }

    let mut last_upstream_id = None;
    let mut last_upstream_name = None;
    let mut last_failure_was_timeout = false;
    let mut connected = None;

    for upstream in candidates {
        last_upstream_id = Some(upstream.id.clone());
        last_upstream_name = Some(upstream.name.clone());
        let conf = route_conf.for_upstream(&upstream.conf);
        let Some((host, port)) = parse_http_upstream(&upstream.host) else {
            last_failure_was_timeout = false;
            continue;
        };

        match timeout(
            conf.proxy_connect_timeout,
            TcpStream::connect(format!("{host}:{port}")),
        )
        .await
        {
            Ok(Ok(stream)) => {
                connected = Some((upstream, conf, host, stream));
                break;
            }
            Ok(Err(_)) => {
                last_failure_was_timeout = false;
            }
            Err(_) => {
                last_failure_was_timeout = true;
            }
        }
    }

    let Some((upstream, conf, host, mut upstream_stream)) = connected else {
        if last_failure_was_timeout {
            write_simple_response(
                client,
                504,
                "Gateway Timeout",
                b"gateway timeout",
                "text/plain",
                false,
                route_conf.send_timeout,
            )
            .await?;
            return Ok((504, last_upstream_id, last_upstream_name));
        }

        write_simple_response(
            client,
            502,
            "Bad Gateway",
            b"bad gateway",
            "text/plain",
            false,
            route_conf.send_timeout,
        )
        .await?;
        return Ok((502, last_upstream_id, last_upstream_name));
    };

    let upstream_id = Some(upstream.id.clone());
    let upstream_name = Some(upstream.name.clone());
    let _upstream_request_guard = proxy_state.increment_upstream_request(&upstream.id);

    let is_websocket = proxy.websocket.enabled && request_is_websocket(request);
    let request_bytes = build_proxy_request(
        request,
        &host,
        remote_address,
        is_websocket,
        proxy.rewrite.as_ref(),
    );
    timed_write_all(
        &mut upstream_stream,
        &request_bytes,
        conf.proxy_send_timeout,
    )
    .await?;
    stream_proxy_request_body(client, &mut upstream_stream, request, &conf).await?;

    if is_websocket {
        let _ = tokio::io::copy_bidirectional(client, &mut upstream_stream).await;
        return Ok((101, upstream_id, upstream_name));
    }

    let status = match proxy_upstream_response(client, &mut upstream_stream, &conf).await {
        Ok(status) => status,
        Err(err) if err.kind() == io::ErrorKind::TimedOut => {
            write_simple_response(
                client,
                504,
                "Gateway Timeout",
                b"gateway timeout",
                "text/plain",
                false,
                conf.send_timeout,
            )
            .await?;
            return Ok((504, upstream_id, upstream_name));
        }
        Err(_) => {
            write_simple_response(
                client,
                502,
                "Bad Gateway",
                b"bad gateway",
                "text/plain",
                false,
                conf.send_timeout,
            )
            .await?;
            return Ok((502, upstream_id, upstream_name));
        }
    };

    Ok((status, upstream_id, upstream_name))
}

async fn stream_proxy_request_body(
    client: &mut TcpStream,
    upstream: &mut TcpStream,
    request: &HttpRequest,
    conf: &RuntimeConf,
) -> io::Result<()> {
    let Some(content_length) = request.content_length else {
        return Ok(());
    };
    if request.body_complete {
        return Ok(());
    }

    let mut remaining = content_length.saturating_sub(request.body.len());
    while remaining > 0 {
        let mut chunk = vec![0_u8; remaining.min(8192)];
        let read = timed_read(client, &mut chunk, conf.client_body_timeout).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "client closed before request body completed",
            ));
        }
        timed_write_all(upstream, &chunk[..read], conf.proxy_send_timeout).await?;
        remaining -= read;
    }

    Ok(())
}

async fn proxy_upstream_response(
    client: &mut TcpStream,
    upstream: &mut TcpStream,
    conf: &RuntimeConf,
) -> io::Result<u16> {
    let mut buffer = Vec::with_capacity(4096);
    let status;

    loop {
        let mut chunk = [0_u8; 8192];
        let read = timed_read(upstream, &mut chunk, conf.proxy_read_timeout).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "upstream closed before response headers",
            ));
        }

        buffer.extend_from_slice(&chunk[..read]);
        if find_header_end(&buffer).is_some() {
            status = status_from_response(&buffer).unwrap_or(200);
            timed_write_all(client, &buffer, conf.send_timeout).await?;
            break;
        }

        if buffer.len() > 64 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "upstream response headers too large",
            ));
        }
    }

    loop {
        let mut chunk = [0_u8; 8192];
        match timed_read(upstream, &mut chunk, conf.proxy_read_timeout).await {
            Ok(0) => break,
            Ok(read) => timed_write_all(client, &chunk[..read], conf.send_timeout).await?,
            Err(err) if err.kind() == io::ErrorKind::TimedOut => break,
            Err(err) => return Err(err),
        }
    }

    Ok(status)
}

fn parse_http_upstream(value: &str) -> Option<(String, u16)> {
    let rest = value.strip_prefix("http://")?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().ok()?),
        None => (authority.to_string(), 80),
    };

    if host.is_empty() {
        None
    } else {
        Some((host, port))
    }
}

fn outcome(
    request: &HttpRequest,
    path: &str,
    status: u16,
    response_time_ms: u128,
    upstream_id: Option<String>,
    upstream_name: Option<String>,
) -> AccessOutcome {
    AccessOutcome {
        method: request.method.clone(),
        path: path.to_string(),
        status,
        response_time_ms,
        upstream_id,
        upstream_name,
    }
}

fn conf_usize(value: &Value, name: &str) -> Option<usize> {
    let raw = value.as_object()?.get(name)?.as_u64()?;
    usize::try_from(raw).ok()
}

fn conf_duration(value: &Value, name: &str) -> Option<Duration> {
    conf_usize(value, name).map(|millis| Duration::from_millis(millis as u64))
}

fn request_keep_alive(request: &HttpRequest) -> bool {
    let connection = request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("connection"))
        .map(|(_, value)| value.to_ascii_lowercase());

    if request.version.eq_ignore_ascii_case("HTTP/1.0") {
        return connection
            .as_deref()
            .map(|value| value.contains("keep-alive"))
            .unwrap_or(false);
    }

    !connection
        .as_deref()
        .map(|value| value.contains("close"))
        .unwrap_or(false)
}

fn request_header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn parse_byte_range(value: &str, size: u64) -> Option<(u64, u64)> {
    if size == 0 {
        return None;
    }

    let value = value.trim().strip_prefix("bytes=")?;
    if value.contains(',') {
        return None;
    }

    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = size.saturating_sub(suffix);
        return Some((start, size - 1));
    }

    let start = start.parse::<u64>().ok()?;
    if start >= size {
        return None;
    }

    let end = if end.is_empty() {
        size - 1
    } else {
        end.parse::<u64>().ok()?.min(size - 1)
    };

    if end < start {
        None
    } else {
        Some((start, end))
    }
}

fn status_from_response(response: &[u8]) -> Option<u16> {
    let line_end = response.windows(2).position(|window| window == b"\r\n")?;
    let line = std::str::from_utf8(&response[..line_end]).ok()?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn request_is_websocket(request: &HttpRequest) -> bool {
    let has_upgrade = request.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("upgrade") && value.eq_ignore_ascii_case("websocket")
    });
    let has_connection_upgrade = request.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("connection") && value.to_ascii_lowercase().contains("upgrade")
    });
    has_upgrade && has_connection_upgrade
}

fn build_proxy_request(
    request: &HttpRequest,
    upstream_host: &str,
    remote_address: &str,
    websocket: bool,
    rewrite: Option<&ProxyRewrite>,
) -> Vec<u8> {
    let mut result = Vec::new();
    let target = proxy_target(request, rewrite);
    result.extend_from_slice(
        format!("{} {} {}\r\n", request.method, target, request.version).as_bytes(),
    );

    let mut wrote_host = false;
    let mut existing_x_forwarded_for = None;
    for (name, value) in &request.headers {
        if name.eq_ignore_ascii_case("host") {
            wrote_host = true;
            result.extend_from_slice(format!("Host: {upstream_host}\r\n").as_bytes());
            continue;
        }

        if name.eq_ignore_ascii_case("x-real-ip")
            || name.eq_ignore_ascii_case("x-forwarded-for")
            || name.eq_ignore_ascii_case("x-forwarded-proto")
        {
            if name.eq_ignore_ascii_case("x-forwarded-for") {
                existing_x_forwarded_for = Some(value.clone());
            }
            continue;
        }

        if name.eq_ignore_ascii_case("content-length")
            || name.eq_ignore_ascii_case("transfer-encoding")
        {
            continue;
        }

        if !websocket
            && (name.eq_ignore_ascii_case("connection")
                || name.eq_ignore_ascii_case("proxy-connection")
                || name.eq_ignore_ascii_case("keep-alive"))
        {
            continue;
        }

        result.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }

    if !wrote_host {
        result.extend_from_slice(format!("Host: {upstream_host}\r\n").as_bytes());
    }

    let remote_ip = remote_address
        .rsplit_once(':')
        .map(|(ip, _)| ip)
        .unwrap_or(remote_address);
    result.extend_from_slice(format!("X-Real-IP: {remote_ip}\r\n").as_bytes());
    let x_forwarded_for = match existing_x_forwarded_for {
        Some(value) if !value.is_empty() => format!("{value}, {remote_ip}"),
        _ => remote_ip.to_string(),
    };
    result.extend_from_slice(format!("X-Forwarded-For: {x_forwarded_for}\r\n").as_bytes());
    result.extend_from_slice(b"X-Forwarded-Proto: http\r\n");

    if !websocket {
        result.extend_from_slice(b"Connection: close\r\n");
    }
    if let Some(content_length) = request.content_length {
        result.extend_from_slice(format!("Content-Length: {content_length}\r\n").as_bytes());
    }

    result.extend_from_slice(b"\r\n");
    result.extend_from_slice(&request.body);
    result
}

fn proxy_target(request: &HttpRequest, rewrite: Option<&ProxyRewrite>) -> String {
    let (path, query) = request
        .target
        .split_once('?')
        .map(|(path, query)| (path, Some(query)))
        .unwrap_or((request.target.as_str(), None));

    let path = rewrite_proxy_path(path, rewrite).unwrap_or_else(|| path.to_string());
    match query {
        Some(query) => format!("{path}?{query}"),
        None => path,
    }
}

fn rewrite_proxy_path(path: &str, rewrite: Option<&ProxyRewrite>) -> Option<String> {
    let rewrite = rewrite?;
    if rewrite.r#type != "replacePrefix" || !path.starts_with(&rewrite.from) {
        return None;
    }

    Some(format!("{}{}", rewrite.to, &path[rewrite.from.len()..]))
}

async fn write_simple_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    body: &[u8],
    content_type: &str,
    keep_alive: bool,
    send_timeout: Duration,
) -> io::Result<()> {
    write_response(
        stream,
        status,
        reason,
        body,
        content_type,
        keep_alive,
        &[],
        send_timeout,
    )
    .await
}

async fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    body: &[u8],
    content_type: &str,
    keep_alive: bool,
    extra_headers: &[(&str, &str)],
    send_timeout: Duration,
) -> io::Result<()> {
    let connection = if keep_alive { "keep-alive" } else { "close" };
    let mut headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: {connection}\r\n",
        body.len()
    );
    for (name, value) in extra_headers {
        headers.push_str(name);
        headers.push_str(": ");
        headers.push_str(value);
        headers.push_str("\r\n");
    }
    headers.push_str("\r\n");
    timed_write_all(stream, headers.as_bytes(), send_timeout).await?;
    timed_write_all(stream, body, send_timeout).await
}

fn file_etag(size: u64, modified: SystemTime) -> String {
    let modified_secs = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("W/\"{size:x}-{modified_secs:x}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::LogManager;
    use crate::model::{
        FileAction, GracefulConfig, ListenConfig, ProxyAction, RouteAction, RouteMatch,
        UpstreamConfig, WebSocketConfig,
    };
    use serde_json::{Value, json};
    use tokio::net::TcpListener;
    use tokio::time::sleep;
    use uuid::Uuid;

    fn route(id: &str, match_type: u8, path: &str) -> RouteConfig {
        RouteConfig {
            id: id.to_string(),
            match_rule: RouteMatch {
                r#type: match_type,
                path: path.to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: ".".to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        }
    }

    #[test]
    fn exact_route_wins_over_prefix() {
        let routes = vec![route("prefix", 1, "/api/"), route("exact", 0, "/api/users")];
        let selected = select_route(&routes, "/api/users").unwrap();
        assert_eq!(selected.id, "exact");
    }

    #[test]
    fn longest_prefix_route_wins() {
        let routes = vec![route("short", 1, "/api/"), route("long", 1, "/api/users/")];
        let selected = select_route(&routes, "/api/users/1").unwrap();
        assert_eq!(selected.id, "long");
    }

    #[test]
    fn file_root_rejects_parent_segments() {
        let route = route("static", 1, "/static/");
        let file = FileAction {
            dir: "public".to_string(),
            alias: 0,
        };
        assert!(resolve_file_path(&route, &file, "/../secret").is_none());
    }

    #[test]
    fn alias_file_path_uses_route_suffix() {
        let route = route("static", 1, "/static/");
        let file = FileAction {
            dir: "public".to_string(),
            alias: 1,
        };
        let path = resolve_file_path(&route, &file, "/static/app.js").unwrap();
        assert_eq!(path, PathBuf::from("public").join("app.js"));
    }

    #[test]
    fn proxy_request_adds_forward_headers() {
        let request = HttpRequest {
            method: "GET".to_string(),
            target: "/api/users".to_string(),
            version: "HTTP/1.1".to_string(),
            headers: vec![
                ("Host".to_string(), "example.test".to_string()),
                ("X-Forwarded-For".to_string(), "10.0.0.1".to_string()),
            ],
            body: Vec::new(),
            content_length: None,
            body_complete: true,
        };

        let raw = build_proxy_request(&request, "127.0.0.1", "127.0.0.2:50100", false, None);
        let text = String::from_utf8(raw).unwrap();

        assert!(text.contains("Host: 127.0.0.1\r\n"));
        assert!(text.contains("X-Real-IP: 127.0.0.2\r\n"));
        assert!(text.contains("X-Forwarded-For: 10.0.0.1, 127.0.0.2\r\n"));
        assert!(text.contains("X-Forwarded-Proto: http\r\n"));
    }

    #[test]
    fn proxy_rewrite_replace_prefix_preserves_query() {
        let request = HttpRequest {
            method: "GET".to_string(),
            target: "/123456789012345/a.png?v=1".to_string(),
            version: "HTTP/1.1".to_string(),
            headers: vec![("Host".to_string(), "example.test".to_string())],
            body: Vec::new(),
            content_length: None,
            body_complete: true,
        };
        let rewrite = ProxyRewrite {
            r#type: "replacePrefix".to_string(),
            from: "/123456789012345/".to_string(),
            to: "/".to_string(),
        };

        let raw = build_proxy_request(
            &request,
            "127.0.0.1",
            "127.0.0.2:50100",
            false,
            Some(&rewrite),
        );
        let text = String::from_utf8(raw).unwrap();

        assert!(text.starts_with("GET /a.png?v=1 HTTP/1.1\r\n"));
    }

    #[tokio::test]
    async fn static_file_request_returns_file_content() {
        let dir = std::env::temp_dir().join(format!("yiz-tunnel-test-{}", Uuid::now_v7().simple()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("hello.txt"), b"hello static")
            .await
            .unwrap();

        let mut server = test_server();
        server.routes.push(RouteConfig {
            id: "rt_static".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/".to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: dir.display().to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        });

        let response =
            send_one_request(server, "GET /hello.txt HTTP/1.1\r\nHost: local\r\n\r\n").await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("hello static"));

        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn static_file_supports_etag_and_if_none_match() {
        let dir =
            std::env::temp_dir().join(format!("yiz-tunnel-cache-{}", Uuid::now_v7().simple()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("cache.txt"), b"cache body")
            .await
            .unwrap();

        let mut server = test_server();
        server.routes.push(RouteConfig {
            id: "rt_static".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/".to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: dir.display().to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        });

        let first = send_one_request(
            server.clone(),
            "GET /cache.txt HTTP/1.1\r\nHost: local\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(first.contains("ETag: "));
        assert!(first.contains("Last-Modified: "));
        let etag = response_header(&first, "ETag").unwrap();

        let second = send_one_request(
            server,
            &format!(
                "GET /cache.txt HTTP/1.1\r\nHost: local\r\nIf-None-Match: {etag}\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert!(second.starts_with("HTTP/1.1 304 Not Modified"));

        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn static_file_supports_single_byte_range() {
        let dir =
            std::env::temp_dir().join(format!("yiz-tunnel-range-{}", Uuid::now_v7().simple()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("range.txt"), b"0123456789")
            .await
            .unwrap();

        let mut server = test_server();
        server.routes.push(RouteConfig {
            id: "rt_static".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/".to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: dir.display().to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        });

        let response = send_one_request(
            server,
            "GET /range.txt HTTP/1.1\r\nHost: local\r\nRange: bytes=2-5\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 206 Partial Content"));
        assert!(response.contains("Content-Range: bytes 2-5/10"));
        assert!(response.ends_with("2345"));

        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[test]
    fn parses_byte_ranges() {
        assert_eq!(parse_byte_range("bytes=2-5", 10), Some((2, 5)));
        assert_eq!(parse_byte_range("bytes=7-", 10), Some((7, 9)));
        assert_eq!(parse_byte_range("bytes=-3", 10), Some((7, 9)));
        assert_eq!(parse_byte_range("bytes=20-30", 10), None);
        assert_eq!(parse_byte_range("bytes=6-2", 10), None);
    }

    #[tokio::test]
    async fn proxy_request_forwards_to_upstream() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let read = socket.read(&mut buffer).await.unwrap();
            let text = String::from_utf8_lossy(&buffer[..read]);
            assert!(text.contains("X-Real-IP: 127.0.0.1"));
            assert!(text.contains("X-Forwarded-For: 127.0.0.1"));
            assert!(text.contains("X-Forwarded-Proto: http"));
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nhello proxy",
                )
                .await
                .unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_test".to_string(),
            group: "api".to_string(),
            name: "v1".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let response =
            send_one_request(server, "GET /api/users HTTP/1.1\r\nHost: local\r\n\r\n").await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("hello proxy"));
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn proxy_rewrite_replace_prefix_reaches_upstream() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let request = read_test_http_message(&mut socket).await;
            assert!(request.starts_with("GET /a.png?x=1 HTTP/1.1\r\n"));
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_rewrite".to_string(),
            group: "api".to_string(),
            name: "v1".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy_rewrite".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/123456789012345/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: Some(ProxyRewrite {
                        r#type: "replacePrefix".to_string(),
                        from: "/123456789012345/".to_string(),
                        to: "/".to_string(),
                    }),
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let response = send_one_request(
            server,
            "GET /123456789012345/a.png?x=1 HTTP/1.1\r\nHost: local\r\n\r\n",
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("ok"));
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn proxy_tries_next_upstream_when_connect_fails() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nfallback",
                )
                .await
                .unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_down".to_string(),
            group: "api".to_string(),
            name: "down".to_string(),
            host: "http://127.0.0.1:9".to_string(),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.upstreams.push(UpstreamConfig {
            id: "up_fallback".to_string(),
            group: "api".to_string(),
            name: "fallback".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 10,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let response =
            send_one_request(server, "GET /api/users HTTP/1.1\r\nHost: local\r\n\r\n").await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("fallback"));
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn proxy_streams_response_before_upstream_closes() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhe")
                .await
                .unwrap();
            sleep(Duration::from_millis(200)).await;
            socket.write_all(b"llo").await.unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_stream".to_string(),
            group: "api".to_string(),
            name: "v1".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let config = Arc::new(RwLock::new(server));
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(
                stream,
                config,
                "127.0.0.1:50003".to_string(),
                ProxyState::default(),
            )
            .await
            .unwrap()
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET /api/stream HTTP/1.1\r\nHost: local\r\n\r\n")
            .await
            .unwrap();

        let mut first_chunk = [0_u8; 256];
        let read = timeout(Duration::from_millis(100), client.read(&mut first_chunk))
            .await
            .unwrap()
            .unwrap();
        let first_text = String::from_utf8_lossy(&first_chunk[..read]);
        assert!(first_text.contains("HTTP/1.1 200 OK"));
        assert!(first_text.ends_with("he"));

        let mut rest = Vec::new();
        client.read_to_end(&mut rest).await.unwrap();
        let mut full = first_chunk[..read].to_vec();
        full.extend_from_slice(&rest);
        assert!(String::from_utf8(full).unwrap().ends_with("hello"));

        server_task.await.unwrap();
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn proxy_streams_content_length_request_body_to_upstream() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let mut first = [0_u8; 1024];
            let first_read = timeout(Duration::from_millis(100), socket.read(&mut first))
                .await
                .unwrap()
                .unwrap();
            let first_text = String::from_utf8_lossy(&first[..first_read]);
            assert!(first_text.contains("Content-Length: 5"));
            assert!(first_text.ends_with("he"));

            let mut rest = [0_u8; 16];
            let rest_read = timeout(Duration::from_millis(500), socket.read(&mut rest))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(&rest[..rest_read], b"llo");

            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_stream_body".to_string(),
            group: "api".to_string(),
            name: "v1".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let config = Arc::new(RwLock::new(server));
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(
                stream,
                config,
                "127.0.0.1:50004".to_string(),
                ProxyState::default(),
            )
            .await
            .unwrap()
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"POST /api/body HTTP/1.1\r\nHost: local\r\nContent-Length: 5\r\n\r\nhe")
            .await
            .unwrap();
        sleep(Duration::from_millis(150)).await;
        client.write_all(b"llo").await.unwrap();
        client.shutdown().await.unwrap();

        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        assert!(String::from_utf8(response).unwrap().ends_with("ok"));

        server_task.await.unwrap();
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn proxy_round_robins_same_priority_upstreams() {
        let upstream_one = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_one_addr = upstream_one.local_addr().unwrap();
        let upstream_one_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_one.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\none")
                .await
                .unwrap();
        });

        let upstream_two = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_two_addr = upstream_two.local_addr().unwrap();
        let upstream_two_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_two.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\ntwo")
                .await
                .unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_one".to_string(),
            group: "api".to_string(),
            name: "one".to_string(),
            host: format!("http://{}", upstream_one_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.upstreams.push(UpstreamConfig {
            id: "up_two".to_string(),
            group: "api".to_string(),
            name: "two".to_string(),
            host: format!("http://{}", upstream_two_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.upstreams.push(UpstreamConfig {
            id: "up_lower".to_string(),
            group: "api".to_string(),
            name: "lower-priority".to_string(),
            host: "http://127.0.0.1:9".to_string(),
            priority: 10,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let proxy_state = ProxyState::default();
        let first = send_one_request_with_state(
            server.clone(),
            "GET /api/users HTTP/1.1\r\nHost: local\r\n\r\n",
            proxy_state.clone(),
        )
        .await;
        let second = send_one_request_with_state(
            server,
            "GET /api/users HTTP/1.1\r\nHost: local\r\n\r\n",
            proxy_state,
        )
        .await;

        assert!(first.ends_with("one"));
        assert!(second.ends_with("two"));
        upstream_one_task.await.unwrap();
        upstream_two_task.await.unwrap();
    }

    #[tokio::test]
    async fn proxy_decodes_chunked_request_body_before_forwarding() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let request = read_test_http_message(&mut socket).await;

            assert!(request.starts_with("POST /api/chunked HTTP/1.1"));
            assert!(request.contains("Content-Length: 11\r\n"));
            assert!(
                !request
                    .to_ascii_lowercase()
                    .contains("transfer-encoding: chunked")
            );
            assert!(request.ends_with("hello world"));

            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_chunked".to_string(),
            group: "api".to_string(),
            name: "v1".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let response = send_one_request(
            server,
            "POST /api/chunked HTTP/1.1\r\nHost: local\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n1\r\n \r\n5\r\nworld\r\n0\r\n\r\n",
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("ok"));
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn client_max_body_size_returns_413() {
        let mut server = test_server();
        server.conf = json!({
            "client_max_body_size": 3
        });
        server.routes.push(RouteConfig {
            id: "rt_static".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/".to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: ".".to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        });

        let response = send_one_request(
            server,
            "POST /upload HTTP/1.1\r\nHost: local\r\nContent-Length: 5\r\n\r\n12345",
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 413 Payload Too Large"));
    }

    #[tokio::test]
    async fn keepalive_requests_limit_closes_connection() {
        let dir =
            std::env::temp_dir().join(format!("yiz-tunnel-keep-limit-{}", Uuid::now_v7().simple()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("one.txt"), b"one").await.unwrap();

        let mut server = test_server();
        server.conf = json!({
            "keepalive_requests": 1
        });
        server.routes.push(RouteConfig {
            id: "rt_static".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/".to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: dir.display().to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        });

        let response =
            send_one_request(server, "GET /one.txt HTTP/1.1\r\nHost: local\r\n\r\n").await;
        assert!(response.contains("Connection: close"));
        assert!(response.ends_with("one"));

        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn proxy_read_timeout_returns_504() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nlate")
                .await;
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_slow".to_string(),
            group: "api".to_string(),
            name: "v1".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 0,
            conf: json!({
                "proxy_read_timeout": 20
            }),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let response =
            send_one_request(server, "GET /api/slow HTTP/1.1\r\nHost: local\r\n\r\n").await;
        assert!(response.starts_with("HTTP/1.1 504 Gateway Timeout"));
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn keep_alive_serves_two_static_requests_on_same_connection() {
        let dir = std::env::temp_dir().join(format!("yiz-tunnel-keep-{}", Uuid::now_v7().simple()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("one.txt"), b"one").await.unwrap();
        tokio::fs::write(dir.join("two.txt"), b"two").await.unwrap();

        let mut server = test_server();
        server.routes.push(RouteConfig {
            id: "rt_static".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/".to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: dir.display().to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let config = Arc::new(RwLock::new(server));
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(
                stream,
                config,
                "127.0.0.1:50001".to_string(),
                ProxyState::default(),
            )
            .await
            .unwrap()
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET /one.txt HTTP/1.1\r\nHost: local\r\n\r\n")
            .await
            .unwrap();
        let first = read_test_response(&mut client).await;
        assert!(first.contains("Connection: keep-alive"));
        assert!(first.ends_with("one"));

        client
            .write_all(b"GET /two.txt HTTP/1.1\r\nHost: local\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let second = read_test_response(&mut client).await;
        assert!(second.contains("Connection: close"));
        assert!(second.ends_with("two"));

        let outcomes = server_task.await.unwrap();
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].path, "/one.txt");
        assert_eq!(outcomes[1].path, "/two.txt");

        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn keep_alive_connection_reads_latest_config_per_request() {
        let dir =
            std::env::temp_dir().join(format!("yiz-tunnel-hot-config-{}", Uuid::now_v7().simple()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("one.txt"), b"one").await.unwrap();

        let mut server = test_server();
        server.routes.push(RouteConfig {
            id: "rt_static".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/".to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: dir.display().to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        });

        let config = Arc::new(RwLock::new(server));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_config = Arc::clone(&config);
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(
                stream,
                server_config,
                "127.0.0.1:50002".to_string(),
                ProxyState::default(),
            )
            .await
            .unwrap()
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET /one.txt HTTP/1.1\r\nHost: local\r\n\r\n")
            .await
            .unwrap();
        let first = read_test_response(&mut client).await;
        assert!(first.starts_with("HTTP/1.1 200 OK"));

        config.write().await.routes.clear();

        client
            .write_all(b"GET /one.txt HTTP/1.1\r\nHost: local\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let second = read_test_response(&mut client).await;
        assert!(second.starts_with("HTTP/1.1 404 Not Found"));

        let outcomes = server_task.await.unwrap();
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].status, 200);
        assert_eq!(outcomes[1].status, 404);

        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn stop_reports_stopping_until_active_connections_close() {
        let dir =
            std::env::temp_dir().join(format!("yiz-tunnel-graceful-{}", Uuid::now_v7().simple()));
        let public_dir = dir.join("public");
        let log_dir = dir.join("logs");
        tokio::fs::create_dir_all(&public_dir).await.unwrap();
        tokio::fs::write(public_dir.join("one.txt"), b"one")
            .await
            .unwrap();

        let mut server = test_server();
        server.id = "hs_graceful".to_string();
        server.listen.port = free_tcp_port();
        server.routes.push(RouteConfig {
            id: "rt_static".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/".to_string(),
            },
            action: RouteAction {
                r#type: "file".to_string(),
                file: Some(FileAction {
                    dir: public_dir.display().to_string(),
                    alias: 0,
                }),
                proxy: None,
            },
            conf: Value::Object(Default::default()),
        });

        let runtime = HttpRuntime::new(LogManager::new(log_dir).unwrap());
        runtime.apply(server.clone()).await.unwrap();

        let mut client = TcpStream::connect(format!("127.0.0.1:{}", server.listen.port))
            .await
            .unwrap();
        client
            .write_all(b"GET /one.txt HTTP/1.1\r\nHost: local\r\n\r\n")
            .await
            .unwrap();
        let response = read_test_response(&mut client).await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Connection: keep-alive"));

        for _ in 0..50 {
            if runtime.info(&server).unwrap().active_connection_count == 1 {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        runtime.stop(&server.id).unwrap();
        let stopping = runtime.info(&server).unwrap();
        assert_eq!(stopping.status, "stopping");
        assert_eq!(stopping.active_connection_count, 1);

        drop(client);

        for _ in 0..50 {
            if runtime.info(&server).unwrap().status == "stopped" {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        let stopped = runtime.info(&server).unwrap();
        assert_eq!(stopped.status, "stopped");
        assert_eq!(stopped.active_connection_count, 0);

        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn proxy_tracks_active_upstream_request_count() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_active".to_string(),
            group: "api".to_string(),
            name: "v1".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let proxy_state = ProxyState::default();
        let request_state = proxy_state.clone();
        let request_task = tokio::spawn(async move {
            send_one_request_with_state(
                server,
                "GET /api/active HTTP/1.1\r\nHost: local\r\n\r\n",
                request_state,
            )
            .await
        });

        for _ in 0..50 {
            if proxy_state.active_upstream_request_count("up_active") == 1 {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(proxy_state.active_upstream_request_count("up_active"), 1);

        let response = request_task.await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(proxy_state.active_upstream_request_count("up_active"), 0);
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn removed_active_upstream_is_reported_as_deading_then_dead() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream_listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
        });

        let mut server = test_server();
        server.upstreams.push(UpstreamConfig {
            id: "up_removed".to_string(),
            group: "api".to_string(),
            name: "v1".to_string(),
            host: format!("http://{}", upstream_addr),
            priority: 0,
            conf: Value::Object(Default::default()),
        });
        server.routes.push(RouteConfig {
            id: "rt_proxy".to_string(),
            match_rule: RouteMatch {
                r#type: 1,
                path: "/api/".to_string(),
            },
            action: RouteAction {
                r#type: "proxy".to_string(),
                file: None,
                proxy: Some(ProxyAction {
                    upstream: "api".to_string(),
                    websocket: WebSocketConfig { enabled: true },
                    rewrite: None,
                }),
            },
            conf: Value::Object(Default::default()),
        });

        let old_upstreams = server.upstreams.clone();
        let proxy_state = ProxyState::default();
        let request_state = proxy_state.clone();
        let request_task = tokio::spawn(async move {
            send_one_request_with_state(
                server,
                "GET /api/removed HTTP/1.1\r\nHost: local\r\n\r\n",
                request_state,
            )
            .await
        });

        for _ in 0..50 {
            if proxy_state.active_upstream_request_count("up_removed") == 1 {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
        proxy_state.reconcile_upstreams("hs_test", &old_upstreams, &[]);

        let deading = proxy_state
            .retired_upstream_info("hs_test", "up_removed")
            .unwrap();
        assert_eq!(deading.status, "deading");
        assert_eq!(deading.active_request_count, 1);

        let response = request_task.await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));

        let dead = proxy_state
            .retired_upstream_info("hs_test", "up_removed")
            .unwrap();
        assert_eq!(dead.status, "dead");
        assert_eq!(dead.active_request_count, 0);
        upstream_task.await.unwrap();
    }

    fn test_server() -> HttpServerConfig {
        HttpServerConfig {
            id: "hs_test".to_string(),
            alias: "test".to_string(),
            enabled: true,
            listen: ListenConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                server_name: vec!["localhost".to_string()],
            },
            graceful: GracefulConfig::default(),
            conf: Value::Object(Default::default()),
            upstreams: Vec::new(),
            routes: Vec::new(),
        }
    }

    fn free_tcp_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    async fn send_one_request(server: HttpServerConfig, request: &str) -> String {
        send_one_request_with_state(server, request, ProxyState::default()).await
    }

    async fn send_one_request_with_state(
        server: HttpServerConfig,
        request: &str,
        proxy_state: ProxyState,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let request = request.to_string();
        let config = Arc::new(RwLock::new(server));

        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, config, "127.0.0.1:50000".to_string(), proxy_state)
                .await
                .unwrap();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(request.as_bytes()).await.unwrap();
        client.shutdown().await.unwrap();

        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server_task.await.unwrap();
        String::from_utf8(response).unwrap()
    }

    async fn read_test_response(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let header_end;

        loop {
            let mut chunk = [0_u8; 256];
            let read = stream.read(&mut chunk).await.unwrap();
            assert!(read > 0);
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(index) = find_header_end(&buffer) {
                header_end = index;
                break;
            }
        }

        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let body_start = header_end + 4;
        while buffer.len() < body_start + content_length {
            let mut chunk = vec![0_u8; body_start + content_length - buffer.len()];
            let read = stream.read(&mut chunk).await.unwrap();
            assert!(read > 0);
            buffer.extend_from_slice(&chunk[..read]);
        }

        String::from_utf8(buffer[..body_start + content_length].to_vec()).unwrap()
    }

    async fn read_test_http_message(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let header_end;

        loop {
            let mut chunk = [0_u8; 256];
            let read = stream.read(&mut chunk).await.unwrap();
            assert!(read > 0);
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(index) = find_header_end(&buffer) {
                header_end = index;
                break;
            }
        }

        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let body_start = header_end + 4;
        while buffer.len() < body_start + content_length {
            let mut chunk = vec![0_u8; body_start + content_length - buffer.len()];
            let read = stream.read(&mut chunk).await.unwrap();
            assert!(read > 0);
            buffer.extend_from_slice(&chunk[..read]);
        }

        String::from_utf8(buffer[..body_start + content_length].to_vec()).unwrap()
    }

    fn response_header(response: &str, name: &str) -> Option<String> {
        response.lines().find_map(|line| {
            let (header_name, value) = line.split_once(':')?;
            if header_name.eq_ignore_ascii_case(name) {
                Some(value.trim().to_string())
            } else {
                None
            }
        })
    }
}
