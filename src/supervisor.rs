use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::sleep;

/// Max automatic wqc-core restarts before mining is stopped.
const CORE_RESTART_MAX_ATTEMPTS: u32 = 10;
/// Base delay (seconds) for exponential backoff: min(60, 2^attempt).
const CORE_RESTART_BASE_DELAY_SECS: u64 = 2;
const CORE_RESTART_MAX_DELAY_SECS: u64 = 60;
/// After this many seconds of continuous core uptime, the restart budget resets.
const CORE_RESTART_STABLE_SECS: u64 = 60;

use crate::config::{MinerSettings, Network, TnBackend};
use crate::data_dir::DataLayout;
use crate::paths::BinaryPaths;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    Error,
    Warn,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StatusIssue {
    pub code: String,
    pub message: String,
    pub severity: IssueSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_source: Option<String>,
}

impl std::fmt::Display for StatusIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl StatusIssue {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            severity: IssueSeverity::Error,
            log_source: None,
        }
    }

    pub fn error_with_log(code: &str, message: impl Into<String>, log_source: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            severity: IssueSeverity::Error,
            log_source: Some(log_source.to_string()),
        }
    }

    pub fn warn(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            severity: IssueSeverity::Warn,
            log_source: None,
        }
    }
}

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
    /// Sticky issues from the last start/auto-start failure or child exit.
    last_issues: Vec<StatusIssue>,
    /// Unexpected core exits / failed restarts since the last stable period.
    core_restart_attempts: u32,
    /// Earliest time to attempt another core restart (`None` = not scheduled).
    core_restart_after: Option<Instant>,
    /// When the current core process last became healthy (`None` if down).
    core_stable_since: Option<Instant>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MiningStatus {
    pub mining: bool,
    pub core_running: bool,
    pub node_running: bool,
    pub network: Network,
    pub tn_backend: TnBackend,
    pub message: Option<String>,
    pub issues: Vec<StatusIssue>,
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
            last_issues: Vec::new(),
            core_restart_attempts: 0,
            core_restart_after: None,
            core_stable_since: None,
        }
    }

    fn clear_core_restart_state(&mut self) {
        self.core_restart_attempts = 0;
        self.core_restart_after = None;
        self.core_stable_since = None;
    }

    fn mark_core_healthy(&mut self) {
        self.core_restart_after = None;
        self.core_stable_since = Some(Instant::now());
    }

    fn maybe_reset_restart_budget(&mut self) {
        if self.core_child.is_none() {
            return;
        }
        let Some(since) = self.core_stable_since else {
            return;
        };
        if since.elapsed() >= Duration::from_secs(CORE_RESTART_STABLE_SECS)
            && self.core_restart_attempts > 0
        {
            tracing::info!(
                stable_secs = CORE_RESTART_STABLE_SECS,
                "wqc-core stayed up; clearing restart budget"
            );
            self.core_restart_attempts = 0;
        }
    }

    fn schedule_core_restart(&mut self, delay: Duration) {
        self.core_restart_after = Some(Instant::now() + delay);
    }

    fn core_restart_delay(attempt: u32) -> Duration {
        let exp = CORE_RESTART_BASE_DELAY_SECS.saturating_mul(1u64 << attempt.min(5));
        Duration::from_secs(exp.min(CORE_RESTART_MAX_DELAY_SECS))
    }

    fn ensure_binaries(&mut self) -> Result<(), StatusIssue> {
        if self.binaries.is_some() {
            return Ok(());
        }
        match BinaryPaths::resolve(self.bin_dir_override.as_deref()) {
            Ok(paths) => {
                self.binaries = Some(paths);
                Ok(())
            }
            Err(err) => Err(StatusIssue::error(
                "binaries_missing",
                format!("{err:#} — set --bin-dir or WQC_MINER_BIN_DIR"),
            )),
        }
    }

    pub fn reload_settings(&mut self, settings: MinerSettings) {
        self.settings = settings;
    }

    fn clear_sticky_issues(&mut self) {
        self.last_issues.clear();
    }

    fn set_sticky_issue(&mut self, issue: StatusIssue) {
        self.last_issues = vec![issue];
    }

    /// Reap exited children and update sticky issues / mining flag.
    pub fn refresh_runtime_state(&mut self) {
        if self.settings.is_mainnet_mock() {
            return;
        }

        let mut core_exited = false;
        let mut node_exited = false;
        let mut core_code = None;
        let mut node_code = None;

        if let Some(child) = self.core_child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    core_exited = true;
                    core_code = status.code();
                    self.core_child = None;
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(error = %err, "try_wait wqc-core failed");
                    core_exited = true;
                    self.core_child = None;
                }
            }
        }

        if let Some(child) = self.node_child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    node_exited = true;
                    node_code = status.code();
                    self.node_child = None;
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(error = %err, "try_wait wqc-node failed");
                    node_exited = true;
                    self.node_child = None;
                }
            }
        }

        if node_exited {
            self.mining_active = false;
            self.clear_core_restart_state();
            let mut issues = Vec::new();
            if core_exited {
                issues.push(StatusIssue::error_with_log(
                    "core_exited",
                    format!("wqc-core exited unexpectedly{}", exit_code_note(core_code)),
                    "core",
                ));
            }
            issues.push(StatusIssue::error_with_log(
                "node_exited",
                format!("wqc-node exited unexpectedly{}", exit_code_note(node_code)),
                "node",
            ));
            self.last_issues = issues;
            return;
        }

        if core_exited && self.mining_active {
            self.core_stable_since = None;
            let delay = Self::core_restart_delay(self.core_restart_attempts);
            self.schedule_core_restart(delay);
            let next_attempt = self.core_restart_attempts.saturating_add(1);
            self.last_issues = vec![StatusIssue::warn(
                "core_restarting",
                format!(
                    "wqc-core exited unexpectedly{} — restarting in {}s (attempt {}/{})",
                    exit_code_note(core_code),
                    delay.as_secs().max(1),
                    next_attempt.min(CORE_RESTART_MAX_ATTEMPTS),
                    CORE_RESTART_MAX_ATTEMPTS
                ),
            )];
            if let Some(issue) = self.last_issues.last_mut() {
                issue.log_source = Some("core".to_string());
            }
            tracing::warn!(
                exit_code = ?core_code,
                delay_secs = delay.as_secs(),
                next_attempt,
                "wqc-core exited; scheduling auto-restart"
            );
        }
    }

    /// Periodic supervision: reap children and auto-restart core while mining.
    pub async fn tick(&mut self) {
        self.refresh_runtime_state();
        self.maybe_reset_restart_budget();

        if !self.mining_active || self.settings.is_mainnet_mock() {
            return;
        }

        if self.core_child.is_some() {
            return;
        }

        let Some(after) = self.core_restart_after else {
            // Core missing without a schedule (e.g. race); schedule immediately.
            self.schedule_core_restart(Duration::from_secs(0));
            return;
        };
        if Instant::now() < after {
            return;
        }

        self.core_restart_attempts = self.core_restart_attempts.saturating_add(1);
        let attempt = self.core_restart_attempts;
        if attempt > CORE_RESTART_MAX_ATTEMPTS {
            self.mining_active = false;
            self.clear_core_restart_state();
            self.set_sticky_issue(StatusIssue::error_with_log(
                "core_restart_failed",
                format!(
                    "wqc-core failed to stay up after {CORE_RESTART_MAX_ATTEMPTS} restart attempts — mining stopped"
                ),
                "core",
            ));
            tracing::error!("wqc-core restart budget exhausted; stopping mining");
            return;
        }

        tracing::info!(
            attempt,
            max = CORE_RESTART_MAX_ATTEMPTS,
            "auto-restarting wqc-core"
        );

        if let Err(issue) = self.start_core().await {
            let delay = Self::core_restart_delay(attempt);
            self.schedule_core_restart(delay);
            let mut issue = issue;
            issue.message = format!(
                "{} — next retry in {}s ({}/{})",
                issue.message,
                delay.as_secs().max(1),
                attempt,
                CORE_RESTART_MAX_ATTEMPTS
            );
            self.set_sticky_issue(issue);
            return;
        }

        if let Err(issue) = self.wait_for_core_health().await {
            // Drop the unhealthy child so the next tick can respawn.
            if let Some(mut child) = self.core_child.take() {
                stop_child(&mut child).await;
            }
            let delay = Self::core_restart_delay(attempt);
            self.schedule_core_restart(delay);
            let mut issue = issue;
            issue.code = "core_restarting".to_string();
            issue.severity = IssueSeverity::Warn;
            issue.message = format!(
                "{} — next retry in {}s ({}/{})",
                issue.message,
                delay.as_secs().max(1),
                attempt,
                CORE_RESTART_MAX_ATTEMPTS
            );
            self.set_sticky_issue(issue);
            return;
        }

        tracing::info!(attempt, "wqc-core auto-restart succeeded");
        self.mark_core_healthy();
        self.clear_sticky_issues();
    }

    pub async fn collect_issues(&mut self) -> Vec<StatusIssue> {
        self.refresh_runtime_state();

        let mut issues = self.last_issues.clone();

        if self.binaries.is_none()
            && BinaryPaths::try_resolve(self.bin_dir_override.as_deref())
                .ok()
                .flatten()
                .is_none()
            && !issues.iter().any(|i| i.code == "binaries_missing")
        {
            issues.push(StatusIssue::warn(
                "binaries_missing",
                "wqc-core / wqc-node not found — set --bin-dir or WQC_MINER_BIN_DIR before starting",
            ));
        }

        if self.mining_active
            && !self.settings.is_mainnet_mock()
            && self.node_child.is_some()
            && !issues.iter().any(|i| {
                i.code == "bootstrap_unreachable"
                    || i.code == "node_exited"
                    || i.code == "core_exited"
                    || i.code == "core_restarting"
                    || i.code == "core_restart_failed"
            })
        {
            if let Err(err) = self.probe_node_http().await {
                issues.push(StatusIssue::warn(
                    "bootstrap_unreachable",
                    format!(
                        "wqc-node HTTP /status is unreachable ({err:#}). Check bootstrap / P2P and node logs."
                    ),
                ));
                if let Some(issue) = issues.last_mut() {
                    issue.log_source = Some("node".to_string());
                }
            }
        }

        issues
    }

    async fn probe_node_http(&self) -> anyhow::Result<()> {
        let url = format!("http://127.0.0.1:{}/status", self.settings.node_http_port);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;
        let response = client.get(&url).send().await?;
        if !response.status().is_success() {
            bail!("HTTP {}", response.status());
        }
        Ok(())
    }

    pub async fn status(&mut self) -> MiningStatus {
        let issues = self.collect_issues().await;
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
            issues,
        }
    }

    pub async fn start(&mut self) -> Result<MiningStatus, StatusIssue> {
        if self.core_child.is_some() || self.node_child.is_some() {
            return Err(StatusIssue::warn(
                "already_running",
                "mining is already running or stopping — try again",
            ));
        }
        if self.mining_active {
            return Err(StatusIssue::warn(
                "already_running",
                "mining is already running",
            ));
        }

        if let Err(issue) = self.preflight_issue() {
            self.set_sticky_issue(issue.clone());
            return Err(issue);
        }

        if self.settings.is_mainnet_mock() {
            tracing::info!(
                wallet = %self.settings.wallet_address,
                "mainnet mock start — skipping core/node spawn"
            );
            self.clear_sticky_issues();
            self.mining_active = true;
            return Ok(self.status().await);
        }

        if let Err(issue) = self.start_core().await {
            self.set_sticky_issue(issue.clone());
            let _ = self.stop_children().await;
            return Err(issue);
        }
        if let Err(issue) = self.wait_for_core_health().await {
            self.set_sticky_issue(issue.clone());
            let _ = self.stop_children().await;
            return Err(issue);
        }
        if let Err(issue) = self.start_node().await {
            self.set_sticky_issue(issue.clone());
            let _ = self.stop_children().await;
            return Err(issue);
        }

        self.core_restart_attempts = 0;
        self.mark_core_healthy();
        self.clear_sticky_issues();
        self.mining_active = true;
        Ok(self.status().await)
    }

    fn preflight_issue(&mut self) -> Result<(), StatusIssue> {
        match self.settings.network {
            Network::Testnet => {
                if self.settings.node_key.trim().is_empty() {
                    return Err(StatusIssue::error(
                        "node_key_missing",
                        "Set your Node Key in settings before starting (from the testnet dashboard).",
                    ));
                }
            }
            Network::Mainnet => {
                if self.settings.wallet_address.trim().is_empty() {
                    return Err(StatusIssue::error(
                        "wallet_missing",
                        "Set your wallet address in settings before starting.",
                    ));
                }
            }
        }
        self.ensure_binaries()?;
        Ok(())
    }

    pub async fn stop(&mut self) -> anyhow::Result<MiningStatus> {
        self.stop_children().await;
        self.mining_active = false;
        self.clear_core_restart_state();
        self.clear_sticky_issues();
        Ok(self.status().await)
    }

    async fn stop_children(&mut self) {
        if let Some(mut child) = self.node_child.take() {
            stop_child(&mut child).await;
        }
        if let Some(mut child) = self.core_child.take() {
            stop_child(&mut child).await;
        }
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

    async fn start_core(&mut self) -> Result<(), StatusIssue> {
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
        forward_env_if_set(&mut cmd, "WQC_PCS_MEMORY_POLICY");
        forward_env_if_set(&mut cmd, "WQC_PCS_MMCS_GROUP_CHUNK");
        forward_env_if_set(&mut cmd, "WQC_PCS_MEMORY_ESTIMATE_SCALE");

        if cfg!(unix) {
            cmd.env("WQC_CONNECTION_MODE", "uds");
            cmd.env("WQC_SOCKET_PATH", &socket_path);
        } else {
            cmd.env("WQC_CONNECTION_MODE", "tcp");
            cmd.env("WQC_CORE_TCP_PORT", self.settings.core_tcp_port.to_string());
        }

        tracing::info!("starting wqc-core: {}", core_bin.display());
        let mut child = cmd.spawn().map_err(|err| {
            StatusIssue::error_with_log(
                "core_spawn_failed",
                format!("failed to spawn wqc-core: {err}"),
                "core",
            )
        })?;
        self.spawn_log_task(
            "core",
            child.stdout.take(),
            child.stderr.take(),
            self.layout.core_log_path(),
        );
        self.core_child = Some(child);
        Ok(())
    }

    async fn start_node(&mut self) -> Result<(), StatusIssue> {
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
        let mut child = cmd.spawn().map_err(|err| {
            StatusIssue::error_with_log(
                "node_spawn_failed",
                format!("failed to spawn wqc-node: {err}"),
                "node",
            )
        })?;
        self.spawn_log_task(
            "node",
            child.stdout.take(),
            child.stderr.take(),
            self.layout.node_log_path(),
        );
        self.node_child = Some(child);
        Ok(())
    }

    async fn wait_for_core_health(&self) -> Result<(), StatusIssue> {
        let health_url = core_http_url(&self.settings, "/health");
        let client = build_core_client(&self.layout).map_err(|err| {
            StatusIssue::error_with_log(
                "core_unhealthy",
                format!("cannot build health client: {err:#}"),
                "core",
            )
        })?;

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
        Err(StatusIssue::error_with_log(
            "core_unhealthy",
            "wqc-core did not become healthy within 30 seconds — check Core logs",
            "core",
        ))
    }

    fn spawn_log_task(
        &self,
        name: &str,
        stdout: Option<tokio::process::ChildStdout>,
        stderr: Option<tokio::process::ChildStderr>,
        path: std::path::PathBuf,
    ) {
        let name = name.to_string();
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
                        format!("[{name}][stdout] {line}\n").as_bytes(),
                    )
                    .await;
                }
            }
            if let Some(err) = stderr {
                let mut reader = BufReader::new(err).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = tokio::io::AsyncWriteExt::write_all(
                        &mut writer,
                        format!("[{name}][stderr] {line}\n").as_bytes(),
                    )
                    .await;
                }
            }
        });
    }
}

fn forward_env_if_set(cmd: &mut Command, key: &str) {
    if let Ok(value) = std::env::var(key) {
        if !value.trim().is_empty() {
            cmd.env(key, value);
        }
    }
}

fn exit_code_note(code: Option<i32>) -> String {
    match code {
        Some(c) => format!(" (exit {c})"),
        None => String::new(),
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
