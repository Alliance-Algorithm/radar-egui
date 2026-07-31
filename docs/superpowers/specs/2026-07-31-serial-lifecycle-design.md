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

## Error Handling

- The close path is idempotent and safe when called after a failed open.
- Worker join failures are not expected from `JoinHandle::join`, but cleanup
  must still clear state if a worker panics.
- A stopped or failed worker must not leave the UI permanently showing an open
  connection on the next frame.

## Testing

- Test the receiver/transmitter stop behavior with the existing serial test
  infrastructure where practical.
- Test that opening failure leaves `serial_open` false and no worker handles.
- Test the existing parser notification behavior remains intact.
- Run `cargo fmt --all --check`, `cargo check`, and `cargo test`.
