# Proposal

## Why

There is no open, Windows-native tool that shows how evenly a mouse's reports reach Windows (interval steadiness, gaps, batching) rather than just a single "polling rate" number. This first change establishes the project — a Rust + GPUI Windows app — with the smallest useful slice: live mouse delivery-timing measurement, plus an automated pipeline that ships it as a GitHub Release, so every later feature lands on a working, distributable base.

## What Changes

- New Cargo workspace for a Windows-only desktop app (working name **`pollscope`**, changeable before the first release).
- Raw Input mouse capture on a dedicated high-priority thread with a hidden message-only window; every report is timestamped with QPC the moment Windows hands it over, tagged with its device.
- Device picker listing connected mice by name, with an "all mice" view that never mixes devices inside one measurement.
- Live view: report rate, interval plot over time, interval histogram, drawn with GPUI.
- Timed measurement run (5 / 10 / 30 s) that produces a result: median interval, implied rate, jitter (p50/p95/p99 deviation), gap count, batched-report count, and a "not enough data" state instead of a number when the sample is too small.
- Plain-language scope statement in the UI: host-side Raw Input delivery timing — not device polling, not click-to-screen latency.
- Export a finished run as JSON and CSV.
- GitHub Actions: CI on every push/PR (fmt, clippy, tests on `windows-latest`; pure-logic tests also run on macOS); tag `v*` builds a release `.exe`, zips it with `SHA256SUMS.txt`, and publishes a GitHub Release.

Out of scope for this change: controllers/HID/XInput, USB tree, power or system settings, ETW/interrupt tracing, any system modification, admin elevation, installer, code signing, auto-update.

## Capabilities

### New Capabilities
- `mouse-capture`: discovering mice and capturing timestamped Raw Input reports per device without blocking on the UI.
- `delivery-timing-analysis`: turning a sequence of report timestamps into interval statistics, gap/batch detection and data-sufficiency verdicts.
- `measurement-ui`: the GPUI window — device selection, live timing view, timed runs, result display and export.
- `release-distribution`: CI checks and tagged GitHub Releases with verifiable artifacts.

### Modified Capabilities
<!-- none: greenfield project -->

## Impact

- New repository layout: `crates/capture`, `crates/analysis`, `crates/app`, `.github/workflows/`.
- Dependencies (exact pins chosen at implementation): `gpui-component` 0.9.x on `gpui-pre` 0.3.x (a GPUI snapshot; see design.md), `windows` (Win32 Raw Input / UI / Performance features), a lock-free SPSC ring buffer (`rtrb`), `serde`/`serde_json`, `csv`.
- Builds require a Windows toolchain; developer machine is macOS, so Windows builds and release artifacts come only from GitHub Actions `windows-latest` runners.
- Unsigned `.exe` → Windows SmartScreen will warn on first run; documented in the README.
