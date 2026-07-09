use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::config::{MinerSettings, Network, TnBackend};
use crate::data_dir::DataLayout;
use crate::paths::BinaryPaths;

#[derive(Debug)]
pub struct Supervisor {
    layout: DataLayout,
    settings: MinerSettings,
    node_private_key_b64: String,
    binaries: Option<BinaryPaths>,
    bin_dir_override: Option<std::path::PathBuf>,
    core_child: Option<Child>,
    node_child: Option<Child>,
    mining_active: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MiningStatus {
    pub mining: bool,
    pub core_running: bool,
    pub node_running: bool,
    pub network: Network,
    pub tn_backend: TnBackend,
    pub message: Option<String>,
}

impl Supervisor {
    pub fn new(
        layout: DataLayout,
        settings: MinerSettings,
        node_private_key_b64: String,
        binaries: Option<BinaryPaths>,
        bin_dir_override: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            layout,
            settings,
            node_private_key_b64,
            binaries,
            bin_dir_override,
            core_child: None,
            node_child: None,
            mining_active: false,
        }
    }

    fn ensure_binaries(&mut self) -> anyhow::Result<()> {
        if self.binaries.is_some() {
            return Ok(());
        }
        self.binaries = Some(
            BinaryPaths::resolve(self.bin_dir_override.as_deref())
                .context("wqc-core / wqc-node not found — set --bin-dir or WQC_MINER_BIN_DIR")?,
        );
        Ok(())
    }

    pub fn reload_settings(&mut self, settings: MinerSettings) {
        self.settings = settings;
    }

    pub fn status(&self) -> MiningStatus {
        let (core_running, node_running, message) = if self.settings.is_mainnet_mock() {
            (
                false,
                false,
                self.mining_active.then(|| {
                    "Mainnet (mock): wallet saved locally; P2P not connected yet".to_string()
                }),
            )
        } else {
            (
                self.mining_active && self.core_child.is_some(),
                self.mining_active && self.node_child.is_some(),
                None,
            )
        };

        MiningStatus {
            mining: self.mining_active,
            core_running,
            node_running,
            network: self.settings.network,
            tn_backend: self.settings.tn_backend,
            message,
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<MiningStatus> {
        if self.core_child.is_some() || self.node_child.is_some() {
            bail!("mining is already running or stopping — try again");
        }
        if self.mining_active {
            bail!("mining is already running");
        }
        self.settings.validate_for_start()?;

        if self.settings.is_mainnet_mock() {
            tracing::info!(
                wallet = %self.settings.wallet_address,
                "mainnet mock start — skipping core/node spawn"
            );
            self.mining_active = true;
            return Ok(self.status());
        }

        self.start_core().await?;
        self.wait_for_core_health().await?;
        self.start_node().await?;
        self.mining_active = true;

        Ok(self.status())
    }

    pub async fn stop(&mut self) -> anyhow::Result<MiningStatus> {
        if let Some(mut child) = self.node_child.take() {
            stop_child(&mut child).await;
        }
        if let Some(mut child) = self.core_child.take() {
            stop_child(&mut child).await;
        }
        self.mining_active = false;
        Ok(self.status())
    }

    pub async fn node_status_json(&self) -> anyhow::Result<serde_json::Value> {
        if self.settings.is_mainnet_mock() {
            if !self.mining_active {
                bail!("mainnet mock is not running");
            }
            return Ok(serde_json::json!({
                "network": "mainnet",
                "mode": "mock",
                "wallet_address": self.settings.wallet_address,
                "status": "mock_running",
                "message": "Mainnet P2P integration is not wired yet"
            }));
        }

        if self.node_child.is_none() {
            bail!("wqc-node is not running");
        }
        let url = format!("http://127.0.0.1:{}/status", self.settings.node_http_port);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        let response = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let body = response
            .json::<serde_json::Value>()
            .await
            .context("parse wqc-node /status JSON")?;
        Ok(body)
    }

    /// Live TN backend probe from wqc-core `/sysinfo` (when core is running).
    pub async fn core_tn_status(&self) -> Option<serde_json::Value> {
        self.core_child.as_ref()?;

        let url = core_http_url(&self.settings, "/sysinfo");
        let client = build_core_client(&self.layout).ok()?;
        let response = client.get(&url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let body = response.json::<serde_json::Value>().await.ok()?;
        Some(serde_json::json!({
            "requested": body.get("tn_backend_requested").and_then(|v| v.as_str()).unwrap_or("cpu"),
            "active": body.get("tn_backend_active").and_then(|v| v.as_str()).unwrap_or("cpu"),
            "note": body.get("tn_backend_note").and_then(|v| v.as_str()),
        }))
    }

    async fn start_core(&mut self) -> anyhow::Result<()> {
        self.ensure_binaries()?;
        let core_bin = self
            .binaries
            .as_ref()
            .expect("binaries ensured")
            .core
            .clone();
        let socket_path = self.layout.core_socket_path();
        if cfg!(unix) && socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        let mut cmd = Command::new(&core_bin);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env(
                "WQC_MAX_MEMORY_GB",
                format!("{}", self.settings.max_memory_gb),
            )
            .env("WQC_TN_BACKEND", self.settings.tn_backend.as_env());

        if cfg!(unix) {
            cmd.env("WQC_CONNECTION_MODE", "uds");
            cmd.env("WQC_SOCKET_PATH", &socket_path);
        } else {
            cmd.env("WQC_CONNECTION_MODE", "tcp");
            cmd.env("WQC_CORE_TCP_PORT", self.settings.core_tcp_port.to_string());
        }

        tracing::info!("starting wqc-core: {}", core_bin.display());
        let mut child = cmd.spawn().context("spawn wqc-core")?;
        self.spawn_log_task(
            "core",
            child.stdout.take(),
            child.stderr.take(),
            self.layout.core_log_path(),
        );
        self.core_child = Some(child);
        Ok(())
    }

    async fn start_node(&mut self) -> anyhow::Result<()> {
        self.ensure_binaries()?;
        let node_bin = self
            .binaries
            .as_ref()
            .expect("binaries ensured")
            .node
            .clone();
        let db_url = format!("sqlite:{}", self.layout.node_db_path().display());
        let mut cmd = Command::new(&node_bin);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("WQC_NODE_PRIVATE_KEY", &self.node_private_key_b64)
            .env("WQC_TESTNET_NODE_KEY", &self.settings.node_key)
            .env("WQC_CORE_URL", self.settings.core_url(&self.layout))
            .env("WQC_BOOTSTRAP_URLS", self.settings.bootstrap_urls_env())
            .env(
                "WQC_MAX_MEMORY_GB",
                format!("{}", self.settings.max_memory_gb),
            )
            .env(
                "WQC_P2P_LISTEN_PORT",
                self.settings.p2p_listen_port.to_string(),
            )
            .env("WQC_HTTP_PORT", self.settings.node_http_port.to_string())
            .env("WQC_DATABASE_URL", db_url);

        tracing::info!("starting wqc-node: {}", node_bin.display());
        let mut child = cmd.spawn().context("spawn wqc-node")?;
        self.spawn_log_task(
            "node",
            child.stdout.take(),
            child.stderr.take(),
            self.layout.node_log_path(),
        );
        self.node_child = Some(child);
        Ok(())
    }

    async fn wait_for_core_health(&self) -> anyhow::Result<()> {
        let health_url = core_http_url(&self.settings, "/health");
        let client = build_core_client(&self.layout)?;

        for attempt in 1..=30 {
            let result = client.get(&health_url).send().await;
            match result {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!("wqc-core health check OK");
                    return Ok(());
                }
                Ok(resp) => {
                    tracing::debug!(
                        attempt,
                        status = %resp.status(),
                        "wqc-core not ready yet"
                    );
                }
                Err(err) => {
                    tracing::debug!(attempt, error = %err, "wqc-core health check failed");
                }
            }
            sleep(Duration::from_secs(1)).await;
        }
        bail!("wqc-core did not become healthy within 30 seconds");
    }

    fn spawn_log_task(
        &self,
        name: &str,
        stdout: Option<tokio::process::ChildStdout>,
        stderr: Option<tokio::process::ChildStderr>,
        path: std::path::PathBuf,
    ) {
        let name = name.to_string();
        // Ignore errors in log task; they are best-effort.
        tokio::spawn(async move {
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let file = match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!(
                        target: "wqc-miner",
                        "failed to open log file {} for {}: {}",
                        path.display(),
                        name,
                        e
                    );
                    return;
                }
            };
            let mut writer = file;

            if let Some(out) = stdout {
                let mut reader = BufReader::new(out).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = tokio::io::AsyncWriteExt::write_all(
                        &mut writer,
                        format!("[{}][stdout] {}\n", name, line).as_bytes(),
                    )
                    .await;
                }
            }
            if let Some(err) = stderr {
                let mut reader = BufReader::new(err).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = tokio::io::AsyncWriteExt::write_all(
                        &mut writer,
                        format!("[{}][stderr] {}\n", name, line).as_bytes(),
                    )
                    .await;
                }
            }
        });
    }
}

fn core_http_url(settings: &MinerSettings, path: &str) -> String {
    if cfg!(unix) {
        format!("http://localhost{path}")
    } else {
        format!("http://127.0.0.1:{}{path}", settings.core_tcp_port)
    }
}

#[cfg(unix)]
fn build_core_client(layout: &DataLayout) -> anyhow::Result<reqwest::Client> {
    let socket_path = layout.core_socket_path().to_string_lossy().to_string();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .unix_socket(socket_path)
        .build()
        .context("build unix-socket HTTP client for wqc-core")
}

#[cfg(not(unix))]
fn build_core_client(_layout: &DataLayout) -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .context("build HTTP client for wqc-core")
}

async fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = child.kill().await;
    }

    let _ = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
    if child.id().is_some() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}
