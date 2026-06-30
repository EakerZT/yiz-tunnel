use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

#[derive(Clone)]
pub struct LogManager {
    inner: Arc<LogInner>,
}

struct LogInner {
    access_path: PathBuf,
    error_path: PathBuf,
    admin_path: PathBuf,
    lock: Mutex<()>,
}

impl LogManager {
    pub fn new(log_dir: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&log_dir)?;
        Ok(Self {
            inner: Arc::new(LogInner {
                access_path: log_dir.join("access.log"),
                error_path: log_dir.join("error.log"),
                admin_path: log_dir.join("admin.log"),
                lock: Mutex::new(()),
            }),
        })
    }

    pub fn access(&self, entry: AccessLogEntry) {
        let value = json!({
            "time": unix_millis(),
            "remoteAddress": entry.remote_address,
            "httpServerId": entry.http_server_id,
            "httpServerAlias": entry.http_server_alias,
            "method": entry.method,
            "path": entry.path,
            "status": entry.status,
            "responseTime": entry.response_time_ms,
            "upstreamId": entry.upstream_id,
            "upstreamName": entry.upstream_name,
        });
        let _ = self.write(&self.inner.access_path, value);
    }

    pub fn error(
        &self,
        level: impl Into<String>,
        module: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        let value = json!({
            "time": unix_millis(),
            "level": level.into(),
            "module": module.into(),
            "message": message.into(),
            "detail": detail,
        });
        let _ = self.write(&self.inner.error_path, value);
    }

    pub fn admin(
        &self,
        operation: impl Into<String>,
        target_type: impl Into<String>,
        target_id: impl Into<String>,
        result: impl Into<String>,
        message: impl Into<String>,
    ) {
        let value = json!({
            "time": unix_millis(),
            "operation": operation.into(),
            "targetType": target_type.into(),
            "targetId": target_id.into(),
            "result": result.into(),
            "message": message.into(),
        });
        let _ = self.write(&self.inner.admin_path, value);
    }

    fn write(&self, path: &PathBuf, value: serde_json::Value) -> std::io::Result<()> {
        let _guard = self.inner.lock.lock().expect("log lock poisoned");
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{}", value)
    }
}

pub struct AccessLogEntry {
    pub remote_address: String,
    pub http_server_id: String,
    pub http_server_alias: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub response_time_ms: u128,
    pub upstream_id: Option<String>,
    pub upstream_name: Option<String>,
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn writes_admin_log_line() {
        let dir = std::env::temp_dir().join(format!("yiz-tunnel-log-{}", Uuid::now_v7().simple()));
        let logger = LogManager::new(dir.clone()).unwrap();
        logger.admin("create", "http-server", "hs_test", "ok", "");

        let content = std::fs::read_to_string(dir.join("admin.log")).unwrap();
        assert!(content.contains("\"operation\":\"create\""));
        assert!(content.contains("\"targetId\":\"hs_test\""));

        let _ = std::fs::remove_dir_all(dir);
    }
}
