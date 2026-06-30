mod admin;
mod config;
mod error;
mod logger;
mod model;
mod runtime;
mod storage;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use admin::AppState;
use config::{load_or_create_system_config, parse_config_path};
use logger::LogManager;
use runtime::HttpRuntime;
use storage::HttpServerStorage;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("yiz-tunnel failed: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config_path = parse_config_path(std::env::args().skip(1))?;
    let loaded_config = load_or_create_system_config(&config_path)?;

    std::fs::create_dir_all(&loaded_config.data_dir)?;
    std::fs::create_dir_all(&loaded_config.log_dir)?;

    let http_server_path = loaded_config.data_dir.join("http-server.json");
    let http_servers = HttpServerStorage::load_or_empty(http_server_path)?;
    let logger = LogManager::new(loaded_config.log_dir.clone())?;
    let runtime = HttpRuntime::new(logger.clone());

    for server in http_servers.list()? {
        if server.enabled {
            if let Err(err) = runtime.apply(server).await {
                eprintln!("failed to start configured http-server: {}", err.message);
            }
        }
    }

    let admin_addr: SocketAddr = format!(
        "{}:{}",
        loaded_config.config.admin.host, loaded_config.config.admin.port
    )
    .parse()?;

    let state = Arc::new(AppState::new(
        env!("CARGO_PKG_VERSION").to_string(),
        Instant::now(),
        config_path,
        loaded_config.data_dir,
        loaded_config.log_dir,
        http_servers,
        runtime,
        logger,
    ));

    let app = admin::router(state);
    let listener = TcpListener::bind(admin_addr).await?;

    println!("yiz-tunnel admin listening on http://{admin_addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    Ok(())
}
