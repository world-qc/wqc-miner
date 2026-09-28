# Contributing to wqc-miner

Thank you for your interest in contributing to **wqc-miner**, the cross-platform launcher and local admin UI for WQC worker nodes.

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold it. Report unacceptable behavior using the contact details in that document.

## How to Contribute

Contributions are welcome in many forms:

- Bug reports and feature requests via [GitHub Issues](https://github.com/world-qc/wqc-miner/issues)
- Documentation improvements
- Code changes via pull requests

If you plan a larger change, please open an issue first so we can discuss the approach and avoid duplicate work.

## Development Setup

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) **1.95** or newer
- Built `wqc-core` and `wqc-node` binaries from sibling repos (for full start/stop testing)

### Clone and build

```bash
git clone https://github.com/world-qc/wqc-miner.git
cd wqc-miner
cargo build
```

### Run locally (full stack)

Public repos are **separate git checkouts**, not one Cargo workspace — so `cargo build -p wqc-core -p wqc-node` from a parent folder does not apply. Build each crate from its own directory (same pattern as [`.github/workflows/release.yml`](.github/workflows/release.yml)).

Clone siblings (`wqc-stark-engine` must sit next to `wqc-core` for its `[patch]`):

```bash
git clone https://github.com/world-qc/wqc-miner.git
git clone https://github.com/world-qc/wqc-core.git
git clone https://github.com/world-qc/wqc-node.git
git clone https://github.com/world-qc/wqc-stark-engine.git

export CARGO_TARGET_DIR="$HOME/wqc-target"   # optional; shared output dir like CI

(cd wqc-core && cargo build --release --features webgpu)
(cd wqc-node && cargo build --release)

export WQC_MINER_BIN_DIR="$CARGO_TARGET_DIR/release"
(cd wqc-miner && cargo run -- --bin-dir "$WQC_MINER_BIN_DIR")
```

Without `CARGO_TARGET_DIR`, copy both binaries into one directory instead:

```bash
mkdir -p ~/wqc-bins
cp wqc-core/target/release/wqc-core ~/wqc-bins/
cp wqc-node/target/release/wqc-node ~/wqc-bins/
export WQC_MINER_BIN_DIR=~/wqc-bins
(cd wqc-miner && cargo run -- --bin-dir "$WQC_MINER_BIN_DIR")
```

Open `http://127.0.0.1:3000` for the admin UI. For end-to-end mining tests, use testnet — see [`docs/TESTNET.md`](docs/TESTNET.md). See [README.md](README.md) for settings, data directory layout, and platform notes.

## Making Changes

1. Fork the repository and create a branch from `main`.
2. Make your changes in a focused, reviewable scope.
3. Run the checks below before opening a pull request.
4. Open a pull request against `main` with a clear description of the change and why it is needed.

### Branch naming

Use short, descriptive names, for example:

- `fix/supervisor-stop-order`
- `docs/settings-example`
- `feat/admin-log-tail`

## Coding Guidelines

- Write all source code, documentation, and comments in **English**.
- Keep the admin API predictable; return clear HTTP errors and log child-process failures.
- Follow common Rust conventions (`cargo fmt`, idiomatic error handling).
- Do not commit private keys, node keys, or wallet secrets. Treat `settings.toml` and `keys/` as sensitive.

## Checks

Before submitting a pull request, run:

```bash
cargo fmt --all
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

If you add new behavior, include tests where practical.

## Pull Request Guidelines

A good pull request:

- Has a concise title and description
- Explains the problem and the chosen solution
- Links related issues (for example, `Fixes #123`)
- Passes local checks listed above
- Keeps unrelated changes out of the diff

Maintainers may request changes or suggest an alternative approach. Once approved, your contribution will be merged.

## Releases

Maintainers cut operator bundles by tagging **`vMAJOR.MINOR.PATCH`** (SemVer + leading `v`), aligned with `Cargo.toml` `version` (the workflow fails if they differ). Example:

```bash
# after bumping version in Cargo.toml / Cargo.lock
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

That push runs [`.github/workflows/release.yml`](.github/workflows/release.yml). See [README — CI / Release](README.md#ci--release) for bump meaning and attestation.

## Licensing

By contributing, you agree that your contributions will be licensed under the same terms as the project: the [GNU General Public License v3.0](LICENSE).

## Questions

If something is unclear, open a [GitHub Issue](https://github.com/world-qc/wqc-miner/issues) or ask in your pull request. We are happy to help you get started.
