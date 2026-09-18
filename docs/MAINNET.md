# wqc-miner — Mainnet

Operator guide for **mainnet** (`mainnet.world-qc.io`). Shared launcher behavior is in the [README](../README.md).

## Status

**Mainnet P2P mining is not available yet.** Selecting **Mainnet** in the admin UI today only:

- stores a **wallet address** in `settings.toml` (`wallet_address`)
- shows **mock** mining status (no `wqc-core` / `wqc-node` spawn)

Real mainnet worker flow will use the same launcher and release bundles as testnet.

## Planned (placeholder)

When mainnet mining ships, this page will document:

- wallet / payout identity setup
- bootstrap URLs and network-specific settings
- headless `settings.toml` for mainnet
- any mainnet-only env vars injected by the launcher

Until then, use [TESTNET.md](TESTNET.md) to run a worker on public testnet.

## UI today

1. Open `http://127.0.0.1:3000`, select **Mainnet**.
2. Enter a wallet address and save settings.
3. **Start mining** activates the mock path only (no swarm connection).

Default bootstrap in [`settings.toml.example`](../settings.toml.example) / UI defaults points at
`https://mainnet.world-qc.io/api/v1/p2p/bootstrap` for when mainnet worker mode is enabled.
Marketing site remains `https://world-qc.io`.
