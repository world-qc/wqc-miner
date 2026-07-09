use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rand::RngCore;

use crate::data_dir::DataLayout;

/// Ensure a libp2p node private key exists on disk; create one if missing.
pub fn ensure_node_private_key(layout: &DataLayout) -> anyhow::Result<String> {
    let path = layout.node_private_key_path();
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        let trimmed = raw.trim();
        validate_private_key_b64(trimmed)?;
        return Ok(trimmed.to_string());
    }

    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let private_seed_b64 = STANDARD.encode(signing_key.to_bytes());
    validate_private_key_b64(&private_seed_b64)?;

    std::fs::write(&path, format!("{private_seed_b64}\n"))?;
    tracing::info!("generated new WQC node private key at {}", path.display());
    Ok(private_seed_b64)
}

fn validate_private_key_b64(value: &str) -> anyhow::Result<()> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|e| anyhow::anyhow!("invalid node private key base64: {e}"))?;
    if bytes.len() != 32 {
        anyhow::bail!("node private key must be 32 bytes after base64 decode");
    }
    Ok(())
}
