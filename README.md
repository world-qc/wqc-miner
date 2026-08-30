# wqc-miner

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Status: Alpha](https://img.shields.io/badge/Status-Alpha-yellow.svg)]()
[![CI](https://github.com/world-qc/wqc-miner/actions/workflows/ci.yml/badge.svg)](https://github.com/world-qc/wqc-miner/actions/workflows/ci.yml)

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

Optional env vars set **before** launching `wqc-miner` are forwarded to `wqc-core` when present (advanced tuning — most miners can ignore):

| Variable | Forwarded to | Notes |
|----------|--------------|-------|
| `WQC_MPS_MAX_BOND_DIM` | `wqc-core` | MPS bond-dimension ceiling (default `128` in core). See [`wqc-core` `doc/tn-engine.md`](https://github.com/world-qc/wqc-core/blob/main/doc/tn-engine.md). |
| `WQC_PCS_MEMORY_POLICY` | `wqc-core` | `refuse` or `spill` |
| `WQC_PCS_MMCS_GROUP_CHUNK` | `wqc-core` | PCS prove chunk size |
| `WQC_PCS_MEMORY_ESTIMATE_SCALE` | `wqc-core` | Memory estimate scale |
| `WQC_PCS_TIMEOUT_SECS` | `wqc-node` | PCS open-call timeout |

## API (admin)

The admin API **binds to localhost and has no authentication** — anything that can reach
the port can rewrite settings and control mining. Do not expose the port.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Bundled admin UI (assets under `/static`) |
| GET | `/api/status` | Mining + settings summary; includes `mining.issues[]` |
| GET/PUT | `/api/settings` | Read/update settings (partial update; secrets write-only) |
| POST | `/api/mining/start` | Start (testnet: core → node; mainnet: mock). Errors may include `code` |
| POST | `/api/mining/stop` | Stop node → core (or mock) |
| GET | `/api/node-status` | Proxy `wqc-node` `/status` (mock on mainnet) |
| WS | `/api/logs/ws?source=core\|node&lines=200` | Initial log tail (`hello`) then live `line` events |

### Semantics worth knowing

- **`PUT /api/settings`:** every field is optional. `max_memory_gb` is clamped server-side to
  `[1.0, host_max_memory_gb]`, so the stored value may differ from what was sent. `node_key`
  and `wallet_address` are write-only; reads report only `node_key_set` / `wallet_address_set`.
- **`GET /api/status` → `core_tn`:** present only when `wqc-core` is running **and** answered
  `/sysinfo`. `null` means unknown, not “CPU backend”.
- **`GET /api/node-status`:** when `wqc-node` is not running (or mainnet mock is inactive),
  returns `400` rather than an empty success.
- **`WS /api/logs/ws`:** frames are JSON text discriminated by `type` — one `hello` (tail),
  then `line` events, or a terminal `error`. Unknown `source` yields an `error` frame after
  upgrade (default `source=core`, `lines` clamped to 1–1000, default 200).

Issue codes (`mining.issues[].code` and start errors):

| Code | Meaning |
|------|---------|
| `node_key_missing` | Testnet selected but no node key is stored |
| `wallet_missing` | Mainnet selected but no wallet address is stored |
| `binaries_missing` | `wqc-core` or `wqc-node` executable not found |
| `bootstrap_unreachable` | No configured bootstrap URL answered |
| `already_running` | Start requested while mining is active or stopping |
| `core_spawn_failed` | `wqc-core` could not be launched |
| `node_spawn_failed` | `wqc-node` could not be launched |
| `core_unhealthy` | `wqc-core` is running but not answering |
| `core_exited` | `wqc-core` exited unexpectedly |
| `core_restarting` | `wqc-core` is being restarted with backoff |
| `core_restart_failed` | Restart budget exhausted; mining stopped |
| `node_exited` | `wqc-node` exited, which stops mining immediately |

While mining, if `wqc-core` exits unexpectedly the miner keeps `wqc-node` running and auto-restarts core with exponential backoff (up to 10 exits). The restart budget resets after core stays healthy for 60s. Exhausted retries set `core_restart_failed` and stop mining. Node exits still stop mining immediately.

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

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding guidelines, and the pull request process.

## License

Distributed under the GNU General Public License v3.0 (GPLv3). See `LICENSE` for more information.
