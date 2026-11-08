# mouse-capture Specification

## Purpose
Discovers the mice connected to Windows and records every Raw Input mouse report with a host timestamp taken on arrival, so timing analysis works from what Windows actually delivered.

## Requirements

### Requirement: List connected mice
The system SHALL list every Raw Input mouse device currently connected, each with a stable per-session identifier and a human-readable name, falling back to the device's vendor and product IDs when Windows provides no name.

#### Scenario: Two mice connected
- **WHEN** two mice are connected and the device list is requested
- **THEN** both appear as separate entries with distinct identifiers

#### Scenario: Device without a product string
- **WHEN** a mouse exposes no product name
- **THEN** its entry shows its vendor and product IDs in hexadecimal instead of an empty name

### Requirement: Refresh on device change
The system SHALL update the device list when a mouse is connected or disconnected, without restarting the app.

#### Scenario: Mouse unplugged while listed
- **WHEN** a listed mouse is unplugged
- **THEN** it is removed from the device list within 2 seconds

#### Scenario: Mouse unplugged during a run
- **WHEN** the mouse being measured is unplugged during a timed run
- **THEN** the run ends early and is marked as interrupted by device removal

### Requirement: Timestamp each report on arrival
The system SHALL record, for every mouse report Windows delivers, the source device, the relative X/Y motion, button-state changes, and a monotonic high-resolution host timestamp with sub-microsecond resolution taken when the report is read.

#### Scenario: Mouse moved
- **WHEN** the user moves the selected mouse
- **THEN** each delivered report is recorded with its device, its motion and its own timestamp

#### Scenario: Timestamps are monotonic
- **WHEN** reports are recorded
- **THEN** timestamps for a device never decrease

### Requirement: Capture reports while unfocused
The system SHALL keep recording reports while its window is in the background or unfocused.

#### Scenario: Window loses focus
- **WHEN** the user clicks another window during a run
- **THEN** reports keep being recorded without a gap caused by the focus change

### Requirement: Detect batched delivery
The system SHALL record when Windows hands over several reports from one device in a single read, so they can be identified as batched rather than individually delivered.

#### Scenario: Batched read
- **WHEN** a single read returns more than one report for the same device
- **THEN** each of those reports is recorded with a marker identifying the batch it arrived in

### Requirement: Capture is isolated from the UI
The system SHALL record reports independently of UI rendering, so a slow or stalled frame does not delay, drop or reorder recorded reports.

#### Scenario: UI stall
- **WHEN** the UI is blocked for 500 ms during a run
- **THEN** no reports from that period are lost and their timestamps reflect their arrival, not the UI's recovery

### Requirement: Report capture overflow
The system SHALL count any reports it had to drop because its buffer was full and SHALL expose that count with the run's results.

#### Scenario: Buffer overflow
- **WHEN** reports arrive faster than they can be stored and some are dropped
- **THEN** the run's result shows a non-zero dropped-report count and is marked as not trustworthy
