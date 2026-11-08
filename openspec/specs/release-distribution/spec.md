# release-distribution Specification

## Purpose
Checks every change automatically and publishes tagged versions as GitHub Releases whose files users can verify.

## Requirements

### Requirement: Check every push and pull request
On every push and pull request, CI SHALL check formatting, run the linter with warnings treated as errors, build the Windows app, and run all tests; a failure in any step SHALL fail the check.

#### Scenario: Lint warning
- **WHEN** a pull request introduces a linter warning
- **THEN** its CI check fails

#### Scenario: Clean change
- **WHEN** a pull request passes formatting, lint, build and tests
- **THEN** its CI check passes

### Requirement: Platform-independent logic is tested off Windows
CI SHALL also run the tests of the platform-independent analysis code on a non-Windows runner.

#### Scenario: Analysis depends on Windows by mistake
- **WHEN** a change makes the analysis code require Windows-only APIs
- **THEN** the non-Windows CI job fails

### Requirement: Publish a release from a version tag
Pushing a tag of the form `vMAJOR.MINOR.PATCH` SHALL build the app in release mode and publish a GitHub Release for that tag containing a zip of the app and a `SHA256SUMS.txt` covering every file in the release.

#### Scenario: Tag pushed
- **WHEN** the tag `v0.1.0` is pushed
- **THEN** a GitHub Release `v0.1.0` appears with `<app>-v0.1.0-windows-x64.zip` and `SHA256SUMS.txt`

#### Scenario: Tag does not match the app version
- **WHEN** a tag's version differs from the app's declared version
- **THEN** the release job fails and nothing is published

### Requirement: Verifiable release contents
The released zip SHALL contain the app executable, the license, and a build-info file naming the source commit, the Rust toolchain version and the build date.

#### Scenario: Checking a download
- **WHEN** a user hashes the downloaded zip
- **THEN** the hash matches the entry in that release's `SHA256SUMS.txt`

### Requirement: Self-contained executable
The released app SHALL run on a clean supported Windows installation without requiring the user to install a runtime or redistributable.

#### Scenario: Fresh Windows machine
- **WHEN** the unzipped executable is started on a clean Windows 11 install
- **THEN** the app window opens without a missing-DLL or runtime error
