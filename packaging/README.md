# WQC Miner

This folder is a release build of the WQC worker launcher. Run `wqc-miner` from here. It starts `bin/wqc-core` and `bin/wqc-node` and serves a local admin page.

Source code and contributor documentation: <https://github.com/world-qc/wqc-miner>

## Contents

```text
wqc-miner            (wqc-miner.exe on Windows)
bin/wqc-core         (wqc-core.exe on Windows)
bin/wqc-node         (wqc-node.exe on Windows)
settings.toml.example
LICENSE
README.md
```

Keep `bin/` next to `wqc-miner`. The launcher looks there for the other two programs.

## Requirements

- Windows 10 or later, macOS 11 or later, or Linux (x64 or arm64)
- Outbound HTTPS to the bootstrap URL
- While mining, inbound P2P on `p2p_listen_port` (default `4002`)

Windows builds statically link the Visual C++ runtime. Installing the VC++ Redistributable is not required.

## Windows and macOS may block the first launch

This release is not signed with a commercial code-signing certificate (no Authenticode, no Apple Developer ID). Windows and macOS ask you to confirm before the program runs. Linux does not show this prompt.

### Windows

SmartScreen may say it prevented an unrecognized app from starting.

1. In that dialog, click **More info**, then **Run anyway**.
2. If `wqc-miner.exe` still will not start, right-click it, open **Properties**, check **Unblock** if that box is there, and click **OK**. Do the same for `bin\wqc-core.exe` and `bin\wqc-node.exe`.

Checking **Unblock** on the zip before you extract it covers every file inside.

### macOS

Gatekeeper may say the developer cannot be verified.

1. Copy this folder out of the disk image onto the Mac.
2. Control-click `wqc-miner` and choose **Open**, then **Open** in the next dialog.
3. If macOS blocks `bin/wqc-core` or `bin/wqc-node` when mining starts, open Terminal, `cd` to this folder, and run:

```bash
xattr -dr com.apple.quarantine .
```

That clears the download quarantine flag on this folder only.

To check that the archive matches the GitHub release build:

```bash
gh attestation verify wqc-miner-linux-x64.tar.gz -R world-qc/wqc-miner
```

Use the filename you downloaded. You still need the SmartScreen or Gatekeeper steps above.

## Start

1. Run `wqc-miner` from this folder (`wqc-miner.exe` on Windows).
2. Open <http://127.0.0.1:3000>.
3. **Testnet:** select Testnet, paste the operator **Node Key** from <https://testnet.world-qc.io>, save, then **Start mining**.
4. **Mainnet:** the page stores a wallet address and shows mock status only. P2P mining is not connected yet.

## Data directory

| OS | Default path |
|----|----------------|
| macOS | `~/Library/Application Support/io.world-qc.wqc-miner/` |
| Linux | `~/.local/share/wqc-miner/` |
| Windows | `%APPDATA%\io.world-qc.wqc-miner\` |

Override with `--data-dir PATH`.

First launch creates `settings.toml` there. [`settings.toml.example`](settings.toml.example) in this folder lists the fields. To pre-seed, copy that file to `settings.toml` inside the data directory and edit it.

## Headless

The admin page is plain HTTP on `127.0.0.1`. No display is required.

1. Copy `settings.toml.example` into the data directory as `settings.toml`.
2. Set `network = "testnet"` and `node_key` (testnet), or `wallet_address` (mainnet).
3. Set `auto_start = true`, or pass `--auto-start`.
4. Leave `tn_backend = "cpu"` unless this machine has a working GPU.

If start fails because a credential or binary is missing, the launcher stays up so you can fix settings in the admin page.

Example systemd unit (install this folder at `/opt/wqc-miner`):

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

The admin port has no authentication. Do not bind it on a public address. Use an SSH tunnel (`ssh -L 3000:127.0.0.1:3000 …`) if you need to open the page from another machine.

## If mining does not start

| Message | What to do |
|---------|------------|
| `node_key_missing` | Select Testnet and save a Node Key from the dashboard |
| `wallet_missing` | Select Mainnet and save a wallet address |
| `binaries_missing` | Run `wqc-miner` from this folder so `bin/` stays beside it |
| `bootstrap_unreachable` | Check outbound HTTPS to the bootstrap URL in settings |
| `core_unhealthy` / `core_exited` | Core did not stay up; the launcher retries with backoff |
| `node_exited` | Node exited; mining stops until you start it again |

## License

GNU General Public License v3.0. See [LICENSE](LICENSE) in this folder.
