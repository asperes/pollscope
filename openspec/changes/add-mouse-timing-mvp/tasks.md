# Tasks

## 1. Workspace scaffold

- [x] 1.1 Create the Cargo workspace (`crates/analysis`, `crates/capture`, `crates/app`) with a shared `workspace.package.version = "0.1.0"`, the MIT `LICENSE`, `.gitignore` and `rust-toolchain.toml` (pinned stable, `x86_64-pc-windows-msvc` target); verify `cargo metadata --no-deps` lists the three members
- [x] 1.2 Add `.cargo/config.toml` with `+crt-static` for `x86_64-pc-windows-msvc`, and pin exact (`=x.y.z`) versions of `gpui`, `gpui-component`, `windows`, `rtrb`, `serde`, `serde_json`, `csv`, `sha2`; verify `cargo tree -p analysis` on macOS shows no Windows or GPUI crates
- [x] 1.3 Add `deny.toml` (network-crate bans and a license allow-list from design.md); verify `cargo deny check` passes locally

## 2. CI pipeline (needed early: Windows code compiles only on CI)

- [ ] 2.1 Add `.github/workflows/ci.yml`: a `windows-latest` job (fmt, clippy `-D warnings`, build, test, `cargo deny check`) and a `macos-latest` job (`cargo test -p analysis`), with every third-party action pinned to a commit SHA; verify with `actionlint` locally
- [ ] 2.2 Create the GitHub repository (with the user's confirmation of name and visibility), push `main`, and verify both CI jobs pass on the scaffold

## 3. Delivery-timing analysis (`crates/analysis`, tested on macOS)

- [ ] 3.1 Define the recorded-run types (`MouseEvent` with device key, dx/dy, buttons, QPC ticks, batch id and index; `RecordedRun` with QPC frequency, settings and drop count), deriving serde; verify a JSON round-trip test
- [ ] 3.2 Implement arrival grouping and per-device intervals in integer nanoseconds; verify unit tests for the single-device, interleaved-devices and batched-reports scenarios
- [ ] 3.3 Implement idle-pause exclusion (≥ 20× median or ≥ 50 ms) and the statistics summary (count, median, implied rate, nearest-rank p50/p95/p99 absolute deviation); verify tests for the steady-stream, outliers and user-stops-moving scenarios
- [ ] 3.4 Implement gap counting (≥ 2× median, longest, per 10,000), the batched share, the "not enough data" rule (< 500 intervals) and the untrustworthy flags with reasons; verify tests for each scenario in `specs/delivery-timing-analysis/spec.md`
- [ ] 3.5 Add a property test (`proptest`) showing that re-analysing the same run gives identical results, plus synthetic-stream fixtures (1 kHz steady, 8 kHz jittery, 1 kHz with idle and gaps); verify `cargo test -p analysis` passes on macOS and on the CI Windows job

## 4. Raw Input capture (`crates/capture`, Windows)

- [ ] 4.1 Implement device enumeration (`GetRawInputDeviceList` plus `GetRawInputDeviceInfoW`) with names via `HidD_GetProductString` and the `VID_xxxx&PID_xxxx` fallback, plus the hashed export id; verify a unit test of the VID/PID parser on sample interface paths, and a CI test that enumeration returns `Ok` (possibly empty) on the runner
- [ ] 4.2 Implement the capture thread: message-only window, `RegisterRawInputDevices` with `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY`, `GetMessage` loop at `THREAD_PRIORITY_TIME_CRITICAL`, and clean shutdown via `PostThreadMessage`; verify a CI test that starts and stops the thread within 1 s without leaks or panics
- [ ] 4.3 On `WM_INPUT`, read QPC once and drain with `GetRawInputBuffer` (with correct `NEXTRAWINPUTBLOCK` alignment), tagging batch ids; push into the `rtrb` ring and count drops; verify unit tests of the buffer-walking code against a recorded byte fixture, including a multi-report batch
- [ ] 4.4 Handle `WM_INPUT_DEVICE_CHANGE` arrival and removal events and expose them to consumers; verify with a unit test of the event translation, and record the unplug scenario in the manual checklist (6.2)
- [ ] 4.5 Document the capture crate's safety invariants in module-level docs (every `unsafe` block has a `// SAFETY:` comment); verify `clippy::undocumented_unsafe_blocks` is enabled and passes

## 5. GPUI measurement UI (`crates/app`, Windows)

- [ ] 5.1 Create the GPUI application shell (window, title, `windows_subsystem = "windows"`, `AppState` entity, 60 Hz drain timer) wired to the capture crate; verify the CI Windows job builds the release binary
- [ ] 5.2 Build device selection: a dropdown of mice, auto-select when there is only one, "Detect" (next mouse that moves), and list refresh on connect/disconnect; verify a unit test of the selection state machine
- [ ] 5.3 Build the live view: current rate (or "—" at rest), a canvas interval-over-time plot with min/max-per-column downsampling, and a canvas histogram; verify a unit test of the downsampler, plus manual checklist items for both visuals
- [ ] 5.4 Build timed runs (5/10/30 s, countdown, move-the-mouse instruction, cancel, early end on device removal) feeding `analysis`; verify a unit test of the run state machine, including the cancel and device-removed paths
- [ ] 5.5 Build the result view showing every figure from the analysis spec, the verdicts with reasons inline, and the measurement-scope statement, plus an About view repeating the scope; verify a snapshot-style test of the formatted result text for a fixture run
- [ ] 5.6 Implement JSON and CSV export through GPUI's save-path prompt, with no user or computer name and only the hashed device id; verify a test that exports a fixture run and checks the content (including the absence of `USERNAME`, `COMPUTERNAME` and raw interface paths)

## 6. Release pipeline and integration

- [ ] 6.1 Add `.github/workflows/release.yml` (tag `v*.*.*`: version check, release build, `BUILDINFO.txt`, zip, `SHA256SUMS.txt`, GitHub Release via a SHA-pinned action); verify with `actionlint`, then push a `v0.1.0-rc.1` tag and confirm a pre-release with the expected files appears
- [ ] 6.2 Write `docs/manual-test.md`, a hardware checklist for a real Windows PC: launch on a clean machine, one mouse auto-selected, Detect with two mice, live view while moving, focus loss during a run, unplug during a run, 1 kHz and higher-rate mice, export files open correctly; verify by running it on a Windows PC with the rc build and recording the results in the file
- [ ] 6.3 Write `README.md`: what the tool measures and doesn't (scope statement), download and SHA256 verification steps, the SmartScreen note, and how to build via CI; verify every command in it runs as written
- [ ] 6.4 Run `openspec validate add-mouse-timing-mvp --strict` and verify it passes

## Workflow follow-up

- After the manual checklist passes, tag `v0.1.0` to publish the first public release.
- Archive the change with `/opsx:archive` once released.
