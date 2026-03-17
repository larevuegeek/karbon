use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct KarbonConfig {
    pub app: AppConfig,
    pub backend: BackendConfig,
    pub frontend: FrontendConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub deploy: Option<DeployConfig>,
}

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct BackendConfig {
    pub package: String,
    #[serde(default = "default_backend_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub watch: bool,
}

#[derive(Debug, Deserialize)]
pub struct FrontendConfig {
    pub dir: String,
    #[serde(default = "default_frontend_port")]
    pub port: u16,
    #[serde(default = "default_dev_cmd")]
    pub dev_cmd: String,
    #[serde(default = "default_build_cmd")]
    pub build_cmd: String,
    #[serde(default = "default_serve_cmd")]
    pub serve_cmd: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ProxyConfig {
    #[serde(default = "default_backend_routes")]
    pub backend_routes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeployConfig {
    /// Deployment path (e.g. /var/www/my-app)
    pub path: String,
    /// System user to own the deployed files and run the app (e.g. "antispammeur")
    pub user: Option<String>,
    /// Process manager: "pm2" or "systemd"
    #[serde(default = "default_manager")]
    pub manager: String,
    /// PM2 config file name (default: ecosystem.config.cjs)
    #[serde(default = "default_pm2_config")]
    pub pm2_config: String,
    /// Systemd service name (default: app name)
    pub service: Option<String>,
    /// SSH host (e.g. user@server). If absent → local deploy.
    pub host: Option<String>,
}

fn default_manager() -> String { "pm2".to_string() }
fn default_pm2_config() -> String { "ecosystem.config.cjs".to_string() }

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            backend_routes: default_backend_routes(),
        }
    }
}

fn default_backend_port() -> u16 { 3005 }
fn default_frontend_port() -> u16 { 3004 }
fn default_true() -> bool { true }
fn default_dev_cmd() -> String { "npm run dev".to_string() }
fn default_build_cmd() -> String { "npm run build".to_string() }
fn default_serve_cmd() -> String { "node build/index.js".to_string() }
fn default_backend_routes() -> Vec<String> {
    vec!["/api".to_string(), "/files".to_string(), "/health".to_string()]
}

impl KarbonConfig {
    /// Load karbon.toml from the project root (searches upward)
    pub fn load() -> Result<(Self, PathBuf), String> {
        let mut dir = std::env::current_dir()
            .map_err(|e| format!("Cannot get current dir: {e}"))?;

        loop {
            let config_path = dir.join("karbon.toml");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)
                    .map_err(|e| format!("Cannot read {}: {e}", config_path.display()))?;
                let config: KarbonConfig = toml::from_str(&content)
                    .map_err(|e| format!("Invalid karbon.toml: {e}"))?;
                return Ok((config, dir));
            }
            if !dir.pop() {
                return Err("karbon.toml not found (searched from current dir upward)".to_string());
            }
        }
    }

    /// Absolute path to the frontend directory
    pub fn frontend_dir(&self, root: &Path) -> PathBuf {
        root.join(&self.frontend.dir)
    }
}
