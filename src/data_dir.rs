use std::path::{Path, PathBuf};

use anyhow::Context;
use directories::ProjectDirs;

#[derive(Debug, Clone)]
pub struct DataLayout {
    pub root: PathBuf,
}

impl DataLayout {
    pub fn resolve(override_dir: Option<&Path>) -> anyhow::Result<Self> {
        let root = if let Some(path) = override_dir {
            path.to_path_buf()
        } else {
            ProjectDirs::from("io", "world-qc", "wqc-miner")
                .context("could not resolve platform data directory")?
                .data_dir()
                .to_path_buf()
        };
        Ok(Self { root })
    }

    pub fn ensure_dirs(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(self.root.join("keys"))?;
        std::fs::create_dir_all(self.root.join("logs"))?;
        Ok(())
    }

    pub fn settings_path(&self) -> PathBuf {
        self.root.join("settings.toml")
    }

    pub fn node_private_key_path(&self) -> PathBuf {
        self.root.join("keys").join("node_private_key.b64")
    }

    pub fn node_db_path(&self) -> PathBuf {
        self.root.join("node.db")
    }

    pub fn core_socket_path(&self) -> PathBuf {
        self.root.join("wqc-core.sock")
    }
}
