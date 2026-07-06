use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug)]
pub struct CliError(String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SystemConfig {
    pub version: u32,
    #[serde(rename = "data-dir")]
    pub data_dir: PathBuf,
    #[serde(rename = "log-dir")]
    pub log_dir: PathBuf,
    pub admin: AdminConfig,
    #[serde(default)]
    pub runtime: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdminConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug)]
pub struct LoadedSystemConfig {
    pub config: SystemConfig,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            version: 1,
            data_dir: PathBuf::from("./data"),
            log_dir: PathBuf::from("./logs"),
            admin: AdminConfig {
                host: "127.0.0.1".to_string(),
                port: 9000,
            },
            runtime: Value::Object(Default::default()),
        }
    }
}

pub fn parse_config_path<I>(args: I) -> Result<PathBuf, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut config_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" => {
                let value = args
                    .next()
                    .ok_or_else(|| CliError("missing path after -c".to_string()))?;
                config_path = Some(PathBuf::from(value));
            }
            "-h" | "--help" => {
                return Err(CliError(
                    "usage: yiz-tunnel [-c path/to/yiz-tunnel.json]".to_string(),
                ));
            }
            other => {
                return Err(CliError(format!("unknown argument: {other}")));
            }
        }
    }

    match config_path {
        Some(path) => Ok(path),
        None => Ok(env::current_dir()
            .map_err(|err| CliError(err.to_string()))?
            .join("yiz-tunnel.json")),
    }
}

pub fn load_or_create_system_config(path: &Path) -> std::io::Result<LoadedSystemConfig> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let content = serde_json::to_string_pretty(&SystemConfig::default())?;
        fs::write(path, content)?;
    }

    let content = fs::read_to_string(path)?;
    let config: SystemConfig = serde_json::from_str(&content)?;
    validate_system_config(&config)?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let data_dir = resolve_config_path(base_dir, &config.data_dir);
    let log_dir = resolve_config_path(base_dir, &config.log_dir);

    Ok(LoadedSystemConfig {
        config,
        data_dir,
        log_dir,
    })
}

fn resolve_config_path(base_dir: &Path, value: &Path) -> PathBuf {
    if value.is_absolute() {
        value.to_path_buf()
    } else {
        base_dir.join(value)
    }
}

fn validate_system_config(config: &SystemConfig) -> std::io::Result<()> {
    if config.version != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported system config version: {}", config.version),
        ));
    }

    if config.data_dir.as_os_str().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "data-dir must not be empty",
        ));
    }

    if config.log_dir.as_os_str().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "log-dir must not be empty",
        ));
    }

    if config.admin.host.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "admin.host must not be empty",
        ));
    }

    if config.admin.port == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "admin.port must not be 0",
        ));
    }

    Ok(())
}

#[allow(dead_code)]
fn _os_string_debug(value: OsString) -> String {
    value.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_system_config_rejects_unsupported_version() {
        let mut config = SystemConfig::default();
        config.version = 2;

        assert!(validate_system_config(&config).is_err());
    }

    #[test]
    fn validate_system_config_rejects_empty_admin_host() {
        let mut config = SystemConfig::default();
        config.admin.host = " ".to_string();

        assert!(validate_system_config(&config).is_err());
    }
}
