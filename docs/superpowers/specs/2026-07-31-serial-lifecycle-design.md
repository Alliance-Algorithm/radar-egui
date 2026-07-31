# Serial Lifecycle Design

## Goal

Make the Serial workspace support a complete open/close lifecycle without
changing the existing DJI protocol, parser, or automatic notification flow.

## Behavior

- The Serial sidebar shows `Open serial` while the port is closed.
- A successful open starts one receiver thread and one transmitter thread and
  changes the UI state to open.
- The Serial sidebar shows `Close serial` while the port is open.
- Closing sets the shared stop flag, lets both worker threads observe it, joins
  them, clears their handles, and changes the UI state to closed.
- Opening after closing uses the currently selected port and baud rate.
- Open failures leave the port closed and show the error without retaining stale
  worker state.
- Application teardown invokes the same close path so worker threads do not
  outlive the app.

## Thread Shutdown

- Receiver I/O must return control periodically while waiting for input so the
  stop flag can be checked. The parser and byte buffering remain unchanged.
- Transmitter channel receives must use a bounded timeout or equivalent wake-up
  mechanism so the stop flag can be checked while idle.
- Existing frame notifications remain unchanged: parsed frames update
  `SharedData` and notify ZMQ PUB and Serial TX through the existing channels.
- No manual protocol-send controls are added in this change.
- Time constraints:
  - `SERIAL_READ_TIMEOUT` (50 ms) for reads only.
  - `SERIAL_WRITE_TIMEOUT` (50 ms) for writes.
  - `SERIAL_TX_POLL_INTERVAL` (50 ms) for the TX channel poll.
  - Worker shutdown tests accept up to 500 ms for `join()` after the stop flag.

## Error Handling

- The close path is idempotent and safe when called after a failed open.
- `JoinHandle::join` can return `Err` when a worker panics; cleanup must still
  clear handles and state, and any such failure is recorded or handled rather
  than assumed away.
- A stopped or failed worker must not leave the UI permanently showing an open
  connection on the next frame; an automatic close records an error log entry
  with the same visibility as the manual close path.

## Testing

- Test the receiver/transmitter stop behavior with the existing serial test
  infrastructure where practical.
- Test that opening failure leaves `serial_open` false and no worker handles.
- Test worker-health failure reconciliation, a successful reopen after close,
  and repeated teardown/close idempotence.
- Test the existing parser notification behavior remains intact: a valid DJI
  frame updates `SharedData` and both notification channels receive the
  expected index.
- Run `cargo fmt --all --check`, `cargo check`, and `cargo test`.
