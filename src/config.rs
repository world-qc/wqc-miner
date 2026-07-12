use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::data_dir::DataLayout;
use crate::memory_budget::host_memory_limits_gib;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    Testnet,
    Mainnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TnBackend {
    #[default]
    Cpu,
    WebGpu,
}

impl TnBackend {
    pub fn as_env(&self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::WebGpu => "webgpu",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinerSettings {
    /// Active network (testnet uses node key; mainnet uses wallet address).
    #[serde(default)]
    pub network: Network,

    /// Local admin UI port.
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,

    /// Orchestrator bootstrap URL(s), comma-separated in env for wqc-node.
    #[serde(default = "default_bootstrap_urls")]
    pub bootstrap_urls: Vec<String>,

    /// Dashboard-issued operator node key (testnet). Empty until user pastes it.
    #[serde(default, alias = "testnet_node_key")]
    pub node_key: String,

    /// Payout wallet address (mainnet). Mock-only for now.
    #[serde(default)]
    pub wallet_address: String,

    /// Tensor-network execution backend for wqc-core (`WQC_TN_BACKEND`).
    #[serde(default)]
    pub tn_backend: TnBackend,

    /// Memory cap passed to wqc-core via WQC_MAX_MEMORY_GB.
    #[serde(default = "default_max_memory_gb")]
    pub max_memory_gb: f64,

    /// libp2p listen port for wqc-node.
    #[serde(default = "default_p2p_listen_port")]
    pub p2p_listen_port: u16,

    /// HTTP API port for wqc-node status endpoint.
    #[serde(default = "default_node_http_port")]
    pub node_http_port: u16,

    /// TCP port for wqc-core when WQC_CONNECTION_MODE=tcp (Windows).
    #[serde(default = "default_core_tcp_port")]
    pub core_tcp_port: u16,

    /// When true, start mining as soon as the launcher is ready (headless / systemd).
    /// Can also be forced with `--auto-start`. Admin UI still binds on localhost.
    #[serde(default)]
    pub auto_start: bool,
}

impl Default for MinerSettings {
    fn default() -> Self {
        Self {
            network: Network::default(),
            admin_port: default_admin_port(),
            bootstrap_urls: default_bootstrap_urls(),
            node_key: String::new(),
            wallet_address: String::new(),
            tn_backend: TnBackend::default(),
            max_memory_gb: default_max_memory_gb(),
            p2p_listen_port: default_p2p_listen_port(),
            node_http_port: default_node_http_port(),
            core_tcp_port: default_core_tcp_port(),
            auto_start: false,
        }
    }
}

fn default_admin_port() -> u16 {
    3000
}

fn default_bootstrap_urls() -> Vec<String> {
    default_bootstrap_urls_for(&Network::Testnet)
}

pub fn default_bootstrap_urls_for(network: &Network) -> Vec<String> {
    match network {
        Network::Testnet => vec!["https://testnet.world-qc.io/api/v1/p2p/bootstrap".to_string()],
        Network::Mainnet => vec!["https://world-qc.io/api/v1/p2p/bootstrap".to_string()],
    }
}

fn default_max_memory_gb() -> f64 {
    // Default: total host RAM minus reserve (1 GiB if <16 GiB, else 2 GiB).
    // Falls back to 16.0 if host memory cannot be detected.
    let (_total_gib, max_gib) = host_memory_limits_gib();
    max_gib.map(|gib| gib as f64).unwrap_or(16.0)
}

fn default_p2p_listen_port() -> u16 {
    4002
}

fn default_node_http_port() -> u16 {
    8080
}

fn default_core_tcp_port() -> u16 {
    13000
}

impl MinerSettings {
    pub fn load(layout: &DataLayout) -> anyhow::Result<Self> {
        let path = layout.settings_path();
        if !path.exists() {
            let settings = Self::default();
            settings.save(layout)?;
            return Ok(settings);
        }
        let raw =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let settings: Self =
            toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
        Ok(settings)
    }

    pub fn save(&self, layout: &DataLayout) -> anyhow::Result<()> {
        let path = layout.settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(&path, raw)?;
        Ok(())
    }

    pub fn bootstrap_urls_env(&self) -> String {
        self.bootstrap_urls.join(",")
    }

    pub fn core_url(&self, layout: &DataLayout) -> String {
        if cfg!(unix) {
            format!("unix:{}", layout.core_socket_path().display())
        } else {
            format!("http://127.0.0.1:{}", self.core_tcp_port)
        }
    }

    pub fn is_mainnet_mock(&self) -> bool {
        self.network == Network::Mainnet
    }
}

/// Partial update from the admin API (all fields optional).
#[derive(Debug, Deserialize)]
pub struct SettingsUpdate {
    pub network: Option<Network>,
    pub admin_port: Option<u16>,
    pub bootstrap_urls: Option<Vec<String>>,
    #[serde(alias = "testnet_node_key")]
    pub node_key: Option<String>,
    pub wallet_address: Option<String>,
    pub tn_backend: Option<TnBackend>,
    pub max_memory_gb: Option<f64>,
    pub p2p_listen_port: Option<u16>,
    pub node_http_port: Option<u16>,
    pub core_tcp_port: Option<u16>,
    pub auto_start: Option<bool>,
}

impl SettingsUpdate {
    pub fn apply(self, settings: &mut MinerSettings) {
        if let Some(v) = self.network {
            settings.network = v;
        }
        if let Some(v) = self.admin_port {
            settings.admin_port = v;
        }
        if let Some(v) = self.bootstrap_urls {
            settings.bootstrap_urls = v;
        }
        if let Some(v) = self.node_key {
            settings.node_key = v;
        }
        if let Some(v) = self.wallet_address {
            settings.wallet_address = v;
        }
        if let Some(v) = self.tn_backend {
            settings.tn_backend = v;
        }
        if let Some(mut v) = self.max_memory_gb {
            let (_total_gib, max_gib) = host_memory_limits_gib();
            if let Some(max) = max_gib {
                let max_f = max as f64;
                if v > max_f {
                    v = max_f;
                }
            }
            if v < 1.0 {
                v = 1.0;
            }
            settings.max_memory_gb = v;
        }
        if let Some(v) = self.p2p_listen_port {
            settings.p2p_listen_port = v;
        }
        if let Some(v) = self.node_http_port {
            settings.node_http_port = v;
        }
        if let Some(v) = self.core_tcp_port {
            settings.core_tcp_port = v;
        }
        if let Some(v) = self.auto_start {
            settings.auto_start = v;
        }
    }
}

/// Settings exposed to the UI (secrets masked when set).
#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub network: Network,
    pub admin_port: u16,
    pub bootstrap_urls: Vec<String>,
    pub node_key_set: bool,
    pub wallet_address_set: bool,
    pub tn_backend: TnBackend,
    pub max_memory_gb: f64,
    pub p2p_listen_port: u16,
    pub node_http_port: u16,
    pub core_tcp_port: u16,
    pub auto_start: bool,
    pub core_url: String,
    pub data_dir: String,
    pub mainnet_mock: bool,
    pub host_total_memory_gb: Option<u64>,
    pub host_max_memory_gb: Option<u64>,
}

impl SettingsView {
    pub fn from_settings(settings: &MinerSettings, layout: &DataLayout) -> Self {
        let (host_total_gib, host_max_gib) = host_memory_limits_gib();
        Self {
            network: settings.network,
            admin_port: settings.admin_port,
            bootstrap_urls: settings.bootstrap_urls.clone(),
            node_key_set: !settings.node_key.trim().is_empty(),
            wallet_address_set: !settings.wallet_address.trim().is_empty(),
            tn_backend: settings.tn_backend,
            max_memory_gb: settings.max_memory_gb,
            p2p_listen_port: settings.p2p_listen_port,
            node_http_port: settings.node_http_port,
            core_tcp_port: settings.core_tcp_port,
            auto_start: settings.auto_start,
            core_url: settings.core_url(layout),
            data_dir: layout.root.display().to_string(),
            mainnet_mock: settings.is_mainnet_mock(),
            host_total_memory_gb: host_total_gib,
            host_max_memory_gb: host_max_gib,
        }
    }
}
