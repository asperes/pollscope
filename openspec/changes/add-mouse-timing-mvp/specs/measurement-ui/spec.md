# Spec Delta

## Purpose

The desktop window where a user picks a mouse, watches its delivery timing live, runs a timed measurement, reads the result and exports it.

## ADDED Requirements

### Requirement: Select the device to measure
The UI SHALL show the connected mice and let the user select exactly one to measure. With a single mouse connected it SHALL be selected automatically.

#### Scenario: One mouse
- **WHEN** the app starts with one mouse connected
- **THEN** that mouse is already selected

#### Scenario: Pick by moving
- **WHEN** several mice are connected and the user chooses "Detect" and then moves one mouse
- **THEN** the mouse that moved becomes the selected device

### Requirement: Live timing view
While a device is selected, the UI SHALL show its current report rate, a plot of recent report intervals over time, and a histogram of recent intervals, updating at least 30 times per second.

#### Scenario: Moving the mouse
- **WHEN** the user moves the selected mouse
- **THEN** the rate, interval plot and histogram update while it moves

#### Scenario: Mouse at rest
- **WHEN** the selected mouse is not moved
- **THEN** the rate shows "—" rather than 0 Hz or a stale value

### Requirement: Timed measurement run
The UI SHALL let the user start a measurement of 5, 10 or 30 seconds with a visible countdown and an instruction to move the mouse continuously, and SHALL let the user cancel it.

#### Scenario: Complete run
- **WHEN** the user starts a 10-second run and keeps moving the mouse
- **THEN** after 10 seconds the result for that run is shown

#### Scenario: Cancelled run
- **WHEN** the user cancels a run before it ends
- **THEN** no result is shown and nothing is saved

### Requirement: Show the result
After a run, the UI SHALL show the delivery-timing figures, the gap and batching counts, and any "not enough data" or "not trustworthy" verdict with its reason in plain language.

#### Scenario: Untrustworthy run
- **WHEN** a run is marked not trustworthy
- **THEN** the reason is shown next to the figures, not only in a tooltip or log

### Requirement: State what is measured
The UI SHALL state, on the result view and in the About view, that figures describe when Windows delivered reports to this app on this PC — not the device's internal polling or scan rate and not click-to-screen latency.

#### Scenario: Viewing a result
- **WHEN** a result is displayed
- **THEN** the measurement-scope statement is visible on the same view

### Requirement: Export a run
The UI SHALL let the user export a finished run as JSON (device identity, app version, run settings, every recorded timestamp, the computed result) and as CSV (one row per report), written to a location the user chooses.

#### Scenario: Export JSON
- **WHEN** the user exports a finished run as JSON
- **THEN** the file contains the full timestamp list and the same figures shown in the UI

#### Scenario: No personal identifiers
- **WHEN** any export is written
- **THEN** it contains no Windows user name, computer name or full device instance path

### Requirement: No network access
The app SHALL NOT make network connections.

#### Scenario: Normal use
- **WHEN** the app is started, used for runs and exported from
- **THEN** it opens no network connections
