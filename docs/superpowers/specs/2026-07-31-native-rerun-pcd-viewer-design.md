# Native Rerun PCD Viewer Design

## Goal

Add an offline PCD viewing workflow to the Radar workspace without embedding the Rerun UI in radar-egui. The user selects a `.pcd` file, radar-egui parses it, converts it to Rerun `Points3D`, and launches the native Rerun Viewer found on `PATH`.

The feature must not affect SDR, Laser, ROS2 Radar, ZMQ, serial, video, point-cloud SHM, or existing process-management behavior.

## Scope

The first version will:

- Add an `Open PCD in Rerun` action to the Radar workspace.
- Open a native file chooser filtered to `.pcd` files.
- Support PCD 0.7 ASCII and binary data.
- Require `x`, `y`, and `z` fields.
- Adapt common optional fields: `rgb`, `rgba`, separate color channels, normals, and intensity.
- Ignore unknown fields.
- Send every valid point to Rerun without sampling.
- Load and parse the file outside the egui UI thread.
- Launch the native Rerun Viewer through the Rerun SDK, using a `rerun` executable found on `PATH`.
- Keep the existing external Rerun integration behind the `rerun` Cargo feature.

The first version will not:

- Embed the Rerun Viewer in radar-egui.
- Support PCD `binary_compressed` data.
- Edit or export PCD files.
- Add point filtering, cropping, or downsampling.
- Cancel an in-progress load.
- Replace the existing `/pointcloud_frame` SHM reader.

## Architecture

Introduce a PCD-specific runtime that is separate from process control and the existing SHM point-cloud runtime.

```text
Radar workspace: Open PCD in Rerun
                 |
                 v
              file dialog
                 |
                 v
       PcdViewerRuntime worker thread
                 |
                 +-- read PCD header and dynamic schema
                 +-- parse ASCII or binary records
                 +-- adapt positions and colors
                 +-- skip invalid coordinates
                 |
                 v
       Rerun RecordingStream::spawn()
                 |
                 v
          native Rerun Viewer
                 |
                 +-- world/pointcloud
                 +-- world/axes
                 +-- world/ground_grid
```

`PcdViewerRuntime` owns only the state and worker associated with one PCD load. It communicates completion and progress to the UI through a dedicated shared state or channel. It must not hold references to `ProcessControl`, `ScriptRunner`, or external process handles.

## Process Isolation

PCD viewing is not part of radar-egui's competition process orchestration.

- `Start SDR`, `Start Radar`, `Start Laser`, `Start All`, and `Stop All` retain their current behavior.
- Opening or closing Rerun must not start, stop, restart, or signal SDR, Laser, or ROS2 Radar.
- PCD viewing must not use `ProcessControl` or `ScriptRunner`.
- PCD viewing must not alter ZMQ, serial, video, or SHM runtime stop flags.
- `Stop All` must not be repurposed to close the Rerun Viewer.
- A missing `rerun` executable, parser error, worker panic, or closed Viewer must only transition the PCD viewer status to `Failed` or disconnected.
- radar-egui shutdown drops its Rerun recording stream normally, but no general process-management cleanup path owns or terminates the Viewer.

This separation ensures that offline visualization cannot interfere with competition processes.

## PCD Parsing

Use a dynamic-schema PCD reader so files with common field combinations do not require a fixed Rust point struct. Parsing is iterator-based and performed in a worker thread.

### Supported Encodings

- `DATA ascii`
- `DATA binary`

`DATA binary_compressed` is rejected with a specific unsupported-format error.

### Position Fields

`x`, `y`, and `z` are mandatory. Supported numeric representations are converted to `f32`. A point is skipped if any converted coordinate is NaN or infinite. The final status reports skipped points.

### Color Fields

Color selection uses this priority:

1. Packed `rgba`
2. Packed `rgb`
3. Separate `r`, `g`, `b`, and optional `a`
4. `intensity` mapped to a scalar color ramp
5. Height-based color generated from `z`

Packed PCL `rgb` and `rgba` fields stored as `f32` bit patterns are supported. Missing alpha defaults to 255.

Height coloring requires the final finite Z range. The worker retains positions and defers generated colors until parsing completes; it does not reread the full PCD file solely for color generation.

### Other Fields

`normal_x`, `normal_y`, and `normal_z` are recognized but are not sent in the first version because Rerun `Points3D` does not require normals. Unknown fields are ignored.

### Large Files

The viewer must preserve all valid points, including files containing several million points. To control memory pressure:

- Parse directly from a buffered file reader rather than reading the whole file into a byte buffer.
- Reserve position and color vectors from the PCD point count when available.
- Avoid constructing an intermediate vector of dynamic PCD records.
- Move final vectors into Rerun archetype construction where the API permits.
- Allow only one active PCD load at a time.

Several million points can still produce a peak memory footprint of hundreds of megabytes across parser buffers, Rerun serialization, the Viewer store, and GPU resources. The UI reports the file point count and loading phase rather than silently appearing frozen.

## Rerun Session

The runtime creates a recording named `radar-pcd-viewer` and starts the native Viewer with the Rerun SDK's spawn mode. Rerun is resolved from `PATH`; radar-egui does not bundle a Viewer executable in the first version.

The recording contains only offline point-cloud visualization data:

- `world/pointcloud`: one `Points3D` archetype containing every valid point.
- `world/axes`: coordinate reference arrows.
- `world/ground_grid`: ground reference lines.

It does not include SDR robot positions, blood values, economy data, or competition runtime state.

Selecting another file after completion starts a new load and publishes a new recording. Rerun's spawn behavior can reuse an already-running compatible Viewer process, but each selected file has an independent recording identity.

## UI States

The Radar workspace exposes an `Open PCD in Rerun` button and a dedicated status area.

- `Idle`: no PCD operation has started.
- `Loading`: show file name, parsed point count, and total point count when known.
- `Launching`: parsing is complete and radar-egui is starting Rerun and publishing the point cloud.
- `Ready`: show file name, encoding, valid points, skipped points, and elapsed time.
- `Failed`: show a concise actionable error and allow another file selection.

The open button is disabled while `Loading` or `Launching` to prevent overlapping high-memory jobs. Cancelling the file chooser leaves the previous state unchanged.

Existing SHM status remains visible as a separate concern. PCD status labels must not describe an offline file as a SHM frame or gRPC competition stream.

When built without the `rerun` feature, the UI explains that native PCD viewing requires `--features rerun`; it must not expose a button that fails silently.

## Error Handling

Errors are contained within the PCD viewer state.

- Missing `rerun` executable: instruct the user to install a Rerun CLI version compatible with the Rust SDK.
- Unsupported `binary_compressed`: identify the unsupported encoding.
- Missing `x`, `y`, or `z`: list required and discovered fields.
- Unsupported field representation: identify the field and schema type.
- Truncated file or malformed record: include the point index when available.
- NaN or infinite positions: skip and count the point.
- Rerun launch or logging failure: retain parsed file statistics and allow retry with another file.
- Worker panic: convert to `Failed`; do not unwind through the egui update loop.

Failures do not invoke process stop routines and do not alter unrelated runtime state.

## Dependencies

- Use `rfd` for the native file chooser.
- Use `pcd-rs 0.13` dynamic-schema iteration for PCD 0.7 ASCII and binary parsing. Its packaged license is MIT.
- Reuse the existing optional `rerun` dependency and keep PCD-to-Rerun code feature-gated.

## Testing

Parser fixtures cover:

- ASCII XYZ.
- Binary XYZ.
- Packed `rgb:f32`.
- Packed `rgba:f32`.
- Separate RGB and RGBA channels.
- Normals, intensity, and unknown fields.
- Intensity fallback coloring.
- Height fallback coloring.
- NaN and infinite coordinate skipping.
- Missing required coordinates.
- Truncated binary records.
- Rejection of `binary_compressed`.

Runtime and UI tests cover:

- Loading runs outside the UI thread.
- Progress transitions are ordered and terminal states are recoverable.
- A second load cannot start while one is active.
- File-dialog cancellation preserves state.
- Rerun launch is tested through a replaceable launcher boundary so CI does not open a real window.
- Rerun launch failure affects only PCD viewer state.
- Existing process-control state and handles are unchanged by PCD viewer success and failure paths.
- Builds without `rerun` retain the rest of the application and show the feature requirement.

Verification commands:

```bash
cargo test
cargo test --features rerun
cargo clippy -- -D warnings
cargo clippy --features rerun -- -D warnings
```

## Acceptance Criteria

- A user can enter the Radar workspace, click `Open PCD in Rerun`, select an ASCII or binary PCD file, and see all valid points in a native Rerun Viewer.
- Common PCD color schemas are preserved; colorless data receives deterministic fallback coloring.
- Loading a multi-million-point file does not block the egui UI thread.
- Unsupported or malformed files produce actionable UI errors.
- A missing or closed Rerun Viewer does not crash radar-egui.
- PCD viewing does not change the lifecycle or state of SDR, Laser, ROS2 Radar, ZMQ, serial, video, or SHM runtimes.
