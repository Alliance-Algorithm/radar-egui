# Serial Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Serial workspace safely open, close, reopen, and tear down its standard RX/TX threads.

**Architecture:** Keep serial I/O on `std::thread` as required by the existing blocking `serial2` path. Make RX reads and TX channel waits periodically interruptible, add a UI close action, and reuse the same idempotent cleanup path during application teardown.

**Tech Stack:** Rust, eframe/egui, `serial2`, `std::thread`, `std::sync::mpsc`, `Arc<AtomicBool>`.

## Global Constraints

- Do not convert the serial path to Tokio.
- Do not change DJI frame parsing, protocol IDs, or automatic ZMQ/Serial notification behavior.
- Do not add manual protocol-send controls in this change.
- The close path must be idempotent and must not block forever on an idle RX or TX worker.
- Preserve the existing port and baud selectors.
- Time constraints used consistently by implementation and tests:
  - `SERIAL_READ_TIMEOUT` = 50 ms, used only for the serial read timeout.
  - `SERIAL_WRITE_TIMEOUT` = 50 ms, used for write timeouts.
  - `SERIAL_TX_POLL_INTERVAL` = 50 ms, used for the TX channel `recv_timeout` poll.
  - Worker stop assertions accept a 500 ms upper bound for `join()` after the stop flag is set.

---

### Task 1: Make Serial Workers Interruptible

**Files:**
- Modify: `src/serial/serial.rs:44-63, 92-115, 120-203`
- Test: `src/serial/serial.rs` existing serial test module

**Interfaces:**
- Consumes: existing `Arc<AtomicBool>` stop flag and serial parser/channel interfaces.
- Produces: receiver and transmitter worker loops that return promptly after the stop flag is set.

- [ ] **Step 1: Write failing shutdown tests**

Add focused tests using the existing serial test infrastructure to assert that a worker can exit when idle after its stop flag is set. Keep the test independent of a physical UART by using the existing test serial construction or a local pseudo-terminal already supported by the repository.

- [ ] **Step 2: Run the focused tests to verify the failure**

Run: `cargo test serial:: -- --nocapture`

Expected: the new shutdown assertion fails or times out because the current receiver read and transmitter `recv()` wait indefinitely.

- [ ] **Step 3: Implement interruptible waits**

Change the receiver path so `receive_data()` returns periodically while waiting for bytes, allowing the worker loop to re-check `stop`. Keep parsed bytes and parser behavior unchanged. Change the transmitter from:

```rust
let Ok(idx) = tx_rx.recv() else { break };
```

to a `recv_timeout` loop that handles `Timeout` by checking `stop`, handles `Disconnected` by exiting, and processes received indexes exactly as before.

- [ ] **Step 4: Run the focused tests to verify the fix**

Run: `cargo test serial:: -- --nocapture`

Expected: all serial tests, including the new worker shutdown tests, pass.

- [ ] **Step 5: Commit the worker change**

```bash
git add src/serial/serial.rs
git commit -m "fix: make serial workers interruptible"
```

### Task 2: Add Serial Close UI and Lifecycle Cleanup

**Files:**
- Modify: `src/app/serial_workspace.rs:243-265`
- Modify: `src/app/mod.rs:235-291`
- Test: `src/app/serial_workspace.rs` existing state observation tests and app tests where lifecycle state is covered

**Interfaces:**
- Consumes: interruptible `serial_start_receiver` and `serial_start_transmitter` workers from Task 1.
- Produces: `Close serial` UI action, idempotent `close_serial()`, and application teardown cleanup.

- [ ] **Step 1: Write failing lifecycle tests**

Add tests for the state contract: opening failure leaves `serial_open == false`, a close operation clears `serial_open`, `serial_stop`, RX handle, and TX handle, and calling close while already closed is harmless. Cover worker-health failure reconciliation, a successful reopen after close, and repeated teardown. Use test-only constructors or helpers rather than opening a real device.

- [ ] **Step 2: Run the tests to verify the failure**

Run: `cargo test app::serial_workspace::tests app::tests -- --nocapture`

Expected: the new lifecycle assertions fail because the UI has no close action and the app lifecycle has no teardown hook.

- [ ] **Step 3: Implement the close action**

Render `Close serial` when `serial_open` is true and `Open serial` otherwise. On close, call `self.close_serial()`, append an `INFO` or `OK` log entry, and clear any stale serial error before a subsequent open attempt. Keep port and baud controls unchanged.

- [ ] **Step 4: Make cleanup idempotent and teardown-safe**

Ensure `close_serial()` always clears handles and state after signaling the stop flag, including when a worker panics. Add the app teardown path through the existing eframe lifecycle hook; `close_serial()` must remain safe and idempotent when teardown invokes it repeatedly, including repeated `on_exit` calls. Do not alter the parser's notification channels.

- [ ] **Step 5: Run the lifecycle tests to verify the fix**

Run: `cargo test app::serial_workspace::tests app::tests -- --nocapture`

Expected: all lifecycle assertions pass.

- [ ] **Step 6: Commit the UI and cleanup change**

```bash
git add src/app/serial_workspace.rs src/app/mod.rs
git commit -m "feat: add serial close lifecycle"
```

### Task 3: Full Verification

**Files:**
- Modify: none unless verification exposes a regression.

**Interfaces:**
- Consumes: completed serial worker and UI lifecycle implementation.
- Produces: verified branch state ready for review.

- [ ] **Step 1: Format and whitespace checks**

Run: `cargo fmt --all --check && git diff --check`

Expected: both commands exit successfully.

- [ ] **Step 2: Compile the project**

Run: `cargo check`

Expected: compilation succeeds. Existing warnings may remain, but no errors are allowed.

- [ ] **Step 3: Run the full test suite**

Run: `cargo test`

Expected: all runnable tests pass; only the existing hardware-dependent ignored test remains ignored.

- [ ] **Step 4: Review the final diff**

Run: `git status --short --branch && git diff HEAD~2..HEAD -- src/app/mod.rs src/app/serial_workspace.rs src/serial/serial.rs`

Confirm that only serial lifecycle behavior changed and protocol files remain behaviorally unchanged.
