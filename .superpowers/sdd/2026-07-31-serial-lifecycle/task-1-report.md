# Task 1 Implementation Report

Status: DONE_WITH_CONCERNS

## Changed Files

- `src/serial/serial.rs`
  - Configured serial reads with a 50 ms timeout so idle receiver reads return and the stop flag can be rechecked.
  - Preserved parser and receiver notification behavior; timeout and other I/O errors are retried as before.
  - Replaced the transmitter's blocking `recv()` with `recv_timeout`, exiting on stop or channel disconnect and preserving index processing.
  - Added focused idle receiver and transmitter shutdown tests using `/dev/ptmx`.

## Commit

- Implementation commit: `5f15b0b` (`fix: make serial workers interruptible`)

## Tests

- `cargo test serial:: -- --nocapture`
  - Passed. Library serial tests: 2 passed, 0 failed. Existing integration serial tests: 1 passed, 1 ignored (hardware-dependent `test_tx_continuous`).
- `cargo fmt -- src/serial/serial.rs`
  - Passed.
- `git diff --check`
  - Passed.

## Concerns

- The focused shutdown tests use Linux `/dev/ptmx` and are guarded only for Unix on the receiver test. They are therefore not portable to Windows and require a Unix-like test environment.
- The repository emits pre-existing compiler warnings during the focused test command; no warning cleanup was included in Task 1.
- The report is a separate uncommitted documentation change at the time of implementation commit `5f15b0b`.

## Review Fix Report

### Fixes

- Added stop checks between idx 6 target frames and during the existing 100 ms pacing interval by using ten interruptible 10 ms sleeps. Frame order, payload construction, and pacing while running are unchanged.
- Added the Unix guard to the idle transmitter shutdown test, matching the receiver test and its `/dev/ptmx` dependency.
- Added a focused idx 6 shutdown regression test.

### Verification

- `cargo test serial::serial::tests::multi_frame_transmitter_stops_between_targets -- --nocapture`
  - Passed: 1 test passed, 0 failed.
- `cargo test serial:: -- --nocapture`
  - Passed after the fixes: all serial tests passed; the existing hardware-dependent test remains ignored.
- `cargo fmt --all --check`
  - Passed.
- `git diff --check`
  - Passed.

### Remaining Concerns

- The serial shutdown tests remain Unix-specific because they use `/dev/ptmx`; the transmitter test is now consistently guarded.
- Existing repository compiler warnings remain outside this fix.

### Fix Commit

- `73a14dd` (`fix: interrupt multi-frame serial shutdown`)
