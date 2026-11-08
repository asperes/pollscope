# pollscope

A small Windows app that shows **how evenly a mouse's reports reach Windows**: the steadiness of the intervals, the gaps, and reports delivered in batches. It goes beyond a single "polling rate" number.

## What it measures, and what it doesn't

pollscope timestamps every Raw Input mouse report the moment Windows hands it to the app, using the high-resolution performance counter.

- **It measures:** host-side delivery timing on this PC. That means intervals between reports, how much they vary (p50/p95/p99 deviation from the median), gaps of at least twice the median interval, and reports Windows delivered together in one read.
- **It doesn't measure:** the mouse's internal polling or sensor scan rate, USB timing on the wire, or click-to-screen latency.

Results describe one mouse, on one PC, under one load. When there isn't enough data (fewer than 500 intervals) the app says so instead of showing numbers. A run is marked as not trustworthy if reports were dropped or the mouse was disconnected mid-run.

pollscope runs entirely offline. It opens no network connections, which CI enforces with a dependency ban list, and it changes no system settings.

## Download

Get the latest `pollscope-vX.Y.Z-windows-x64.zip` from [Releases](https://github.com/asperes/pollscope/releases). Unzip it and run `pollscope.exe`. Nothing needs installing.

Windows 10 22H2 or later, or Windows 11, on x64.

**SmartScreen:** the executable isn't code-signed yet, so Windows may show "Windows protected your PC". Click **More info → Run anyway**, after checking the hash as described below.

### Verify the download

Each release has a `SHA256SUMS.txt` listing the zip and every file inside it.

PowerShell:

```powershell
Get-FileHash .\pollscope-v0.1.0-windows-x64.zip -Algorithm SHA256
```

Compare the hash with the zip's line in `SHA256SUMS.txt`. `BUILDINFO.txt` inside the zip names the exact source commit and Rust version used.

## Using it

1. Pick your mouse. With only one connected, it's already selected. With several, click **Detect** and move the one you want.
2. Move the mouse and watch the live rate, the interval plot and the histogram.
3. Choose 5, 10 or 30 seconds, click **Start measurement**, and keep moving the mouse in circles until the countdown ends.
4. Read the result, then use **Export JSON** (every timestamp plus the result) or **Export CSV** (one row per report) if you want to keep it. Exports contain no user name, computer name or device path; the mouse is identified by its product name and a short hashed id.

## Building

Windows builds run on GitHub Actions; see [`.github/workflows`](.github/workflows).

- `ci.yml` runs on every push and pull request. On Windows: fmt, clippy (`-D warnings`), build, a check that the exe needs no runtime redistributable, and tests. On macOS: the platform-independent `analysis` tests. On Linux: `cargo deny` (licenses, advisories, network-crate bans).
- `release.yml` runs when a tag `vX.Y.Z` (or `vX.Y.Z-rc.N`) matching the version in `Cargo.toml` is pushed. It builds, packages and publishes a GitHub Release with `SHA256SUMS.txt`.

To build locally on Windows (Rust is installed from `rust-toolchain.toml` automatically):

```powershell
cargo build --release -p pollscope
```

The `analysis` crate builds and tests on any OS:

```sh
cargo test -p analysis
```

## Project layout

| Path | What |
|---|---|
| `crates/analysis` | Pure timing analysis: intervals, idle pauses, percentiles, gaps, batching, verdicts |
| `crates/capture` | Windows Raw Input capture on a dedicated thread, device enumeration |
| `crates/app` | GPUI (gpui-kit) desktop app |
| `openspec/` | Specs and change proposals ([OpenSpec](https://openspec.dev)) |
| `docs/manual-test.md` | Hardware checklist for a real Windows PC |

## License

MIT
