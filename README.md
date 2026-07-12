# wqc-miner

Cross-platform launcher and local admin UI for WQC worker nodes. Bundles `wqc-core` (quantum compute) and `wqc-node` (P2P worker) behind a single desktop-style workflow.

## What it does

- Runs a local admin UI (default `http://127.0.0.1:3000`)
- Lets you choose **Testnet** or **Mainnet**
- Auto-generates `WQC_NODE_PRIVATE_KEY` on first launch
- **Testnet**: paste your dashboard operator node key, then start `wqc-core` + `wqc-node`
- **Mainnet**: set a wallet address (mock for now — no P2P connection yet)
- Proxies `wqc-node` `/status` when mining is active (mock JSON on mainnet)

## Data directory

| OS | Default path |
|----|----------------|
| macOS | `~/Library/Application Support/io.world-qc.wqc-miner/` |
| Linux | `~/.local/share/wqc-miner/` (via `directories` crate) |
| Windows | `%APPDATA%\io.world-qc.wqc-miner\` |

Override with `--data-dir`.

Contents:

- `settings.toml` — network, bootstrap URLs, ports, credentials (see [`settings.toml.example`](settings.toml.example))
- `keys/node_private_key.b64` — auto-generated libp2p key
- `wqc-core.sock` — Unix socket (macOS/Linux only)
- `node.db` — SQLite state for `wqc-node`

On first launch, `settings.toml` is created automatically with defaults. To pre-seed or edit by hand, copy the example:

```bash
mkdir -p "$HOME/.local/share/wqc-miner"   # Linux example
cp settings.toml.example "$HOME/.local/share/wqc-miner/settings.toml"
```

## Binary layout (release)

Place sibling binaries next to the launcher:

```text
wqc-miner(.exe)
bin/
  wqc-core(.exe)
  wqc-node(.exe)
```

Or set `WQC_MINER_BIN_DIR` / `--bin-dir`, or put both binaries on `PATH`.

## Development

Build `wqc-core` and `wqc-node` from the sibling repos, then point the miner at them:

```bash
# from world-qc monorepo root
cargo build --release -p wqc-core -p wqc-node

export WQC_MINER_BIN_DIR="../target/release"   # adjust if needed
cargo run -p wqc-miner -- --bin-dir "$WQC_MINER_BIN_DIR"
```

Open `http://127.0.0.1:3000`, pick **Testnet** or **Mainnet**, save settings, then **Start mining**.

For testnet, paste your operator node key from [testnet.world-qc.io](https://testnet.world-qc.io).

## Headless / Linux auto-start

The admin UI is plain HTTP on `127.0.0.1` — no display server is required. For servers and systemd:

1. Copy `settings.toml.example` into the data directory and set `node_key` (testnet) or `wallet_address` (mainnet).
2. Set `auto_start = true` in `settings.toml`, **or** pass `--auto-start`.
3. Keep `tn_backend = "cpu"` on headless hosts unless you have a working GPU/WebGPU stack.

If auto-start fails (missing key, missing binaries), the process stays up and the admin UI remains available so you can fix settings.

Example systemd unit:

```ini
[Unit]
Description=WQC Miner
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=wqc
WorkingDirectory=/opt/wqc-miner
ExecStart=/opt/wqc-miner/wqc-miner --auto-start
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Remote admin access: prefer an SSH tunnel (`ssh -L 3000:127.0.0.1:3000 …`) rather than binding the UI to `0.0.0.0`.

### Node ↔ core transport

| OS | `wqc-core` mode | `WQC_CORE_URL` |
|----|----------------|----------------|
| macOS / Linux | UDS (`WQC_CONNECTION_MODE=uds`) | `unix:/path/to/wqc-core.sock` |
| Windows | TCP (`WQC_CONNECTION_MODE=tcp`) | `http://127.0.0.1:13000` (default) |

On Windows, `wqc-core` reads `WQC_CORE_TCP_PORT` (default `3000`; miner defaults to `13000` to avoid clashing with the admin UI on `3000`).

## Environment (injected by launcher, testnet only)

| Variable | Set by miner |
|----------|----------------|
| `WQC_NODE_PRIVATE_KEY` | auto-generated key file |
| `WQC_TESTNET_NODE_KEY` | settings UI (testnet node key) |
| `WQC_BOOTSTRAP_URLS` | settings (comma-separated) |
| `WQC_CORE_URL` | derived from data dir / TCP port |
| `WQC_MAX_MEMORY_GB` | settings |
| `WQC_TN_BACKEND` | settings (`cpu` or `webgpu`) |
| `WQC_P2P_LISTEN_PORT` | settings |
| `WQC_HTTP_PORT` | settings |
| `WQC_DATABASE_URL` | `sqlite:{data_dir}/node.db` |

## API (admin)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/status` | Mining + settings summary; includes `mining.issues[]` (`code`, `message`, `severity`, optional `log_source`) |
| GET/PUT | `/api/settings` | Read/update settings |
| POST | `/api/mining/start` | Start (testnet: core → node; mainnet: mock). Errors may include `code` |
| POST | `/api/mining/stop` | Stop node → core (or mock) |
| GET | `/api/node-status` | Proxy `wqc-node` `/status` (mock on mainnet) |
| WS | `/api/logs/ws?source=core\|node&lines=200` | Initial log tail (`hello`) then live `line` events |

Issue codes include `node_key_missing`, `binaries_missing`, `core_unhealthy`, `core_exited`, `node_exited`, `bootstrap_unreachable`.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding guidelines, and the pull request process.

## License

GPL-3.0 — see [LICENSE](LICENSE).

## CI / Release

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| [`.github/workflows/ci.yml`](.github/workflows/ci.yml) | push / PR to `main` | `cargo fmt`, `clippy`, `build`, `test` |
| [`.github/workflows/release.yml`](.github/workflows/release.yml) | tag `v*` or manual dispatch | Build bundled zip / dmg / tar.gz and upload to GitHub Releases |

Release builds check out sibling repos (`wqc-core`, `wqc-node`, `wqc-stark-engine`) into the same workspace so `wqc-core`'s `[patch]` for `wqc-stark-engine` resolves. `wqc-core` is built with `--features webgpu`.

```bash
git tag v0.1.0
git push origin v0.1.0
```

Artifacts:

- `wqc-miner-windows-x64.zip` — x86_64 Windows
- `wqc-miner-windows-arm64.zip` — ARM64 Windows (Snapdragon / ARM PCs)
- `wqc-miner-mac-universal.dmg` — macOS Universal Binary (Apple Silicon + Intel)
- `wqc-miner-linux-x64.tar.gz` — x86_64 Linux (`x86_64-unknown-linux-gnu`)
- `wqc-miner-linux-arm64.tar.gz` — ARM64 Linux (`aarch64-unknown-linux-gnu`)

Each bundle contains `wqc-miner` + `bin/wqc-core` + `bin/wqc-node`.

### Provenance (no CA code signing)

WQC does **not** ship commercial CA signatures (Sectigo, GlobalSign, Apple Developer ID, Authenticode). Release assets are attested with [GitHub Artifact Attestations](https://docs.github.com/en/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds) (SLSA provenance via Sigstore) in the release workflow.

```bash
gh attestation verify wqc-miner-linux-x64.tar.gz -R world-qc/wqc-miner
```

OS SmartScreen / Gatekeeper may still warn; use attestation verify as the trust path.
