# Task 2 Report: Serial Close Lifecycle

## Status

DONE_WITH_CONCERNS

## Changed Files

- `src/app/serial_workspace.rs`
- `src/app/mod.rs`

The required report file is also added at:

- `.superpowers/sdd/2026-07-31-serial-lifecycle/task-2-report.md`

## Implementation

- Added `Close serial` UI action when the serial connection is open.
- Kept `Open serial` available when closed and preserved port and baud controls.
- Close clears stale serial errors, signals the shared stop flag, joins RX/TX workers, and clears all lifecycle handles/state.
- Cleanup remains idempotent and clears handles even when a worker panics during join.
- Added `eframe::App::on_exit` teardown cleanup using the existing app lifecycle hook.
- Preserved parser notification channels and the `std::thread` serial worker model.
- Added lifecycle tests for open failure state, idempotent close, and panic-safe worker cleanup.

## Commit

- `c389164` (`feat: add serial close lifecycle`)

## Tests

- `cargo test app::serial_workspace::tests -- --nocapture`: PASS, 4 passed, 0 failed.
- `cargo test app::tests -- --nocapture`: PASS, 7 passed, 0 failed.
- `cargo fmt --check`: PASS.
- `git diff --check`: PASS.

The brief's combined command, `cargo test app::serial_workspace::tests app::tests -- --nocapture`, was rejected by Cargo because `cargo test` accepts only one positional test filter. The two valid focused commands above were run separately.

## Concerns

- The focused test runs emit 61 pre-existing compiler warnings in unrelated modules, including deprecated egui APIs, unused imports, and future-incompatible float literal fallback warnings. No warning cleanup was included because Task 2 is scoped to the serial lifecycle files.
- The panic-safe lifecycle test intentionally spawns a panicking worker, so Rust prints the expected worker panic message even though the test passes.
- The report file is uncommitted because the requested implementation commit was created before the SHA-dependent report was written. The implementation commit contains only the two Task 2 source files; the report remains as a working-tree addition.

## Review Fix

The lifecycle coverage review was addressed by adding tests that exercise the actual `RadarApp` cleanup paths with synthetic `std::thread` handles:

- `app::tests::close_serial_and_on_exit_clear_connection_state_and_all_workers` calls `RadarApp::close_serial()`, then invokes `eframe::App::on_exit()` twice, asserting `serial_open`, `serial_error`, `serial_stop`, RX handle, and TX handle are cleared after each lifecycle transition.
- `app::tests::failed_reopen_keeps_serial_closed_without_uart` covers the closed/error state transition without opening physical UART hardware.
- Existing worker-level tests remain, and lifecycle state placement stays in `app::tests` where `RadarApp` private fields and cleanup methods require it.

## Review-Fix Verification

- `cargo test app::serial_workspace::tests -- --nocapture`: PASS, 4 passed, 0 failed.
- `cargo test app::tests -- --nocapture`: PASS, 9 passed, 0 failed.
- `cargo fmt --all --check`: PASS.
- `git diff --check`: PASS.

## Review-Fix Concerns

- Focused test runs still emit the repository's pre-existing unrelated compiler warnings.
- Panic-safe synthetic worker tests intentionally print expected worker panic messages while `JoinHandle::join()` safely collects the panic.
