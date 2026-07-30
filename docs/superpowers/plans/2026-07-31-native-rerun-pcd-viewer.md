# Native Rerun PCD Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users select an ASCII or binary PCD file from the Radar workspace and display all valid points in a separately launched native Rerun Viewer.

**Architecture:** A pure PCD loader converts a dynamic PCD schema into positions and colors. An isolated `PcdViewerRuntime` runs loading and Rerun launch/logging on its own worker thread and exposes status snapshots to the UI; Radar UI only starts and polls this runtime. It has no dependency on `ProcessControl`, `ScriptRunner`, or competition runtime handles.

**Tech Stack:** Rust 2021, egui/eframe 0.31, `pcd-rs 0.13`, `rfd`, optional `rerun 0.33`

## Global Constraints

- Support PCD 0.7 `DATA ascii` and `DATA binary`; reject `binary_compressed` explicitly.
- Preserve every valid point without sampling, including multi-million-point files.
- Parse and publish outside the egui UI thread, with only one active load.
- Require `x/y/z`; support packed `rgb`/`rgba`, separate channels, intensity, then height fallback.
- PCD viewing must not use or alter `ProcessControl`, `ScriptRunner`, SDR, Laser, ROS2 Radar, ZMQ, serial, video, or SHM lifecycle state.
- Keep all Rerun launch/log code behind the existing `rerun` feature.
- Do not create git commits unless the user explicitly requests them.

---

## File Structure

- Create `src/pointcloud/pcd_loader.rs`: PCD schema adaptation, parsing, color fallback, errors, parser tests.
- Create `src/pointcloud/pcd_viewer.rs`: isolated worker state machine and feature-gated native Rerun launcher.
- Modify `src/pointcloud/mod.rs`: export the two focused modules.
- Modify `src/app/mod.rs`: own only a `PcdViewerRuntime` field and poll it each frame.
- Modify `src/app/radar_workspace.rs`: file chooser button and dedicated status card.
- Modify `Cargo.toml`: add `pcd-rs`, `rfd`, and test fixture support.

### Task 1: Dynamic PCD Loader

**Files:**
- Create: `src/pointcloud/pcd_loader.rs`
- Modify: `src/pointcloud/mod.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `pub struct LoadedPcd { pub positions: Vec<[f32; 3]>, pub colors: Vec<[u8; 4]>, pub skipped_points: u64, pub declared_points: u64, pub encoding: PcdEncoding }`
- Produces: `pub enum PcdEncoding { Ascii, Binary }`
- Produces: `pub fn load_pcd(path: &Path, progress: impl FnMut(u64, u64)) -> Result<LoadedPcd, PcdLoadError>`

- [ ] Add `pcd-rs = "0.13"`, `rfd = "0.17"`, and `tempfile = "3"` under dev dependencies.
- [ ] Write parser tests using generated temporary PCD fixtures for ASCII XYZ, binary XYZ, packed RGB/RGBA, separate channels, intensity fallback, height fallback, invalid coordinates, missing XYZ, truncated binary, and compressed rejection.
- [ ] Run `cargo test pointcloud::pcd_loader` and confirm the tests fail before the module exists.
- [ ] Implement schema lookup that excludes padding fields, converts scalar integer/float fields to `f32`, decodes PCL packed colors by bit pattern, and validates required fields.
- [ ] Implement one-pass buffered record iteration with preallocated output vectors, progress callbacks, finite-coordinate filtering, and deferred fallback coloring.
- [ ] Run `cargo test pointcloud::pcd_loader` and confirm all loader tests pass.

### Task 2: Isolated Viewer Runtime

**Files:**
- Create: `src/pointcloud/pcd_viewer.rs`
- Modify: `src/pointcloud/mod.rs`

**Interfaces:**
- Consumes: `load_pcd(&Path, progress)` and `LoadedPcd` from Task 1.
- Produces: `pub enum PcdViewerStatus { Idle, Loading { ... }, Launching { ... }, Ready { ... }, Failed { ... } }`
- Produces: `pub struct PcdViewerRuntime` with `new()`, `start(PathBuf) -> bool`, `poll()`, `status()`, and `is_busy()`.

- [ ] Write state-machine tests using an injected loader/launcher boundary to prove ordered transitions, single-active-load rejection, worker error recovery, and launcher failure containment.
- [ ] Run `cargo test pointcloud::pcd_viewer` and confirm failure before implementation.
- [ ] Implement a dedicated mpsc event channel and one worker thread per accepted load; catch worker panics and report `Failed` without joining from the UI thread.
- [ ] Under `feature = "rerun"`, launch with `RecordingStreamBuilder::new("radar-pcd-viewer").spawn()`, log `world/pointcloud`, axes, and ground grid, then flush.
- [ ] Under no `rerun` feature, return a deterministic feature-disabled error without touching any process-management module.
- [ ] Run `cargo test pointcloud::pcd_viewer` and confirm all runtime tests pass.

### Task 3: Radar Workspace Integration

**Files:**
- Modify: `src/app/mod.rs`
- Modify: `src/app/radar_workspace.rs`

**Interfaces:**
- Consumes: `PcdViewerRuntime` and `PcdViewerStatus` from Task 2.
- Does not consume: `ProcessControl`, `ScriptRunner`, or any competition process handle.

- [ ] Add `pcd_viewer: PcdViewerRuntime` to `RadarApp` and initialize it independently of `process_control`.
- [ ] Poll only the PCD runtime from `eframe::App::update` so background progress repaints without blocking.
- [ ] Add `Open PCD in Rerun`, using `rfd::FileDialog` filtered to `pcd`, disabled while loading or launching.
- [ ] Render Idle, Loading, Launching, Ready, and Failed details in a dedicated offline PCD card; leave existing SHM status separate.
- [ ] Build without Rerun and verify the Radar page explicitly reports the feature requirement.
- [ ] Build with Rerun and verify the button compiles and no existing process-control method changed.

### Task 4: Full Verification and Isolation Review

**Files:**
- Review: `src/pointcloud/pcd_loader.rs`
- Review: `src/pointcloud/pcd_viewer.rs`
- Review: `src/app/mod.rs`
- Review: `src/app/radar_workspace.rs`

- [ ] Run `cargo fmt --check`; if it fails, run `cargo fmt` and rerun the check.
- [ ] Run `cargo test` and require success.
- [ ] Run `cargo test --features rerun` and require success.
- [ ] Run `cargo clippy -- -D warnings` and require success.
- [ ] Run `cargo clippy --features rerun -- -D warnings` and require success.
- [ ] Search `pcd_loader.rs` and `pcd_viewer.rs` for `ProcessControl`, `ScriptRunner`, process stop flags, and external runtime handles; require no matches.
- [ ] Inspect the final diff and confirm `Start All`, `Stop All`, SDR, Laser, and ROS2 Radar behavior is unchanged.
