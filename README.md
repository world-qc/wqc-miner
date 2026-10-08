# wqc-miner

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Status: Beta](https://img.shields.io/badge/Status-Beta-orange.svg)]()
[![CI](https://github.com/world-qc/wqc-miner/actions/workflows/ci.yml/badge.svg)](https://github.com/world-qc/wqc-miner/actions/workflows/ci.yml)

Cross-platform launcher and local admin UI for WQC worker nodes. Bundles [`wqc-core`](https://github.com/world-qc/wqc-core) (quantum compute) and [`wqc-node`](https://github.com/world-qc/wqc-node) (P2P worker) behind a single desktop-style workflow.

## Role in the WQC pipeline

```
client → wqc-orchestrator → wqc-node → wqc-core
                              ↑ wqc-miner (this repo) starts & configures both
```

**Recommended entry point for operators** on testnet and mainnet. The launcher generates keys, holds network-specific credentials, sets memory budget and bootstrap URLs, and supervises `wqc-core` + `wqc-node` when mining is active.

Task lifecycle is normative in [wqc-docs `spec/architecture-current.md` §3](https://github.com/world-qc/wqc-docs/blob/main/spec/architecture-current.md#3-task-lifecycle).

## Networks

| Network | Mining | Operator guide |
|---------|--------|----------------|
| **Testnet** | Live — core + node + P2P | [`docs/TESTNET.md`](docs/TESTNET.md) |
| **Mainnet** | UI mock only (P2P not connected yet) | [`docs/MAINNET.md`](docs/MAINNET.md) |

Pick **Testnet** or **Mainnet** in the admin UI (`http://127.0.0.1:3000` by default). Network-specific setup steps live in the guides above.

## Quick start

1. Download a release from [GitHub Releases](https://github.com/world-qc/wqc-miner/releases).
2. Run `wqc-miner`, open the admin UI, choose your network.
3. Follow [`docs/TESTNET.md`](docs/TESTNET.md) or [`docs/MAINNET.md`](docs/MAINNET.md).

## What it does

- Local admin UI on `127.0.0.1` (default port `3000`)
- **Testnet**: start/stop `wqc-core` + `wqc-node`, proxy node `/status`, tail logs over WebSocket
- **Mainnet** (today): wallet-address settings and mock status only — see [`docs/MAINNET.md`](docs/MAINNET.md)
- Headless / systemd via `auto_start` in `settings.toml` or `--auto-start`

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

On first launch, `settings.toml` is created automatically with defaults. To pre-seed or edit by hand:

```bash
mkdir -p "$HOME/.local/share/wqc-miner"   # Linux example
cp settings.toml.example "$HOME/.local/share/wqc-miner/settings.toml"
```

## Headless / Linux auto-start

The admin UI is plain HTTP on `127.0.0.1` — no display server is required.

1. Copy `settings.toml.example` into the data directory and set credentials for your network (`node_key` on testnet — see [`docs/TESTNET.md`](docs/TESTNET.md); `wallet_address` on mainnet — see [`docs/MAINNET.md`](docs/MAINNET.md)).
2. Set `auto_start = true` in `settings.toml`, **or** pass `--auto-start`.
3. Keep `tn_backend = "cpu"` on headless hosts unless you have a working GPU/WebGPU stack.

If auto-start fails (missing credentials, missing binaries), the process stays up and the admin UI remains available so you can fix settings.

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

## Binary layout (release)

Release bundles ship:

```text
wqc-miner(.exe)
bin/
  wqc-core(.exe)
  wqc-node(.exe)
settings.toml.example
LICENSE
README.md
```

The archive `README.md` is [`packaging/README.md`](packaging/README.md), written for people who only have the binaries. This repository README is not shipped.

For local builds, place sibling binaries next to the launcher, set `WQC_MINER_BIN_DIR` / `--bin-dir`, or put both on `PATH`.

## Admin API

The admin API **binds to localhost and has no authentication** — anything that can reach the port can rewrite settings and control mining. Do not expose the port.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Bundled admin UI (assets under `/static`) |
| GET | `/api/status` | Mining + settings summary; includes `mining.issues[]` |
| GET/PUT | `/api/settings` | Read/update settings (partial update; secrets write-only) |
| POST | `/api/mining/start` | Start mining (testnet: core → node; mainnet: mock until P2P ships) |
| POST | `/api/mining/stop` | Stop node → core (or mock) |
| GET | `/api/node-status` | Proxy `wqc-node` `/status` (mock on mainnet today) |
| WS | `/api/logs/ws?source=core\|node&lines=200` | Log tail + live stream |

`PUT /api/settings` clamps `max_memory_gb` to `[1.0, host_max_memory_gb]`. `node_key` and `wallet_address` are write-only.

Network-specific env injection and issue codes: [`docs/TESTNET.md`](docs/TESTNET.md). Core auto-restart on unexpected exit: exponential backoff (up to 10 exits; budget resets after 60s healthy). Node exit stops mining immediately.

## Development

Build `wqc-core` and `wqc-node`, then point the miner at them. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full sibling-repo workflow.

Open `http://127.0.0.1:3000`, select a network, configure credentials, then **Start mining** (testnet only for real P2P today).

## CI / Release

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| [`.github/workflows/ci.yml`](.github/workflows/ci.yml) | push / PR to `main` | `cargo fmt`, `clippy`, `build`, `test` |
| [`.github/workflows/release.yml`](.github/workflows/release.yml) | tag `vMAJOR.MINOR.PATCH` or manual dispatch | Bundled zip / dmg / tar.gz → GitHub Releases |

**Version tags** use SemVer with a leading `v`, matching `Cargo.toml` `version` (e.g. `0.1.0` → tag `v0.1.0`). The release workflow checks that the tag equals `v` + `[package].version` before building. While still in `0.y.z`, MINOR may include breaking changes; PATCH is for compatible fixes. This version is the **release bundle**, not a lockstep with `wqc-core` / `wqc-node` crate versions.

Release builds check out sibling repos (`wqc-core`, `wqc-node`, `wqc-stark-engine`) so `wqc-core`'s `[patch]` for `wqc-stark-engine` resolves. `wqc-core` is built with `--features webgpu`.

Artifacts: Windows (x64, arm64), macOS universal `.dmg`, Linux (x64, arm64). Each bundle contains `wqc-miner` + `bin/wqc-core` + `bin/wqc-node`. Windows binaries statically link the Visual C++ runtime, so the zip runs on Windows 10+ without the VC++ Redistributable (`VCRUNTIME140.dll`).

### Provenance (no CA code signing)

WQC does **not** ship commercial CA signatures (Sectigo, GlobalSign, Apple Developer ID, Authenticode). Release assets are attested with [GitHub Artifact Attestations](https://docs.github.com/en/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds) (SLSA provenance via Sigstore).

```bash
gh attestation verify wqc-miner-linux-x64.tar.gz -R world-qc/wqc-miner
```

OS SmartScreen / Gatekeeper may still warn; use attestation verify as the trust path.

## Upcoming

- Mainnet P2P mining (replacing mock UI — see [`docs/MAINNET.md`](docs/MAINNET.md)).
- On-chain economic layer (orchestrator / L2).

## Documentation

- [`docs/TESTNET.md`](docs/TESTNET.md) — testnet operator guide
- [`docs/MAINNET.md`](docs/MAINNET.md) — mainnet operator guide (placeholder)
- [`settings.toml.example`](settings.toml.example) — configurable fields and defaults
- [`wqc-node` `docs/OPERATIONS.md`](https://github.com/world-qc/wqc-node/blob/main/docs/OPERATIONS.md) — P2P worker lifecycle and troubleshooting
- [`wqc-core` README](https://github.com/world-qc/wqc-core/blob/main/README.md) — compute engine and advanced core env
- [wqc-docs `spec/architecture-current.md`](https://github.com/world-qc/wqc-docs/blob/main/spec/architecture-current.md) — swarm topology and economy

## Requirements

- **OS**: Windows 10+, macOS 11+, or Linux x64/arm64 (see release artifacts)
- **RAM**: `max_memory_gb` in settings (default: host RAM minus reserve). See [`wqc-node` README](https://github.com/world-qc/wqc-node/blob/main/README.md#environment-variables) for how this maps to advertised qubit capability.
- **Network**: outbound HTTPS to bootstrap URL; inbound P2P on `p2p_listen_port` when mining on a network that requires it
- **Rust** 1.95+ only if building from source (see `AGENTS.md`)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding guidelines, and the pull request process.

## License

Distributed under the GNU General Public License v3.0 (GPLv3). See `LICENSE` for more information.
