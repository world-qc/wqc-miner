use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "wqc-miner", about = "WQC worker launcher and admin UI")]
pub struct Cli {
    /// Admin UI listen port (default from settings.toml or 3000).
    #[arg(long)]
    pub admin_port: Option<u16>,

    /// Override data directory (default: platform app data dir / wqc-miner).
    #[arg(long)]
    pub data_dir: Option<std::path::PathBuf>,

    /// Directory containing wqc-core and wqc-node binaries (default: ./bin next to this exe).
    #[arg(long)]
    pub bin_dir: Option<std::path::PathBuf>,

    /// Start mining immediately after loading settings (overrides settings.toml `auto_start = false`).
    /// Useful for headless Linux / systemd. Admin UI still listens on localhost.
    #[arg(long, default_value_t = false)]
    pub auto_start: bool,
}
