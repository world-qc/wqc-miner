# wqc-miner — Testnet

Operator guide for **public testnet** (`testnet.world-qc.io`). Shared launcher behavior (data directory, admin API, releases) is in the [README](../README.md).

## Quick start

1. Download a release for your platform from [GitHub Releases](https://github.com/world-qc/wqc-miner/releases) (bundled `wqc-miner` + `bin/wqc-core` + `bin/wqc-node`).
2. Run `wqc-miner` and open `http://127.0.0.1:3000`.
3. Select **Testnet**, paste your operator **Node Key** from [testnet.world-qc.io](https://testnet.world-qc.io), save settings.
4. Click **Start mining**.

The launcher auto-generates `WQC_NODE_PRIVATE_KEY` on first launch and writes `settings.toml` under the [data directory](../README.md#data-directory). Adjust **Max memory (GiB)** if needed — this becomes `WQC_MAX_MEMORY_GB` for core and node.

Verify release artifacts with [GitHub Artifact Attestations](../README.md#provenance-no-ca-code-signing).

## Headless / systemd

1. Copy [`settings.toml.example`](../settings.toml.example) into the data directory and set `network = "testnet"` and `node_key`.
2. Set `auto_start = true`, **or** pass `--auto-start`.
3. Keep `tn_backend = "cpu"` unless you have a working WebGPU stack.

Default bootstrap URL is `https://testnet.world-qc.io/api/v1/p2p/bootstrap` (see `settings.toml.example`).

See [README — Headless](../README.md#headless--linux-auto-start) for a systemd unit example.

## Environment (injected when mining)

| Variable | Source |
|----------|--------|
| `WQC_NODE_PRIVATE_KEY` | auto-generated key file |
| `WQC_TESTNET_NODE_KEY` | settings (`node_key`) |
| `WQC_BOOTSTRAP_URLS` | settings |
| `WQC_CORE_URL` | derived from data dir / TCP port |
| `WQC_MAX_MEMORY_GB` | settings (`max_memory_gb`) |
| `WQC_TN_BACKEND` | settings |
| `WQC_P2P_LISTEN_PORT` | settings |
| `WQC_HTTP_PORT` | settings |
| `WQC_DATABASE_URL` | `sqlite:{data_dir}/node.db` |

Optional env vars set **before** launching `wqc-miner` are forwarded when present (advanced):

| Variable | Forwarded to | Notes |
|----------|--------------|-------|
| `WQC_MPS_MAX_BOND_DIM` | `wqc-core` | Default `128` in core — see [`wqc-core` `doc/tn-engine.md`](https://github.com/world-qc/wqc-core/blob/main/doc/tn-engine.md) |
| `WQC_PCS_MEMORY_POLICY` | `wqc-core` | `spill` required for PCS open-call bids |
| `WQC_PCS_MMCS_GROUP_CHUNK` | `wqc-core` | PCS prove chunk size |
| `WQC_PCS_MEMORY_ESTIMATE_SCALE` | `wqc-core` | Memory estimate scale |
| `WQC_PCS_TIMEOUT_SECS` | `wqc-node` | PCS prove wall-clock budget |

Worker-side P2P troubleshooting: [`wqc-node` `docs/OPERATIONS.md`](https://github.com/world-qc/wqc-node/blob/main/docs/OPERATIONS.md).

## Common issues

| Code | Meaning |
|------|---------|
| `node_key_missing` | Testnet selected but no node key stored |
| `binaries_missing` | `wqc-core` or `wqc-node` not found in bundle / `--bin-dir` |
| `bootstrap_unreachable` | Bootstrap URL did not answer |
| `core_unhealthy` / `core_exited` | Core not answering or exited (auto-restart with backoff) |
| `node_exited` | Node exited — mining stops immediately |

Full issue codes and restart policy: [`src/supervisor.rs`](../src/supervisor.rs).
