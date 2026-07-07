use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

#[derive(Debug, Clone)]
pub struct BinaryPaths {
    pub core: PathBuf,
    pub node: PathBuf,
}

impl BinaryPaths {
    pub fn resolve(override_dir: Option<&Path>) -> anyhow::Result<Self> {
        Self::try_resolve(override_dir)?
            .ok_or_else(|| anyhow::anyhow!("wqc-core and wqc-node binaries not found"))
    }

    /// Returns `None` when binaries are not present yet (admin UI can still start).
    pub fn try_resolve(override_dir: Option<&Path>) -> anyhow::Result<Option<Self>> {
        if let Some(dir) = override_dir {
            return Ok(Some(Self::from_dir(dir)?));
        }
        if let Ok(dir) = std::env::var("WQC_MINER_BIN_DIR") {
            return Ok(Some(Self::from_dir(Path::new(&dir))?));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let bundled = exe_dir.join("bin");
                if let Ok(paths) = Self::from_dir(&bundled) {
                    return Ok(Some(paths));
                }
            }
        }
        match Self::from_path_env() {
            Ok(paths) => Ok(Some(paths)),
            Err(_) => Ok(None),
        }
    }

    fn from_dir(dir: &Path) -> anyhow::Result<Self> {
        let core = dir.join(binary_name("wqc-core"));
        let node = dir.join(binary_name("wqc-node"));
        if !core.is_file() {
            bail!("wqc-core not found at {}", core.display());
        }
        if !node.is_file() {
            bail!("wqc-node not found at {}", node.display());
        }
        Ok(Self { core, node })
    }

    fn from_path_env() -> anyhow::Result<Self> {
        let core = which::which(binary_name("wqc-core"))
            .context("wqc-core not found in PATH — set --bin-dir or WQC_MINER_BIN_DIR")?;
        let node = which::which(binary_name("wqc-node"))
            .context("wqc-node not found in PATH — set --bin-dir or WQC_MINER_BIN_DIR")?;
        Ok(Self { core, node })
    }
}

fn binary_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}
