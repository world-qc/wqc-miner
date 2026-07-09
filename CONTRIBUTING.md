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

### Run locally

From the `world-qc` monorepo root (or with `WQC_MINER_BIN_DIR` pointing at release binaries):

```bash
cargo build --release -p wqc-core -p wqc-node
export WQC_MINER_BIN_DIR="../target/release"
cargo run -p wqc-miner -- --bin-dir "$WQC_MINER_BIN_DIR"
```

Open `http://127.0.0.1:3000` for the admin UI. See [README.md](README.md) for settings, data directory layout, and platform notes.

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

## Licensing

By contributing, you agree that your contributions will be licensed under the same terms as the project: the [GNU General Public License v3.0](LICENSE).

## Questions

If something is unclear, open a [GitHub Issue](https://github.com/world-qc/wqc-miner/issues) or ask in your pull request. We are happy to help you get started.
