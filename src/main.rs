mod admin;
mod cli;
mod config;
mod data_dir;
mod keys;
mod logs;
mod memory_budget;
mod paths;
mod supervisor;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

use crate::admin::AdminState;
use crate::cli::Cli;
use crate::config::MinerSettings;
use crate::data_dir::DataLayout;
use crate::keys::ensure_node_private_key;
use crate::paths::BinaryPaths;
use crate::supervisor::Supervisor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let layout = DataLayout::resolve(cli.data_dir.as_deref())?;
    layout.ensure_dirs()?;

    let mut settings = MinerSettings::load(&layout)?;
    if let Some(port) = cli.admin_port {
        settings.admin_port = port;
    }
    settings.save(&layout)?;

    let node_private_key_b64 = ensure_node_private_key(&layout)?;
    let binaries = BinaryPaths::try_resolve(cli.bin_dir.as_deref())?;

    tracing::info!("data directory: {}", layout.root.display());
    tracing::info!("admin UI: http://127.0.0.1:{}", settings.admin_port);
    match &binaries {
        Some(paths) => tracing::info!(
            "bundled binaries: core={} node={}",
            paths.core.display(),
            paths.node.display()
        ),
        None => tracing::warn!(
            "wqc-core / wqc-node not found yet — set --bin-dir before starting mining"
        ),
    }

    let want_auto_start = cli.auto_start || settings.auto_start;
    let supervisor = Arc::new(RwLock::new(Supervisor::new(
        layout.clone(),
        settings.clone(),
        node_private_key_b64,
        binaries,
        cli.bin_dir.clone(),
    )));

    if want_auto_start {
        match supervisor.write().await.start().await {
            Ok(status) => {
                tracing::info!(
                    mining = status.mining,
                    "auto-start: mining started (admin UI still available)"
                );
            }
            Err(err) => {
                tracing::error!(
                    code = %err.code,
                    "auto-start failed: {err} — admin UI remains available; fix settings and retry"
                );
            }
        }
    }

    let admin_port = settings.admin_port;
    let admin_state = AdminState {
        layout,
        settings: Arc::new(RwLock::new(settings)),
        supervisor: supervisor.clone(),
    };

    {
        let supervisor = supervisor.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                supervisor.write().await.tick().await;
            }
        });
    }

    let app = admin::router(admin_state);
    let addr = SocketAddr::from(([127, 0, 0, 1], admin_port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind admin UI on {addr}"))?;

    tracing::info!("wqc-miner ready — open http://{addr}/ in your browser");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(supervisor))
        .await
        .context("admin server exited with error")?;

    Ok(())
}

async fn shutdown_signal(supervisor: Arc<RwLock<Supervisor>>) {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c");
    tracing::info!("shutting down — stopping child processes");
    let _ = supervisor.write().await.stop().await;
}
