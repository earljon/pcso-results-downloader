# PCSO Results Downloader

A small Rust CLI that downloads the daily lotto results page from
[pcso.gov.ph](https://www.pcso.gov.ph/SearchLottoResult.aspx) and archives it
to S3 at `s3://your-bucket/results/downloads/<yyyy>/<Month>/<MM-dd-yyyy>.html`.

Works on macOS, Linux (Ubuntu / DietPi on Raspberry Pi), and Windows. Designed
to run unattended in headless mode — typically from cron on a Raspberry Pi.

## Features

- Single date or inclusive date range (`--from` / `--to`).
- Default date is **today** in `Asia/Manila` (the timezone PCSO publishes in).
- Sequential, polite to the PCSO server.
- Akamai-resistant: matches Playwright's launch flags + uses old `--headless`
  mode, which gets past the PCSO WAF where new-headless does not.
- Persistent Chromium profile next to the binary, so Akamai's clearance
  cookies survive between runs.
- 3-attempt retry with exponential back-off per date; on terminal failure
  prints a resume hint with the failing date.
- Uploads to S3 using the standard AWS credentials chain — supports
  `--profile`, `--region`, and any `~/.aws/credentials` setup.
- Single static-ish binary; no Node, Docker, or runtime dependency beyond
  Chromium itself.

## Quick start (macOS)

```bash
# One-time: tell the Makefile which S3 bucket to upload into.
export BUCKET=your-bucket

# Build + run for today (Asia/Manila), headless:
make run-mac

# Or for a specific range:
make run-mac-date FROM=06-25-2026 TO=06-30-2026
```

The result lands in `s3://$BUCKET/results/downloads/<yyyy>/<Month>/<MM-dd-yyyy>.html`.
Add `export BUCKET=…` (and `export AWS_PROFILE=…` if you use a non-default
AWS profile) to your `~/.zshrc` so you don't have to repeat it every shell.

## CLI reference

```
Usage: pcso-results-downloader [OPTIONS]

Options:
      --from <FROM>                Start date. `MM-dd-yyyy` (single day) or `MonthName-yyyy` (whole month).
                                   Defaults to today in Asia/Manila.
      --to <TO>                    End date, inclusive. Same formats as --from; `MonthName-yyyy`
                                   resolves to the last day of the month. Defaults to --from.
      --headed                     Show the browser window (default: headless).
      --minimize                   When --headed, minimize the window. Requires --headed.
      --bucket <BUCKET>            S3 bucket to upload into. **Required.**
      --profile <PROFILE>          AWS profile name from ~/.aws/credentials.
      --region <REGION>            AWS region of the target bucket. [default: us-east-1]
      --profile-dir <PROFILE_DIR>  Persistent Chromium profile directory.
                                   Defaults to .pcso-profile next to the binary;
                                   falls back to PCSO_PROFILE_DIR env var.
  -h, --help                       Print help.
  -V, --version                    Print version.
```

### Common invocations

`--bucket` is required on every run. Pick the AWS profile via `--profile`
or via your shell's `AWS_PROFILE` env var.

```bash
# Today, headless, default AWS chain
pcso-results-downloader --bucket your-bucket

# Specific date with a named AWS profile
pcso-results-downloader --bucket your-bucket --from 06-30-2026 --profile my-profile

# Day-precision range
pcso-results-downloader --bucket your-bucket --from 06-01-2026 --to 06-30-2026

# Whole month — March 1 through March 31, 2022
pcso-results-downloader --bucket your-bucket --from March-2022

# Range of months — March 1 through June 30, 2022
pcso-results-downloader --bucket your-bucket --from March-2022 --to June-2022

# Mixing: from a specific day through the end of a month
pcso-results-downloader --bucket your-bucket --from 03-05-2026 --to June-2026

# Visible (minimized) window — handy on macOS while debugging
pcso-results-downloader --bucket your-bucket --from 06-30-2026 --headed --minimize

# Custom profile directory
pcso-results-downloader --bucket your-bucket --profile-dir /var/cache/pcso
```

#### Date / month spec format

- `MM-dd-yyyy` — single day (e.g. `06-30-2026`).
- `MonthName-yyyy` — full English month name or 3-letter abbreviation,
  case-insensitive (e.g. `March-2026`, `mar-2026`, `MARCH-2026`).
- When used as `--from`, a month spec expands to the **first** day of the month.
- When used as `--to`, it expands to the **last** day of the month.
- Mixing forms across `--from` and `--to` is allowed.

### S3 path produced

`s3://your-bucket/results/downloads/2026/June/06-30-2026.html`

Always overwrites existing objects for the same date.

### Exit behavior

- All dates succeed: exit 0.
- Any date fails 3 times: process exits non-zero with
  `re-run with --from <failed_date> to resume.` Subsequent runs simply
  re-process the requested dates (the tool always overwrites in S3 — it is
  the caller's responsibility to narrow `--from` when resuming).

## Prerequisites

### Common to all platforms

| Component | Purpose |
|---|---|
| **Rust toolchain** (rustup) | Build the binary. Not needed on the Pi if you cross-compile. |
| **Chromium or Chrome** | Browser driver target. Auto-detected. |
| **AWS credentials** with `s3:PutObject` on the target bucket | S3 upload. |

### macOS

```bash
# Rust via rustup (preferred over Homebrew's rust):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"

# Chrome — install Google Chrome from https://www.google.com/chrome/
# (Auto-detected at /Applications/Google Chrome.app/Contents/MacOS/Google Chrome)

# AWS CLI (optional but useful for verifying uploads):
brew install awscli

# Configure your profile:
aws configure --profile my-profile
# Or hand-edit ~/.aws/credentials + ~/.aws/config
```

For cross-compiling to the Pi, also install `zig` and the build helper:

```bash
brew install zig
cargo install cargo-zigbuild
rustup target add aarch64-unknown-linux-gnu
```

### Linux (Ubuntu / Debian)

```bash
sudo apt update
sudo apt install -y curl build-essential pkg-config ca-certificates chromium-browser awscli

# Rust:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"

# Configure AWS:
aws configure --profile my-profile
```

### Raspberry Pi (DietPi or Raspberry Pi OS)

Designed to **run** the binary, not build it. Cross-compile on your Mac /
Linux dev machine and `rsync` over.

On the Pi:

```bash
sudo apt update
sudo apt install -y chromium ca-certificates rsync

# AWS credentials:
mkdir -p ~/.aws
cat > ~/.aws/credentials <<'EOF'
[my-profile]
aws_access_key_id     = AKIA...
aws_secret_access_key = ...
EOF
cat > ~/.aws/config <<'EOF'
[profile my-profile]
region = us-east-1
EOF
chmod 600 ~/.aws/credentials
```

`chromium` on DietPi installs to `/usr/lib/chromium/chromium`; chromiumoxide
auto-detects it. If detection ever fails, point at it explicitly:

```bash
export CHROME=/usr/lib/chromium/chromium
```

### Windows

```powershell
# Rust via rustup (winget):
winget install Rustlang.Rustup

# Chrome — install from https://www.google.com/chrome/

# AWS CLI:
winget install Amazon.AWSCLI

# Configure your profile:
aws configure --profile my-profile
```

Cross-compiling from Windows to the Pi works but is fiddlier than from
macOS/Linux. Recommended: build the Pi binary on your Mac/Linux dev box and
copy it over. For native Windows builds:

```powershell
cargo build --release
.\target\release\pcso-results-downloader.exe --from 06-30-2026 --profile my-profile
```

## Building from source

Local build for the host platform:

```bash
cargo build --release
./target/release/pcso-results-downloader --help
```

Or via Make:

```bash
make build-mac           # macOS / native build
make build               # cross-compile for Pi (see below)
make test                # run unit tests
make check               # fast type-check, no binary
```

## Cross-compiling for Raspberry Pi (from macOS)

The repo's `Makefile` is set up for `cargo-zigbuild`, which avoids Docker and
runs natively on Apple Silicon.

```bash
# One-time setup:
brew install zig
cargo install cargo-zigbuild
rustup target add aarch64-unknown-linux-gnu
```

Find your Pi's architecture + glibc:

```bash
make pi-info
# Prints: aarch64 + e.g. ldd (Debian GLIBC 2.36-9+rpt2+deb12u13)
```

Then build + deploy:

```bash
make deploy
```

What that does:

1. `cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36`
2. `ssh dietpi "mkdir -p /mnt/apps/pcso"`
3. `rsync -avz --progress target/aarch64-unknown-linux-gnu/release/pcso-results-downloader dietpi:/mnt/apps/pcso/`
4. `ssh dietpi "chmod +x /mnt/apps/pcso/pcso-results-downloader"`

Override variables when needed:

```bash
make build TARGET=armv7-unknown-linux-gnueabihf GLIBC=2.31   # 32-bit Pi 2/3
make deploy SSH_HOST=other-pi PI_BIN_DIR=/opt/pcso
```

## Makefile reference

```
make help                  Show every target with its one-line description.

# Build
make build                 Cross-compile for the Pi (TARGET, GLIBC overridable).
make build-mac             Local release build.
make check                 cargo check.
make test                  cargo test.
make clean                 cargo clean.

# Local (Mac) install / run
make install-mac           Build + copy binary to /usr/local/bin (MAC_INSTALL_DIR overridable).
make run-mac               Build + run for today, headless.
make run-mac-headed        Build + run with a minimized visible browser window.
make run-mac-date FROM=…   Build + run for a specific date / range.

# Pi deploy / run
make deploy                Build + rsync to /mnt/apps/pcso on the SSH_HOST.
make deploy-only           Skip rebuild; just rsync the existing binary.
make run-pi                Run the deployed binary on the Pi for today.
make run-pi-date FROM=…    Run the deployed binary for a specific date / range.
make ssh                   Open an SSH shell on the Pi.
make pi-info               Show the Pi's arch + glibc.
```

Variables (override on the command line or export in your environment):

| Variable | Default | Purpose |
|---|---|---|
| `TARGET` | `aarch64-unknown-linux-gnu` | Rust cross target. |
| `GLIBC` | `2.36` | Zigbuild target glibc; match the Pi's. |
| `SSH_HOST` | `dietpi` | SSH alias (from `~/.ssh/config`) or `user@host`. |
| `PI_BIN_DIR` | `/mnt/apps/pcso` | Where the binary lands on the Pi. |
| `BUCKET` | _empty_ | **Required.** Passed as `--bucket <name>`. Every `run-*` target fails fast if it isn't set. Export once in your shell: `export BUCKET=your-bucket`. |
| `PROFILE` | _empty_ | Passed as `--profile <name>` when set; otherwise the binary uses the default AWS credential chain (`AWS_PROFILE` env var → `[default]` in `~/.aws/credentials`). |
| `MAC_INSTALL_DIR` | `/usr/local/bin` | Where `make install-mac` writes the binary. |

## Scheduling with cron (Pi)

```bash
crontab -e
```

Run once a day at 22:30 Asia/Manila, archive to `/var/log/pcso.log`:

```
30 22 * * * /mnt/apps/pcso/pcso-results-downloader --bucket your-bucket --profile my-profile >> /var/log/pcso.log 2>&1
```

cron has a minimal `PATH`; always use the absolute path to the binary.

## How it works

```
┌────────────┐    ┌────────────────────────────┐    ┌──────────┐
│ user / cron│ →  │ pcso-results-downloader CLI │ →  │   AWS S3 │
└────────────┘    └────────────────────────────┘    └──────────┘
                          │
                          ↓
                  ┌───────────────┐
                  │ headless Chrome│  (chromiumoxide via CDP)
                  └───────────────┘
                          │
                          ↓
                 pcso.gov.ph/SearchLottoResult.aspx
                  (ASP.NET WebForms postback)
```

### Per-date pipeline

1. Parse `--from` / `--to`; default to today in `Asia/Manila`.
2. Launch Chromium once with the persistent profile dir.
3. For each date:
   1. `goto` the PCSO search page; wait 3 s for any JS challenge.
   2. Fill the six date dropdowns (From + To both set to the same date) by
      content (`January`…`December`, `1`…`31`, year list) — avoids brittle
      ASP.NET IDs.
   3. Click "Search Lotto"; wait for the WebForms postback to replace the
      document.
   4. Grab the full rendered HTML via `page.content()`.
   5. `PutObject` to `s3://<bucket>/results/downloads/<yyyy>/<Month>/<MM-dd-yyyy>.html`.
   6. On failure: retry up to 3 times with 2 s / 4 s backoff. After the
      third failure: abort the whole run with an actionable message.
4. Close the browser.

### Module layout

```
src/
├── main.rs        Tokio runtime + tracing init + top-level error handling
├── cli.rs         clap::Parser struct for CLI flags
├── dates.rs       Date parsing, Asia/Manila "today", range iteration
├── browser.rs     chromiumoxide launch + form fill + postback wait
├── s3.rs          AWS S3 client construction + key/body upload
├── pipeline.rs    Per-date retry loop + range orchestration
└── error.rs       Typed error enum (thiserror)
```

### Akamai notes

`pcso.gov.ph` sits behind Akamai bot management. To get through headlessly,
the launch must use **old** `--headless` (not `--headless=new`) and match
Playwright's flag set fairly closely. The repo's `browser.rs` is the canonical
record of which exact flags are required. If you ever see "Access Denied"
again:

```bash
# Run with debug logging and inspect what the page actually returned:
RUST_LOG=debug ./pcso-results-downloader --bucket your-bucket --from 06-30-2026 \
    2>&1 | grep -E 'diagnosis|attempt'
```

A `"title": "Access Denied"` in the diagnosis line means Akamai blocked at
the HTTP layer — re-check the launch flags against `ps -ef | grep chromium`
output from a known-good Playwright run.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `Browser process exited … Running as root without --no-sandbox is not supported` | The binary should pass `--no-sandbox` automatically. If you see this, the deployed binary is stale. Re-run `make deploy`. |
| `Access Denied` page on every attempt | Akamai blocking. Likely the chromiumoxide launch flags are wrong (see Akamai notes), or you redeployed and forgot to refresh the binary. |
| `AccessDenied` from S3 | IAM permissions on the bucket. Verify with `aws s3 ls s3://your-bucket/results/downloads/ --profile my-profile`. |
| `PermanentRedirect` from S3 | Wrong region. Pass `--region <correct-region>` or set it in your AWS profile. |
| `cannot execute binary file` on the Pi | Wrong arch. Check `uname -m` on the Pi and rebuild with the matching `TARGET=`. |
| `GLIBC_x.y not found` on the Pi | The Pi's glibc is older than what you built against. Rebuild with `make build GLIBC=<pi-version>` — see `make pi-info`. |
| Browser window briefly steals focus on macOS | Use `--minimize` with `--headed`. To avoid the window entirely, run headless: drop `--headed`. |
| `ssh: Could not resolve hostname dietpi` | Add a `Host dietpi` block to your `~/.ssh/config`, or override `SSH_HOST=user@1.2.3.4` per make invocation. |

## Project layout

```
pcso-results-downloader/
├── Cargo.toml          Dependencies + crate metadata
├── Cargo.lock          Pinned dependency versions (commit this)
├── Makefile            Build / deploy / run helpers
├── README.md           This file
├── src/                Source modules (see "Module layout" above)
└── target/             Build artifacts (gitignored)
```

## Dependencies (Cargo)

| Crate | Why |
|---|---|
| `clap` (derive) | CLI parsing |
| `chrono` + `chrono-tz` | Date math; `Asia/Manila` timezone |
| `tokio` | Async runtime |
| `chromiumoxide` | Headless Chrome via CDP (no Node required) |
| `aws-config`, `aws-sdk-s3` | S3 PUT using default cred chain |
| `tracing`, `tracing-subscriber` | Structured logging to stderr |
| `thiserror` | Typed errors |
| `anyhow` | Error bubbling in `main` |
| `futures` | StreamExt for the chromiumoxide handler loop |
| `serde_json` | Reading values returned from `page.evaluate(...)` |

## License

MIT — see [LICENSE](LICENSE).
