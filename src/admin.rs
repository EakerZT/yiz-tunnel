use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::json;

use crate::error::{ApiResult, ok};
use crate::logger::LogManager;
use crate::model::{
    CreateHttpServerRequest, CreateRouteRequest, CreateUpstreamRequest, HttpServerInfo,
    SetEnabledRequest, SystemStatus, UpdateHttpServerRequest,
};
use crate::runtime::HttpRuntime;
use crate::storage::HttpServerStorage;

pub struct AppState {
    version: String,
    started_at: Instant,
    system_config_path: PathBuf,
    data_dir: PathBuf,
    log_dir: PathBuf,
    http_servers: HttpServerStorage,
    runtime: HttpRuntime,
    logger: LogManager,
}

impl AppState {
    pub fn new(
        version: String,
        started_at: Instant,
        system_config_path: PathBuf,
        data_dir: PathBuf,
        log_dir: PathBuf,
        http_servers: HttpServerStorage,
        runtime: HttpRuntime,
        logger: LogManager,
    ) -> Self {
        Self {
            version,
            started_at,
            system_config_path,
            data_dir,
            log_dir,
            http_servers,
            runtime,
            logger,
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/system/status", get(system_status))
        .route(
            "/api/v1/http-servers",
            get(list_http_servers).post(create_http_server),
        )
        .route(
            "/api/v1/http-server/{id}",
            get(get_http_server)
                .put(update_http_server)
                .delete(delete_http_server),
        )
        .route(
            "/api/v1/http-server/{id}/enabled",
            put(set_http_server_enabled),
        )
        .route("/api/v1/http-server/{id}/info", get(get_http_server_info))
        .route("/api/v1/http-server/{id}/reload", post(reload_http_server))
        .route(
            "/api/v1/http-server/{id}/upstreams",
            get(list_upstreams).post(add_upstream),
        )
        .route(
            "/api/v1/http-server/{id}/upstream/{upstream_id}",
            get(get_upstream).delete(delete_upstream),
        )
        .route(
            "/api/v1/http-server/{id}/routes",
            get(list_routes).post(add_route),
        )
        .route(
            "/api/v1/http-server/{id}/route/{route_id}",
            get(get_route).delete(delete_route),
        )
        .with_state(state)
}

async fn system_status(State(state): State<Arc<AppState>>) -> ApiResult<SystemStatus> {
    let status = SystemStatus {
        version: state.version.clone(),
        uptime: state.started_at.elapsed().as_secs(),
        system_config_path: state.system_config_path.display().to_string(),
        data_dir: state.data_dir.display().to_string(),
        log_dir: state.log_dir.display().to_string(),
        http_server_count: state.http_servers.count()?,
        tcp_forward_count: 0,
        active_connection_count: state.runtime.active_connection_count()?,
        last_error: state.runtime.last_error()?,
    };

    Ok(ok(status))
}

async fn list_http_servers(State(state): State<Arc<AppState>>) -> ApiResult<serde_json::Value> {
    Ok(ok(json!(state.http_servers.list()?)))
}

async fn create_http_server(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateHttpServerRequest>,
) -> ApiResult<serde_json::Value> {
    let server = state.http_servers.create(request)?;
    state.runtime.apply(server.clone()).await?;
    state
        .logger
        .admin("create", "http-server", &server.id, "ok", "");
    Ok(ok(json!(server)))
}

async fn get_http_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    Ok(ok(json!(state.http_servers.get(&id)?)))
}

async fn update_http_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateHttpServerRequest>,
) -> ApiResult<serde_json::Value> {
    let server = state.http_servers.update(&id, request)?;
    if server.enabled {
        state.runtime.apply(server.clone()).await?;
    }
    state
        .logger
        .admin("update", "http-server", &server.id, "ok", "");
    Ok(ok(json!(server)))
}

async fn set_http_server_enabled(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<SetEnabledRequest>,
) -> ApiResult<serde_json::Value> {
    let server = state.http_servers.set_enabled(&id, request)?;
    if server.enabled {
        state.runtime.apply(server.clone()).await?;
    } else {
        state.runtime.stop(&server.id)?;
    }
    state
        .logger
        .admin("set-enabled", "http-server", &server.id, "ok", "");
    Ok(ok(json!(server)))
}

async fn delete_http_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let server = state.http_servers.delete(&id)?;
    state.runtime.stop(&server.id)?;
    state
        .logger
        .admin("delete", "http-server", &server.id, "ok", "");
    Ok(ok(json!(server)))
}

async fn get_http_server_info(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<HttpServerInfo> {
    let server = state.http_servers.get(&id)?;
    Ok(ok(state.runtime.info(&server)?))
}

async fn reload_http_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let server = state.http_servers.get(&id)?;
    if server.enabled {
        state.runtime.apply(server.clone()).await?;
    }
    state
        .logger
        .admin("reload", "http-server", &server.id, "ok", "");
    Ok(ok(json!({
        "id": server.id,
        "reloaded": true
    })))
}

async fn list_upstreams(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let upstreams = state
        .http_servers
        .list_upstreams(&id)?
        .into_iter()
        .map(|upstream| state.runtime.upstream_info(&upstream))
        .chain(state.runtime.retired_upstream_infos(&id))
        .collect::<Vec<_>>();
    Ok(ok(json!(upstreams)))
}

async fn add_upstream(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<CreateUpstreamRequest>,
) -> ApiResult<serde_json::Value> {
    let upstream = state.http_servers.add_upstream(&id, request)?;
    let server = state.http_servers.get(&id)?;
    if server.enabled {
        state.runtime.apply(server).await?;
    }
    state
        .logger
        .admin("create", "upstream", &upstream.id, "ok", "");
    Ok(ok(json!(upstream)))
}

async fn get_upstream(
    State(state): State<Arc<AppState>>,
    Path((id, upstream_id)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    match state.http_servers.get_upstream(&id, &upstream_id) {
        Ok(upstream) => Ok(ok(json!(state.runtime.upstream_info(&upstream)))),
        Err(err) if err.code == 10002 => {
            let Some(info) = state.runtime.retired_upstream_info(&id, &upstream_id) else {
                return Err(err);
            };
            Ok(ok(json!(info)))
        }
        Err(err) => Err(err),
    }
}

async fn delete_upstream(
    State(state): State<Arc<AppState>>,
    Path((id, upstream_id)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let upstream = state.http_servers.delete_upstream(&id, &upstream_id)?;
    let server = state.http_servers.get(&id)?;
    if server.enabled {
        state.runtime.apply(server).await?;
    }
    state
        .logger
        .admin("delete", "upstream", &upstream.id, "ok", "");
    Ok(ok(json!(upstream)))
}

async fn list_routes(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    Ok(ok(json!(state.http_servers.list_routes(&id)?)))
}

async fn add_route(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<CreateRouteRequest>,
) -> ApiResult<serde_json::Value> {
    let route = state.http_servers.add_route(&id, request)?;
    let server = state.http_servers.get(&id)?;
    if server.enabled {
        state.runtime.apply(server).await?;
    }
    state.logger.admin("create", "route", &route.id, "ok", "");
    Ok(ok(json!(route)))
}

async fn get_route(
    State(state): State<Arc<AppState>>,
    Path((id, route_id)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    Ok(ok(json!(state.http_servers.get_route(&id, &route_id)?)))
}

async fn delete_route(
    State(state): State<Arc<AppState>>,
    Path((id, route_id)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let route = state.http_servers.delete_route(&id, &route_id)?;
    let server = state.http_servers.get(&id)?;
    if server.enabled {
        state.runtime.apply(server).await?;
    }
    state.logger.admin("delete", "route", &route.id, "ok", "");
    Ok(ok(json!(route)))
}
