# Final Whole-Branch Review Fix Report

Status: DONE

Scope: `src/serial/serial.rs`, `src/app/mod.rs`, `Cargo.toml` (serial2 feature), docs.

## Findings Addressed

### P1 — `close_serial_workers()` joins TX on the UI thread while `send_data()` may block in `write_all()`

- Verified the vendored `serial2` 0.2.37 API: `set_write_timeout(&mut self, Duration)` bounds every
  individual `write()` call (unix impl polls `POLLOUT` with `write_timeout_ms`), and the inherent
  `write_all(&self)` loops over such bounded writes. No Tokio conversion involved.
- `Serial::new` and `clone_serial_port` now set a 50 ms write timeout on both the RX and TX port
  handles, so every write blocks at most 50 ms.
- The TX worker now sends through `send_data_interruptible()`, a stop-flag-aware write loop that
  re-checks `stop` between partial writes (instead of a single `write_all`), preserving identical
  byte output and the existing 5-frame robot-interaction pacing.
- The close path (UI-thread join in `close_serial_workers` / `on_exit` / worker reconcile) is now
  bounded: reads ≤ 50 ms (`TimedOut` → loop), channel waits ≤ 50 ms (`recv_timeout`), writes ≤ 50 ms
  per call. Safe handle cleanup and protocol behavior are preserved.

### P1 — a failed/stopped worker can leave `serial_open` permanently true

- Workers now take a shared `Arc<AtomicBool>` health flag and store `false` on any exit (stop flag,
  read error, channel disconnect, panic unwind).
- `RadarApp::reconcile_serial_workers()` runs every `update()` frame: if the port is open and either
  the health flag cleared or a `JoinHandle::is_finished()`, it records
  `serial_error = "serial worker stopped"`, runs the idempotent `close_serial_workers()` cleanup
  (stop flag + join + drop handles), clears the health handle, and sets `serial_open = false`.
- New test `reconcile_serial_workers_closes_after_worker_exit` covers the transition: closed state,
  recorded error, no stale handles, stop flag set.

### P2 — receiver integration test for parser notification behavior

- New test `serial::serial::tests::receiver_updates_shared_data_and_notifies_both_consumers` uses
  `serial2::SerialPort::pair()` (enabled via `serial2` `unix` feature; pty pair, unix-gated) to feed
  a valid DJI GameState frame through the receiver worker. It asserts `SharedData.game_state` is
  updated and both notification senders (ZMQ PUB channel + Serial TX channel) receive the expected
  idx `0`, preserving the existing parser notification flow.

### P2 — multi-frame shutdown test scheduling-only pass

- `multi_frame_transmitter_stops_between_targets` now synchronizes on actual frame-processing start:
  it spins on the worker's `sent` log until the first frame write began, then sets `stop` and asserts
  the worker joins within 80 ms. With 4 frames × ~100 ms pacing still pending at that point, a pass
  is only possible if the stop flag interrupts the in-progress multi-frame sequence.

## Additional Fix (test reliability)

- `RadarApp::default()` eagerly starts ZMQ and binds `tcp://*:5557` with an expect-panic on conflict;
  the new reconcile test and the round-2 lifecycle test race on that bind under parallel execution
  (observed `ZMQ PUB init failed: Address already in use`). Added a test-only static
  `ZMQ_TEST_PORT_LOCK` plus an immediate `zmq_pub.stop()` in a shared `radar_app_for_test()` helper
  so the bind window is serialized and the port is released deterministically before the app drops.
  Production code untouched.

## Verification

- `cargo test serial::` — 4 passed (incl. 2 new/updated worker tests), 0 failed.
- `cargo test app::` — 17 passed (3 runs), 0 failed.
- `cargo test` (full suite, twice) — 85 lib + 7 integration passed, 0 failed; only the existing
  hardware-dependent `test_tx_continuous` remains ignored.
- `cargo fmt --all --check` — passed.
- `git diff --check` — passed.
- `cargo check` — passed; only the repository's 61 pre-existing warnings.

## Constraints Preserved

- Serial I/O stays on `std::thread`; no Tokio conversion.
- DJI protocol IDs, parser semantics, and automatic ZMQ/Serial notifications unchanged.
- Port/baud selectors and the `Open serial` / `Close serial` UI contract unchanged.
- `send_data()` retained (used by the ignored hardware integration test); now bounded by the write
  timeout as well.
