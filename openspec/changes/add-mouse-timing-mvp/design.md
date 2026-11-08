# Design

## Context

Greenfield repository. Motivation and scope: see proposal.md. Requirements: see `specs/`.

Constraints that shape the approach:
- Windows-only product, but the developer works on macOS. Anything that needs Windows is built, tested and released only on GitHub Actions `windows-latest`. The only code that runs locally on the Mac is the analysis crate's tests.
- GPUI is pre-1.0 (`gpui-component` 0.9.x on `gpui-pre` 0.3.x). Its API changes between versions and its docs are thin, so the examples in Zed's and gpui-component's source are the reference.
- Timing figures must not be distorted by the app itself, through UI frames, allocation or a garbage collector. Rust has no GC; the remaining risk is the UI thread, which is handled by isolating capture.

## Goals / Non-Goals

**Goals:**
- A capture path whose timestamps depend only on when Windows delivers input, not on rendering.
- An analysis crate that is pure, deterministic and testable on any OS from recorded timestamps.
- A release pipeline that produces a self-contained `.exe` from a tag with no manual steps.

**Non-Goals:**
- An abstraction layer for future input sources such as HID and XInput. Add it when the controller change needs it.
- Making the UI build on macOS. A stub capture backend for local UI development is out; the user confirmed the app is Windows-only.
- Persisting runs or settings between launches. Export is the only output.

## Decisions

### Workspace of three crates
`crates/analysis` (no Windows dependencies, `#![forbid(unsafe_code)]`), `crates/capture` (Windows only, all `unsafe` FFI lives here behind a small safe API), `crates/app` (GPUI binary, `#![windows_subsystem = "windows"]`).
- *Why:* isolates every `unsafe` line in one crate, and lets the macOS CI job compile and test `analysis` alone.
- *Alternative:* a single crate with `cfg` gates. Rejected because the boundary between portable and Windows code would erode silently. The non-Windows CI job enforces it.

### Dedicated capture thread with a message-only window
The capture thread creates a `HWND_MESSAGE` window and registers for mouse Raw Input (usage page 0x01, usage 0x02) with `RIDEV_INPUTSINK` (keeps receiving input in the background) and `RIDEV_DEVNOTIFY` (connect and disconnect events). It runs its own `GetMessage` loop at `THREAD_PRIORITY_TIME_CRITICAL`.
- *Why:* GPUI owns the main thread and its event loop. Raw Input arriving there would be timestamped whenever GPUI got around to it.
- *Alternative:* hooking GPUI's window procedure. Rejected: it isn't public API, and it ties timestamps to frame pacing.

### Read with `GetRawInputBuffer`, timestamp once per read
On `WM_INPUT` the thread reads `QueryPerformanceCounter` once, then drains everything pending with `GetRawInputBuffer`. Each report keeps its position in the read (`batch_id`, `index_in_batch`). A read that returns more than one report from the same device marks those reports as batched.
- *Why:* draining the whole buffer is the only way to see batching. A single timestamp per read is honest: the app can't know when each report inside a batch arrived, and the spec counts batching separately for that reason.
- *Alternative:* `GetRawInputData` per message. Simpler, but it hides batching and costs one syscall per report at 8 kHz.

### Integer ticks end to end
Events carry raw QPC ticks (`u64`) plus the QPC frequency recorded once per session. Analysis converts to nanoseconds with integer math (`ticks * 1_000_000_000 / freq` in `u128`). Percentiles use the nearest-rank method on sorted integers. Floating point appears only in display values such as the Hz rate.
- *Why:* makes the deterministic-analysis requirement hold across machines and OSes, and makes the spec's scenarios exact unit tests.

### Lock-free SPSC ring from capture to UI
`rtrb` ring buffer holding 65,536 fixed-size `MouseEvent` records, about 8 s of 8 kHz input. The capture thread pushes. If the ring is full it increments an `AtomicU64` drop counter instead of blocking. The UI side drains it on every frame into a run buffer (`Vec`, reserved when the run starts) and a rolling window for the live view.
- *Why:* the capture thread never waits on the UI or allocates. Dropping and counting makes overflow visible, as the spec requires, rather than turning it into a hidden stall.
- *Alternative:* `crossbeam-channel`. Rejected because it allocates per message and blocks on a full bounded queue.

### Device identity
Devices are keyed by their Raw Input handle for the session. The name comes from `HidD_GetProductString` on the device interface path, falling back to `VID_xxxx&PID_xxxx` parsed from that path. Exports use the first 12 hex digits of the SHA-256 of the interface path, never the path itself.
- *Why:* readable names and stable IDs without exporting full device instance paths, as the no-personal-identifiers scenario requires.

### UI in GPUI with canvas-drawn plots
A single `Entity<AppState>` holds the device list, the selection, the live window, the run state and the last result. A GPUI timer at about 60 Hz drains the ring and calls `cx.notify()`. The interval-over-time plot and the histogram are custom `canvas` elements that draw paths and quads and cap the points drawn per frame by downsampling with min/max per pixel column. Buttons, dropdown, tabs and dialogs come from `gpui-component`. The save dialog uses GPUI's own `prompt_for_new_path`, avoiding an extra dialog crate.
- *Why:* gpui-component's charts are built for business dashboards, and a µs-scale scatter plot of tens of thousands of points needs control over downsampling.

### GPUI source: `gpui-pre` via gpui-component
`gpui-component` 0.9.0 depends on `gpui-pre =0.3.11`, a crates.io snapshot of Zed's GPUI (zed@f816e1f) published by the gpui-component maintainer. The official `gpui` 0.2.2 published by Zed can't be combined with it. The app depends on `gpui-pre` (renamed to `gpui` in Cargo.toml) and `gpui-component` at exact versions.
- *Why:* ready-made widgets (button, dropdown, tabs, dialogs, theming) outweigh building our own. User decision, 2026-11-08.
- *Alternative:* official `gpui` 0.2.2 with hand-built widgets. Rejected: more UI code on an older API.
- *Trade-off:* the core UI crate comes from a single third-party publisher. Pin with `=`, and review the diff on every upgrade.

### Self-contained build
`.cargo/config.toml` sets `target-feature=+crt-static` for `x86_64-pc-windows-msvc`. The MSVC toolchain is pinned in `rust-toolchain.toml`.
- *Why:* the self-contained-executable requirement means no VC++ redistributable on the user's PC.

### No network: enforced, not just claimed
`cargo-deny` runs in CI with a `bans` list (`reqwest`, `hyper`, `ureq`, `isahc`, `curl`, `native-tls`, `rustls`, `tokio` with the `net` feature) and a license allow-list. GPUI's HTTP client trait stays unset.
- *Why:* the no-network requirement is easy to break with one transitive dependency.

### CI and release workflows
- `ci.yml` (push and PR): on `windows-latest`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build`, `cargo test --workspace`, `cargo deny check`. On `macos-latest`, `cargo test -p analysis`.
- `release.yml` (tag `v*.*.*`): checks that the tag equals `workspace.package.version`, runs `cargo build --release`, writes `BUILDINFO.txt` (commit, `rustc -V`, UTC date), and zips the exe with `LICENSE` and `BUILDINFO.txt`. It then computes `SHA256SUMS.txt` over the zip and its contents and publishes with `softprops/action-gh-release`. Every third-party action is pinned to a commit SHA.

## Risks / Trade-offs

- [GPUI on Windows needs the Windows SDK shader compiler at build time] → `windows-latest` ships the SDK. Build on CI only, never cross-compile from macOS. If a future runner image breaks it, pin the runner image version.
- [GPUI API churn breaks the build on upgrade] → pin exact versions with `=x.y.z` and upgrade deliberately in a dedicated change.
- [The developer can't run the app locally] → every Windows behavior gets an integration smoke test in CI where feasible (device enumeration, a window that opens and closes), plus a manual test checklist in tasks.md for real hardware on a Windows PC.
- [`THREAD_PRIORITY_TIME_CRITICAL` could starve other work if the loop spins] → the loop blocks in `GetMessage`, so it uses no CPU while idle.
- [One timestamp per read hides spacing inside a batch] → by design: batching is reported as its own figure, and batched reports never produce fake 0 µs gaps. Analysis computes intervals only from the first report of each batch to the next read.
- [Unsigned exe triggers SmartScreen] → documented in the README; code signing is a later change.
- [GitHub-hosted runners are VMs with no real mouse] → CI tests cover analysis against synthetic timestamp streams and capture plumbing against recorded fixtures. Real-device timing is checked only manually.

## Open Questions

- Final product name and icon (working name `pollscope`). Renaming is a find-and-replace before `v0.1.0`.
- Whether the live view should also offer a "raw report count per frame" readout. That's cosmetic and can be decided while building the UI.
