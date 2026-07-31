# UI Backend Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect the existing egui workspaces to ROS2 Radar, Laser, SDR process control and observable Serial state without modifying the Serial/ZMQ backends or any external repository.

**Architecture:** A dedicated Tokio actor owns `ScriptRunner`, receives `ProcessCommand` through `mpsc`, and publishes immutable `ProcessSnapshot` values through `watch`. The egui thread sends non-blocking commands and renders snapshots; existing Serial/ZMQ threads remain untouched. Pure helpers own team-side mapping, repository validation, Radar mark presentation, and Serial snapshot-diff logging so behavior is unit-testable without hardware.

**Tech Stack:** Rust 2021, eframe/egui 0.31, Tokio full, `std::process::Command`, existing shared-state readers, Rust unit/integration tests.

## Global Constraints

- Modify only the `radar-egui` repository.
- Do not modify `src/serial/**` or `src/zmq/**`.
- Do not modify Serial/ZMQ protocols, ports, threads, or existing backend tests.
- Do not modify `/home/yukikaze/Documents/workspace/alliance_radar_location_lidar`, `/home/yukikaze/Documents/workspace/laser_guidance`, `alliance_radar_sdr`, or any other external repository.
- Preserve the existing four-workspace layout and visual language.
- Use one global `TeamSide` representing our side; Radar receives our side, SDR/Laser receive the opposite side.
- Start All order is ROS2 Radar, then SDR, then Laser Competition.
- Keep Serial and ZMQ on their existing blocking `std::thread` implementations.
- Do not add `#[tokio::main]` to eframe startup.
- Do not fabricate process health, gRPC connectivity, Serial frame counts, CRC counts, throughput, or raw-frame logs.
- `competition.launch.py` remains the canonical ROS2 launch filename.
- Laser `Auto` affects only Laser and maps its FIFO command to `enemy auto`.
- Ordinary UI text uses the existing proportional font family; status logs use the existing monospace family.

---

## File Structure

### New Files

- `src/services/process_runtime.rs`: Tokio actor, command/state types, Start All sequence, cancellation, retry, and UI handle.

### Modified Files

- `src/services/script_runner.rs`: `TeamSide`, canonical repository resolution, current Laser script names, removal of the obsolete camera override, and process backend implementation.
- `src/services/process_control.rs`: slim non-blocking facade around `ProcessRuntime`; remove frame-polled pending-start logic.
- `src/services/mod.rs`: export the runtime module and shared process types.
- `src/app/mod.rs`: replace duplicate side/camera fields with one `TeamSide`, retain a previous Serial snapshot, poll process snapshots, emit state-update logs, and remove frame-polled Start All.
- `src/app/laser_process_controls.rs`: current vertical controls wired to async commands and global side mapping.
- `src/app/laser_inspector.rs`: remove editable Camera device and show read-only HikCamera ownership.
- `src/app/radar_workspace.rs`: show ROS2 Radar process state separately from point-cloud SHM; make Rerun gRPC wording honest.
- `src/app/serial_workspace.rs`: call snapshot-diff logging and preserve existing open behavior.
- `src/widgets/serial_panel.rs`: five opponent Radar marks, responsive game-progress header, state-log title, and monospace log rendering.
- `src/app/assets.rs`: rename the stale texture identifier only; preserve font setup.
- `README.md`: current architecture and Tokio process actor.
- `AGENTS.md`: current process runtime, repository paths, and script contracts.
- `docs/data-flow.md`: command/watch channels and Start All flow; remove stale process/path descriptions.
- `todo.md`: correct historical paths and mark superseded architecture accurately.

### Explicitly Untouched

- `src/serial/**`
- `src/zmq/**`
- `tests/runtime/serial.rs`
- `tests/runtime/zmq.rs`
- all external repositories

---

### Task 1: Team Side and External Repository Contracts

**Files:**
- Modify: `src/services/script_runner.rs`
- Modify: `src/services/mod.rs`
- Test: `src/services/script_runner.rs` unit tests

**Interfaces:**
- Produces: `pub enum TeamSide { Red, Blue }`
- Produces: `TeamSide::as_str(self) -> &'static str`
- Produces: `TeamSide::enemy(self) -> TeamSide`
- Produces: `TeamSide::laser_enemy_command(self, laser_auto: bool) -> &'static str`
- Produces: `LaserScript::script_name(self) -> &'static str`
- Produces: `resolve_radar_root() -> io::Result<PathBuf>`
- Produces: `resolve_laser_root() -> io::Result<PathBuf>`
- Consumes: environment variables `ALLIANCE_RADAR_LOCATION_LIDAR_ROOT` and `LASER_GUIDANCE_ROOT`

- [ ] **Step 1: Add failing side-mapping and script-name tests**

Add tests inside `src/services/script_runner.rs`:

```rust
#[test]
fn team_side_maps_our_and_enemy_colors() {
    assert_eq!(TeamSide::Red.as_str(), "red");
    assert_eq!(TeamSide::Red.enemy(), TeamSide::Blue);
    assert_eq!(TeamSide::Blue.enemy(), TeamSide::Red);
    assert_eq!(TeamSide::Red.laser_enemy_command(false), "enemy blue");
    assert_eq!(TeamSide::Blue.laser_enemy_command(false), "enemy red");
    assert_eq!(TeamSide::Red.laser_enemy_command(true), "enemy auto");
}

#[test]
fn laser_scripts_match_current_repository_contract() {
    assert_eq!(LaserScript::Competition.script_name(), "competition-laser");
    assert_eq!(LaserScript::Preview.script_name(), "preview-laser");
    assert_eq!(LaserScript::Stream.script_name(), "stream");
    assert_eq!(LaserScript::Record.script_name(), "record");
}
```

- [ ] **Step 2: Run the focused tests and verify failure**

Run:

```bash
cargo test services::script_runner::tests::team_side_maps_our_and_enemy_colors
cargo test services::script_runner::tests::laser_scripts_match_current_repository_contract
```

Expected: FAIL because `TeamSide` does not exist and old script names are returned.

- [ ] **Step 3: Implement `TeamSide` and current Laser script mapping**

Add the public enum near `LaserScript`:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TeamSide {
    #[default]
    Red,
    Blue,
}

impl TeamSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Blue => "blue",
        }
    }

    pub fn enemy(self) -> Self {
        match self {
            Self::Red => Self::Blue,
            Self::Blue => Self::Red,
        }
    }

    pub fn laser_enemy_command(self, laser_auto: bool) -> &'static str {
        if laser_auto {
            "enemy auto"
        } else {
            match self.enemy() {
                Self::Red => "enemy red",
                Self::Blue => "enemy blue",
            }
        }
    }
}
```

Make `LaserScript::script_name` public and map Competition/Preview to `competition-laser`/`preview-laser`.

- [ ] **Step 4: Add failing repository-validation tests using temporary directories**

Extract validators that accept an explicit path, then test without changing process-global environment:

```rust
#[test]
fn valid_laser_root_requires_current_scripts() {
    let temp = temp_test_dir("laser-root");
    std::fs::create_dir_all(temp.join(".script")).unwrap();
    std::fs::write(temp.join(".script/competition-laser"), "").unwrap();
    std::fs::write(temp.join(".script/preview-laser"), "").unwrap();
    std::fs::write(temp.join(".script/stream"), "").unwrap();
    std::fs::write(temp.join(".script/record"), "").unwrap();
    assert!(valid_laser_root(temp.clone()).is_ok());
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn valid_radar_root_requires_workspace_contract() {
    let temp = temp_test_dir("radar-root");
    std::fs::create_dir_all(temp.join("ros_ws/install")).unwrap();
    std::fs::create_dir_all(temp.join("ros_ws/src/radar_bringup/launch")).unwrap();
    std::fs::write(temp.join("ros_ws/install/setup.bash"), "").unwrap();
    std::fs::write(
        temp.join("ros_ws/src/radar_bringup/launch/competition.launch.py"),
        "",
    )
    .unwrap();
    assert!(valid_radar_root(temp.clone()).is_ok());
    std::fs::remove_dir_all(temp).unwrap();
}
```

Implement `temp_test_dir(name)` with `std::env::temp_dir()`, process ID, and a timestamp so tests do not need a new crate.

- [ ] **Step 5: Run the validation tests and verify failure**

Run: `cargo test services::script_runner::tests::valid_`

Expected: FAIL because the Radar validator does not exist and the Laser validator expects `.script/competition`.

- [ ] **Step 6: Implement canonical root resolution and preflight validation**

Use constants:

```rust
const RADAR_ROOT_ENV: &str = "ALLIANCE_RADAR_LOCATION_LIDAR_ROOT";
const LASER_ROOT_ENV: &str = "LASER_GUIDANCE_ROOT";
```

Resolution order must be environment override first, then `Path::new(env!("CARGO_MANIFEST_DIR")).join("../../alliance_radar_location_lidar")` or `join("../../laser_guidance")`. Do not search runtime CWD and do not include the stale `../laser_guidance` candidate.

`valid_radar_root` must require `ros_ws/install/setup.bash` and `ros_ws/src/radar_bringup/launch/competition.launch.py`. `valid_laser_root` must require all four current scripts. Return `io::ErrorKind::NotFound` with the rejected absolute/display path and missing contract.

Update `start_radar` to use `resolve_radar_root`, `bash -lc`, `setup.bash`, and `exec ros2 launch ... side:=...`. Update Laser start to use `resolve_laser_root`, remove the `device` parameter, and remove `.env("LASER_CAMERA_DEVICE", ...)`.

- [ ] **Step 7: Run focused and service tests**

Run: `cargo test services::script_runner`

Expected: PASS.

- [ ] **Step 8: Commit the contract update**

```bash
git add src/services/script_runner.rs src/services/mod.rs
git commit -m "fix: align external process contracts"
```

---

### Task 2: Tokio Process Actor and Start All Coroutine

**Files:**
- Create: `src/services/process_runtime.rs`
- Rewrite: `src/services/process_control.rs`
- Modify: `src/services/mod.rs`
- Test: `src/services/process_runtime.rs` unit tests

**Interfaces:**
- Consumes: `TeamSide`, `LaserScript`, `ScriptRunner`
- Produces: `pub enum ProcessComponent { Radar, Sdr, Laser }`
- Produces: `pub enum ProcessPhase`
- Produces: `pub enum ProcessCommand`
- Produces: `pub struct ComponentSnapshot { pub managed: bool, pub active_laser: Option<LaserScript> }`
- Produces: `pub struct ProcessSnapshot`
- Produces: `pub struct ProcessRuntime { command_tx, snapshot_rx, thread }`
- Produces: `ProcessRuntime::start() -> Self`
- Produces: `ProcessRuntime::send(&self, command: ProcessCommand) -> Result<(), ProcessSendError>`
- Produces: `ProcessRuntime::snapshot(&self) -> ProcessSnapshot`
- Produces: slim `ProcessControl` methods used by egui

- [ ] **Step 1: Define a testable synchronous backend trait and write actor-order tests**

In `process_runtime.rs`, define:

```rust
pub(crate) trait ProcessBackend: Send + 'static {
    fn start_radar(&mut self, side: TeamSide) -> io::Result<()>;
    fn start_sdr(&mut self, enemy: TeamSide) -> io::Result<()>;
    fn start_laser(&mut self, script: LaserScript) -> io::Result<()>;
    fn configure_laser(&mut self, enemy: &str, stream: bool, record: bool) -> io::Result<()>;
    fn stop_radar(&mut self);
    fn stop_sdr(&mut self);
    fn stop_laser(&mut self);
}
```

In the `#[cfg(test)]` module of `process_runtime.rs`, create `FakeBackend` backed by `Arc<Mutex<Vec<String>>>` and test with constructor-injected millisecond delays:

```rust
#[tokio::test]
async fn start_all_runs_radar_then_sdr_then_laser() {
    let delay = Duration::from_millis(1);
    let (runtime, events) = test_runtime(FakeBackend::default(), delay);
    runtime.send(ProcessCommand::StartAll {
        side: TeamSide::Red,
        stream: true,
        record: false,
        laser_auto: false,
    }).unwrap();

    wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;

    assert_eq!(events.lock().unwrap().as_slice(), [
        "radar:red",
        "sdr:blue",
        "laser:Competition",
        "fifo:enemy blue,stream on,record off",
    ]);
}
```

Implement `wait_for_phase` with `tokio::time::timeout` and short `tokio::task::yield_now()` retries. The test constructor accepts a backend and delay so production delays stay unchanged and tests require no Tokio `test-util` feature.

- [ ] **Step 2: Run the actor test and verify failure**

Run: `cargo test start_all_runs_radar_then_sdr_then_laser`

Expected: FAIL because `process_runtime` and its command/state types do not exist.

- [ ] **Step 3: Implement the minimal actor loop**

Use one Tokio runtime on one OS thread:

```rust
let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
let (snapshot_tx, snapshot_rx) = tokio::sync::watch::channel(ProcessSnapshot::default());
let thread = std::thread::spawn(move || {
    tokio::runtime::Runtime::new()
        .expect("process runtime")
        .block_on(run_process_actor(backend, command_rx, snapshot_tx));
});
```

Represent an active Start All as actor-owned sequence state, not a detached task sharing the backend:

```rust
enum SequenceStep {
    StartRadar,
    WaitAfterRadar,
    StartSdr,
    WaitAfterSdr,
    StartLaser,
    ConfigureLaser,
}
```

The actor loop uses `tokio::select!` between `command_rx.recv()` and the next `Sleep` deadline. Synchronous spawn/config methods run only in the actor. Publish a new `ProcessSnapshot` after every state transition.

- [ ] **Step 4: Add failure, retry, and cancellation tests**

Add tests that configure `FakeBackend` to fail once at SDR:

```rust
#[tokio::test]
async fn failure_stops_later_steps_and_retry_continues() {
    let delay = Duration::from_millis(1);
    let backend = FakeBackend::fail_once(ProcessComponent::Sdr);
    let (runtime, events) = test_runtime(backend, delay);
    runtime.send(start_all_red()).unwrap();
    wait_for_failed(&runtime, ProcessComponent::Sdr, Duration::from_secs(1)).await;
    assert_eq!(events.lock().unwrap().as_slice(), ["radar:red", "sdr:blue"]);

    runtime.send(ProcessCommand::RetryFailed).unwrap();
    wait_for_phase(&runtime, ProcessPhase::Running, Duration::from_secs(1)).await;
    assert!(events.lock().unwrap().ends_with(&[
        "sdr:blue".into(),
        "laser:Competition".into(),
        "fifo:enemy blue,stream on,record off".into(),
    ]));
}

#[tokio::test]
async fn stop_all_during_delay_cancels_remaining_steps() {
    let delay = Duration::from_secs(30);
    let (runtime, events) = test_runtime(FakeBackend::default(), delay);
    runtime.send(start_all_red()).unwrap();
    wait_for_phase(&runtime, ProcessPhase::WaitingForRadar, Duration::from_secs(1)).await;
    runtime.send(ProcessCommand::StopAll).unwrap();
    wait_for_phase(&runtime, ProcessPhase::Idle, Duration::from_secs(1)).await;
    assert!(!events.lock().unwrap().iter().any(|e| e.starts_with("sdr:")));
    assert_eq!(runtime.snapshot().phase, ProcessPhase::Idle);
}
```

- [ ] **Step 5: Run tests and verify failure**

Run: `cargo test process_runtime`

Expected: FAIL until failed-step retention, retry continuation, and delay cancellation are implemented.

- [ ] **Step 6: Implement failure retention, Retry Failed, Stop All, and Shutdown**

Store the failed sequence context in the actor:

```rust
struct FailedSequence {
    command: StartAllOptions,
    step: SequenceStep,
}
```

`RetryFailed` reinstates the failed step. `StopAll` clears sequence/failure state, publishes `Stopping`, invokes `stop_laser`, `stop_sdr`, then `stop_radar`, and publishes `Idle`. `Shutdown` performs the same cleanup and breaks the loop.

Implement `Drop` for `ProcessRuntime`: send `Shutdown`, drop/take sender as needed, and join only the process runtime thread. Avoid blocking from inside that same thread.

- [ ] **Step 7: Implement `ScriptRunner` as the production backend**

Map:

```rust
fn start_radar(&mut self, side: TeamSide) -> io::Result<()> {
    ScriptRunner::start_radar(self, side.as_str())
}

fn start_sdr(&mut self, enemy: TeamSide) -> io::Result<()> {
    ScriptRunner::start_sdr(self, enemy.as_str())
}
```

Laser configuration retries `send_fifo` asynchronously in actor time without `std::thread::sleep`: schedule up to 100 attempts at 50 ms intervals and fail with the final `io::Error`. Preserve the existing command order: enemy, stream, record.

- [ ] **Step 8: Rewrite `ProcessControl` as a non-blocking facade**

`ProcessControl` should contain only `ProcessRuntime` and methods such as:

```rust
pub fn snapshot(&self) -> ProcessSnapshot;
pub fn start_all(&self, options: StartAllOptions) -> Result<(), ProcessSendError>;
pub fn retry_failed(&self) -> Result<(), ProcessSendError>;
pub fn start_radar(&self, side: TeamSide) -> Result<(), ProcessSendError>;
pub fn start_sdr(&self, side: TeamSide) -> Result<(), ProcessSendError>;
pub fn start_laser(&self, options: StartLaserOptions) -> Result<(), ProcessSendError>;
pub fn stop_all(&self) -> Result<(), ProcessSendError>;
```

Remove `PendingStartAll`, `trigger_pending_start_all`, `std::thread::sleep` orchestration, and direct mutable `ScriptRunner` ownership.

- [ ] **Step 9: Run actor and service tests**

Run: `cargo test process_runtime && cargo test services`

Expected: PASS.

- [ ] **Step 10: Commit the actor**

```bash
git add src/services/process_runtime.rs src/services/process_control.rs src/services/mod.rs
git commit -m "feat: orchestrate processes with tokio actor"
```

---

### Task 3: Application State and Global Team-Side Wiring

**Files:**
- Modify: `src/app/mod.rs`
- Modify: `src/app/laser_process_controls.rs`
- Test: `src/app/mod.rs` unit tests

**Interfaces:**
- Consumes: `TeamSide`, `ProcessControl`, `ProcessSnapshot`, `StartAllOptions`, `StartLaserOptions`
- Produces: one `team_side: TeamSide` field and one `laser_auto: bool` field in `RadarApp`
- Removes: `camera_device`, `enemy_color`, `radar_side`

- [ ] **Step 1: Add failing UI-state mapping tests**

Add pure helper tests without constructing eframe:

```rust
#[test]
fn red_team_maps_process_parameters_consistently() {
    let side = TeamSide::Red;
    assert_eq!(side.as_str(), "red");
    assert_eq!(side.enemy().as_str(), "blue");
    assert_eq!(side.laser_enemy_command(false), "enemy blue");
}
```

This may reuse Task 1 coverage; add instead a test for `RadarApp` option construction if no new behavior remains:

```rust
#[test]
fn start_all_options_preserve_laser_flags() {
    let options = start_all_options(TeamSide::Blue, true, false, true);
    assert_eq!(options.side, TeamSide::Blue);
    assert!(options.stream);
    assert!(!options.record);
    assert!(options.laser_auto);
}
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `cargo test app::tests::start_all_options_preserve_laser_flags`

Expected: FAIL because the helper/options wiring does not exist.

- [ ] **Step 3: Replace duplicate fields and remove frame polling**

In `RadarApp`:

```rust
process_control: ProcessControl,
team_side: TeamSide,
laser_auto: bool,
stream_on_start: bool,
record_on_start: bool,
```

Initialize `team_side` to Red and update existing `SharedData.radar_side` whenever the user changes side. Remove `camera_device`, `enemy_color`, and `radar_side`. Remove `self.process_control.trigger_pending_start_all()` from `eframe::App::update`.

Add `fn process_snapshot(&self) -> ProcessSnapshot` only if it reduces repeated borrow complexity; do not cache a second mutable copy.

- [ ] **Step 4: Rewire the existing vertical process controls**

Keep the current card order. Build commands from the global fields:

```rust
let options = StartAllOptions {
    side: self.team_side,
    stream: self.stream_on_start,
    record: self.record_on_start,
    laser_auto: self.laser_auto,
};
self.process_control.start_all(options)
```

The team-side UI label is `我方阵营`; show Red/Blue only. Display read-only derived text:

```text
ROS2 Radar: side=red · Laser/SDR: enemy=blue
```

Keep Competition/Preview/Stream/Record and Stop Laser. Add Laser-only Auto as a checkbox/toggle near Laser mode controls. Add per-component Start/Stop, Start All, Retry Failed, Stop All, current phase, and last error. Button clicks only enqueue commands; they do not spawn threads or sleep.

- [ ] **Step 5: Surface command-channel errors in the UI**

Add a small helper that stores `ProcessSendError` text in existing UI error state or a dedicated `process_command_error: Option<String>`. Render it in the process card and clear it after the next successful command. Do not use log-only failure handling.

- [ ] **Step 6: Run app/service tests and compile**

Run:

```bash
cargo test app::tests
cargo test services
cargo check
```

Expected: PASS; no references to `trigger_pending_start_all`, `EnemyColor`, `camera_device`, or `radar_side` app fields remain.

- [ ] **Step 7: Commit application wiring**

```bash
git add src/app/mod.rs src/app/laser_process_controls.rs
git commit -m "feat: wire global team process controls"
```

---

### Task 4: Laser and ROS2 Radar Workspace Accuracy

**Files:**
- Modify: `src/app/laser_inspector.rs`
- Modify: `src/app/radar_workspace.rs`
- Modify: `src/app/assets.rs`
- Test: existing app tests plus `cargo check`

**Interfaces:**
- Consumes: `ProcessSnapshot` from `ProcessControl::snapshot()`
- Produces: read-only HikCamera ownership display
- Produces: honest separation of ROS2 Radar process, location transport, point-cloud SHM, and optional Rerun

- [ ] **Step 1: Add a failing pure Radar status-label test**

Extract a helper:

```rust
fn rerun_status_label() -> &'static str {
    "optional · not monitored"
}

#[test]
fn rerun_status_does_not_claim_connection() {
    assert_eq!(rerun_status_label(), "optional · not monitored");
}
```

Run: `cargo test rerun_status_does_not_claim_connection`

Expected: FAIL because the helper does not exist and current UI derives gRPC status from SHM.

- [ ] **Step 2: Remove editable Camera UI**

Replace the `TextEdit` in `laser_inspector.rs` with read-only rows:

```text
Camera backend  HikCamera
Configuration   managed by laser_guidance
Selection       auto when one device is present
```

Keep Laser ZMQ and observation status exactly as observable today; add video SHM status only from `video_feed`/texture availability already present in app state.

- [ ] **Step 3: Separate Radar process and point-cloud status**

Rename the workspace heading to `ROS2 Radar Workspace`. In the right inspector add an `ROS2 Radar` card showing actor-managed process state and canonical launch text. Keep `/pointcloud_frame` in its own card.

Replace the status-strip `gRPC: streaming/idle` cell with:

```text
Rerun  optional
```

Use `rerun_status_label()` in the sidebar. Do not infer gRPC state from `has_data`; only SHM and point counts use `has_data`.

- [ ] **Step 4: Rename the stale texture identifier**

Change only egui's internal texture key from `unity_minimap_bg` to `sdr_minimap_bg`. Do not alter the image or rendering behavior.

- [ ] **Step 5: Run focused tests and compile**

Run: `cargo test rerun_status_does_not_claim_connection && cargo check`

Expected: PASS.

- [ ] **Step 6: Commit workspace accuracy changes**

```bash
git add src/app/laser_inspector.rs src/app/radar_workspace.rs src/app/assets.rs
git commit -m "fix: show accurate radar and laser status"
```

---

### Task 5: Serial UI Markers, Responsive Progress, and State-Update Log

**Files:**
- Modify: `src/widgets/serial_panel.rs`
- Modify: `src/app/serial_workspace.rs`
- Modify: `src/app/mod.rs`
- Test: `src/widgets/serial_panel.rs` unit tests
- Test: `src/app/serial_workspace.rs` unit tests

**Interfaces:**
- Consumes: existing `SharedData`, `RadarMarkProcessData`, and serial-open state
- Produces: `opponent_mark_rows(mark: &RadarMarkProcessData) -> [MarkRow; 5]`
- Produces: `SerialObservedState::from_shared(data: &SharedData) -> Self`
- Produces: `diff_serial_state(previous, current) -> Vec<SerialLogEvent>`
- Produces: `RadarApp::update_serial_state_log(&SharedData)`
- Removes: cosmetic `serial_parse_enable` controls that no backend reads

- [ ] **Step 1: Add failing five-marker mapping test**

Define a private presentation type:

```rust
#[derive(Debug, PartialEq, Eq)]
struct MarkRow {
    label: &'static str,
    vulnerable: bool,
}
```

Test:

```rust
#[test]
fn opponent_mark_rows_show_required_five_units() {
    let mark = RadarMarkProcessData {
        opponent_hero_vulnerable: 1,
        opponent_engineer_vulnerable: 0,
        opponent_infantry_3_vulnerable: 1,
        opponent_infantry_4_vulnerable: 0,
        opponent_sentry_vulnerable: 1,
        ..Default::default()
    };
    let rows = opponent_mark_rows(&mark);
    assert_eq!(rows.map(|row| row.label), [
        "Hero", "Engineer", "Infantry 3", "Infantry 4", "Sentry",
    ]);
    assert_eq!(rows.map(|row| row.vulnerable), [true, false, true, false, true]);
}
```

- [ ] **Step 2: Run the marker test and verify failure**

Run: `cargo test opponent_mark_rows_show_required_five_units`

Expected: FAIL because the current grid contains Enemy 1-6 and ally rows.

- [ ] **Step 3: Implement the five-unit Radar mark grid**

Use one responsive row of five cells when width permits and a wrapped `egui::Grid`/two-row arrangement at narrow widths. Remove Aerial and all ally cells from this view. The legend must say `红=易伤` and must not mention blue marked state if that state is no longer rendered.

- [ ] **Step 4: Add failing Serial snapshot-diff tests**

Define a deliberately small observable snapshot containing displayed primitive fields because the backend data types do not derive `Eq`:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SerialObservedState {
    game: (u8, u8, u16, u64),
    site: (u8, u8, u8, u8, u8, u16, u8, u8, u8, u8, u8),
    radar: [u8; 5],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SerialLogEvent {
    kind: SerialLogKind,
    text: String,
}

impl SerialLogEvent {
    fn rx(text: impl Into<String>) -> Self {
        Self { kind: SerialLogKind::Rx, text: text.into() }
    }
}
```

`from_shared` fills `game` from GameState, `site` from every SiteEvent field in display order, and `radar` from Hero, Engineer, Infantry 3, Infantry 4, and Sentry vulnerable flags.

Tests:

```rust
#[test]
fn serial_diff_logs_only_changed_observable_groups() {
    let before = SerialObservedState::default();
    let mut after = before.clone();
    after.game.2 = 419;
    after.radar[0] = 1;
    assert_eq!(
        diff_serial_state(&before, &after),
        vec![
            SerialLogEvent::rx("0x0001 GameState · remain=419s"),
            SerialLogEvent::rx("0x020C RadarMarkProcess · Hero=vulnerable"),
        ]
    );
}

#[test]
fn identical_serial_snapshots_do_not_create_fake_frames() {
    let state = SerialObservedState::default();
    assert!(diff_serial_state(&state, &state).is_empty());
}
```

- [ ] **Step 5: Run snapshot-diff tests and verify failure**

Run:

```bash
cargo test serial_diff_logs_only_changed_observable_groups
cargo test identical_serial_snapshots_do_not_create_fake_frames
```

Expected: FAIL because observation/diff helpers do not exist.

- [ ] **Step 6: Implement state-update logging**

Add `serial_last_observed: Option<SerialObservedState>` to `RadarApp`. In `eframe::App::update`, after taking `SharedData` snapshot and before rendering, call `update_serial_state_log` only when `serial_open` is true. On first observation, set the baseline without emitting synthetic RX entries. On later changes, append timestamped `SerialLogKind::Rx` entries through the existing bounded deque.

Rename visible card title/subtitle from `帧日志 / 最近帧` to `状态更新日志 / SharedData 可观察变化`. Keep open success/failure entries.

Remove the `serial_parse_enable` field and the “解析开关” card because the existing Serial backend exposes no parse-enable interface. Do not replace it with another control; the parser continues its existing behavior unchanged.

- [ ] **Step 7: Make log text use the existing monospace family**

Render each log line and empty-state message with:

```rust
RichText::new(text)
    .family(egui::FontFamily::Monospace)
    .color(color)
    .size(11.0)
```

Do not change global font loading paths or add a font asset.

- [ ] **Step 8: Fix responsive game-progress layout**

Replace the nested `right_to_left` layout with either `egui::Grid::new(...).num_columns(2)` using a bounded second cell or a width branch:

```rust
if ui.available_width() >= 180.0 {
    ui.horizontal(|ui| {
        ui.label("比赛进度");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(format!("{elapsed} / {total} s"));
        });
    });
} else {
    ui.label("比赛进度");
    ui.label(format!("{elapsed} / {total} s"));
}
```

Constrain the layout to the current card's available width; do not allocate against a parent/right inspector rect.

- [ ] **Step 9: Run Serial UI tests and compile**

Run:

```bash
cargo test opponent_mark_rows
cargo test serial_diff
cargo test identical_serial
cargo check
```

Expected: PASS.

- [ ] **Step 10: Verify backend files remain untouched**

Run: `git diff --exit-code HEAD -- src/serial src/zmq tests/runtime/serial.rs tests/runtime/zmq.rs`

Expected: exit 0 with no output.

- [ ] **Step 11: Commit Serial UI changes**

```bash
git add src/widgets/serial_panel.rs src/app/serial_workspace.rs src/app/mod.rs
git commit -m "fix: connect serial status UI"
```

---

### Task 6: Architecture and Naming Documentation

**Files:**
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/data-flow.md`
- Modify: `todo.md`

**Interfaces:**
- Consumes: final runtime/type/script names from Tasks 1-5
- Produces: documentation matching shipped behavior

- [ ] **Step 1: Inventory stale active documentation**

Run:

```bash
rg -n "Unity|RADAR_APP|\.\./alliance_radar_location_lidar|\.script/competition\b|\.script/preview\b|trigger_pending_start_all|Start All \(SDR|gRPC.*streaming|ZMQ_PUB_GAME_STATE|ZMQ_PUB_RADAR_MARK" README.md AGENTS.md docs todo.md
```

Expected: matches identify stale paths, scripts, architecture, or historical references requiring correction. Keep only explicit negative guardrails such as “do not reintroduce Unity” where useful.

- [ ] **Step 2: Update README architecture**

Document:

```text
egui → mpsc ProcessCommand → Tokio ProcessRuntime actor → ScriptRunner
                                      │
                                      └→ watch ProcessSnapshot → egui
```

State that Start All uses a coroutine and `tokio::select!`, starts Radar → SDR → Laser, and remains cancellable during delays. Include global side mapping, environment override names, canonical repository locations, current Laser scripts, and HikCamera ownership.

State that Rerun gRPC is optional visualization only; ROS2 Radar, Laser, SDR, and Serial use their existing ZMQ/SHM/UART paths.

- [ ] **Step 3: Update AGENTS and data-flow**

In `AGENTS.md`, update the runtime model and process-control module responsibility. In `docs/data-flow.md`, add command/watch channels and remove the frame-polled `PendingStartAll` path. Correct the Radar and Laser repository contracts and script names.

Do not change Serial/ZMQ protocol values. When current docs conflict with implementation, describe current implementation without inventing new protocol IDs.

- [ ] **Step 4: Update historical todo wording**

Correct active claims that `ScriptRunner` launches wrong relative paths or old scripts. Clearly mark obsolete TCP/Unity notes as historical/superseded rather than active architecture.

- [ ] **Step 5: Verify stale terminology is gone or intentionally guarded**

Run the same `rg` command from Step 1.

Expected: no active architecture claims use wrong paths/scripts or fake gRPC state. Any remaining Unity/RADAR_APP occurrence is an explicit prohibition or historical note.

- [ ] **Step 6: Commit documentation**

```bash
git add README.md AGENTS.md docs/data-flow.md todo.md
git commit -m "docs: describe tokio process orchestration"
```

---

### Task 7: Full Verification and Guardrail Audit

**Files:**
- Modify only files required to fix verification failures, excluding all globally forbidden paths
- Test: full repository

**Interfaces:**
- Consumes: all previous tasks
- Produces: verified implementation ready for review

- [ ] **Step 1: Format and verify formatting**

Run: `cargo fmt --all && cargo fmt --all --check`

Expected: PASS.

- [ ] **Step 2: Run the complete test suite**

Run: `cargo test`

Expected: all unit and integration tests PASS.

- [ ] **Step 3: Run Clippy with warnings denied**

Run: `cargo clippy --all-targets -- -D warnings`

Expected: PASS with no warnings.

- [ ] **Step 4: Verify forbidden backend paths have no implementation diff**

Compare against the design commit that precedes implementation:

```bash
git diff --exit-code f1efa99 -- src/serial src/zmq tests/runtime/serial.rs tests/runtime/zmq.rs
```

Expected: exit 0 with no output.

- [ ] **Step 5: Verify no external repository was modified by this work**

Run read-only status checks:

```bash
git -C /home/yukikaze/Documents/workspace/alliance_radar_location_lidar status --short
git -C /home/yukikaze/Documents/workspace/laser_guidance status --short
```

Record the output for the final report. Do not clean or alter pre-existing external changes; the implementation must not add any new changes there.

- [ ] **Step 6: Inspect final repository diff**

Run:

```bash
git status --short --branch
git diff --stat f1efa99..HEAD
git diff --check f1efa99..HEAD
```

Expected: only planned `radar-egui` files changed, no whitespace errors, and unrelated untracked `.superpowers/` or other user files remain uncommitted.

- [ ] **Step 7: Perform a manual UI smoke test where display access is available**

Run: `RUST_LOG=info cargo run --release`

Verify:

- Laser panel remains vertical and has one “our side” selector.
- Camera input is gone and HikCamera ownership is read-only.
- Start All displays Radar → SDR → Laser and does not freeze egui.
- Stop All remains available during startup.
- Radar page separates ROS2 Radar, SHM, and optional Rerun.
- Serial Radar marks show exactly Hero, Engineer, Infantry 3, Infantry 4, Sentry.
- Serial status log is monospace and does not claim raw frames.
- `0 / 420 s` remains inside its card at the minimum supported window width.

If hardware or display access is unavailable, report the smoke test as not run; do not claim it passed.

- [ ] **Step 8: Commit verification fixes only when verification changed tracked files**

Inspect `git status --short` and stage each verification-fix path explicitly; never use `git add .`. Commit with `git commit -m "fix: resolve integration verification issues"`. Skip this step when verification required no code or documentation fixes; do not create an empty commit.
