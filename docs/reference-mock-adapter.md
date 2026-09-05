# Reference Mock Adapter (M66)

The existing `dragonstui-adapter-host-mock` binary has a combined `--mode reference` provider. One external process exposes the contracts implemented through M65: generic RPC, typed observations, declared actions and interactive echo sessions. Docker, Git, a network service and credentials for an external provider are not required.

This is deliberately a **fixture provider**. It does not execute input as shell commands, provide a terminal emulator, deploy anything, or implement a domain adapter. Protocol v1 is unchanged. The separate [M67 conformance runner](adapter-conformance.md) can exercise this provider through an explicit example profile; the [M68 SDK specification](adapter-sdk-specification.md) documents the implementation contract without publishing language SDK libraries.

## Build and create an isolated adapter root

Run from the repository root on macOS or Linux with Rust (edition 2024 support) and Python 3.10+:

```sh
cargo build --release -p dragonstui-adapter-host --bins
cargo build --release --features adapter-showcase --bin dragonstui-showcase
python3 tools/reference_mock_fixture.py --root /tmp/dragonstui-reference-demo --mock target/release/dragonstui-adapter-host-mock --gated
target/release/dragonstui-adapter --root /tmp/dragonstui-reference-demo list
target/release/dragonstui-adapter --root /tmp/dragonstui-reference-demo info reference
```

Choose a **new** root; setup refuses an existing directory or symlink. Its parent must exist. Setup copies the built binary, writes a manifest and a quoted POSIX launcher, and creates private marker storage. It does not start the adapter or a daemon, download anything, or touch an existing adapter store. The printed JSON includes the copied binary's SHA-256. Launchers contain absolute paths: regenerate into a new root rather than moving the fixture.

The binary can be run directly with protocol JSON Lines on stdin/stdout; it waits for a host `hello`. `--help` describes supported fixture options. The setup launcher and PTY acceptance are POSIX-only, not Windows acceptance claims.

## Interactive walkthrough

```sh
target/release/dragonstui-adapter --root /tmp/dragonstui-reference-demo start reference
target/release/dragonstui-showcase --adapter-root /tmp/dragonstui-reference-demo
```

- Skip the splash with Enter; press `8` for Adapters. The only provider is **Reference Mock**.
- Wait until the inspector reports **Live events received: 8**. In another terminal, release the second observation batch:

```sh
python3 -c 'from pathlib import Path; Path("/tmp/dragonstui-reference-demo/.reference-control/observations.release").touch()'
```

- At **16** received events, `o` opens Observability. Keys `1`–`6` select Logs, Metrics, Heatmap, Status, Timeline and Errors. Error detail includes textual stack frames. `o` returns to Adapters.
- `a` opens declared actions. Alpha succeeds; Inspect is rejected; Confirm inspection requires a distinct confirmation. Escape cancels without sending the action. **Confirmation is not authorization or RBAC.**
- Before selecting Delta in this manual walkthrough, release its gate from another terminal. The host's existing action deadline is **two seconds**; a marker does not pause or extend that deadline. Leaving it held longer deliberately yields an operation timeout, not indefinite Running:

```sh
python3 -c 'from pathlib import Path; Path("/tmp/dragonstui-reference-demo/.reference-control/actions.release").touch()'
```

- Select Delta and invoke it to see success. Leave Actions using `a`; `h` discovers this provider's session and Enter opens it. Input is echoed, resize is reported, and Alt+X requests close. Within a session, Ctrl+E emits a bounded output burst and exits nonzero. Open another session and exit the showcase with `q` or Ctrl+C to exercise host cleanup.
- The automated runner—not a manual pause—proves a session input round-trip while Delta is held, releases it within the unchanged host deadline, and verifies the terminal operation result. Resize and close follow independently.
- Stop the provider when finished:

```sh
target/release/dragonstui-adapter --root /tmp/dragonstui-reference-demo stop reference
```

The CLI's controller daemon is intentionally independent of the TUI; exiting the TUI is not a request to stop every adapter. The automated PTY runner below owns and tears down its own daemon and fixture instead. Marker files persist across restarts: recreate a fresh fixture to repeat a gated scenario from its initial state. Without `--gated`, both startup batches are immediate and Delta completes without manual release; lossy bounded UI queues may coalesce/drop a burst, so use gates when asserting all samples reached the UI.

## Feature and evidence matrix

| Surface | Reference behavior | Acceptance |
| --- | --- | --- |
| Handshake / discovery | Matching manifest identity/version, protocol v1, declared capabilities/actions/sessions | Real protocol child and controller tests; CLI list/info |
| RPC | `test.echo` preserves opaque JSON; `test.stream` emits a generic event plus the full observation sample set and acknowledges | Protocol child tests |
| RPC failure | `test.fail` returns correlated `test_failed`; `test.crash` exits provider with code 24; `test.slow` deliberately blocks for timeout testing | Protocol/controller controlled-failure tests; legacy fault-mode tests |
| Observability | Two eight-event batches of Log, Metric, Status, Event and Error; fixture timestamps and missing timestamps are deterministic | Protocol/controller tests; all six real TUI projections |
| Actions | Existing Alpha, Inspect, Confirm inspection and Delta declarations | Controller metadata/operation tests; UI cancel/confirm enforcement and producer dispatch markers in real PTY |
| Operation state | Host owns Pending → Running → Succeeded/Failed; gated Delta permits sessions and other RPC while held | Controller and TUI acceptance (release within the host deadline) |
| Sessions | `fixture.terminal` explicitly declared; fixed provider-local `fixture-session`; echo, resize, typed close/exit | Protocol/controller and real PTY |
| Session failure | `fixture.exit-nonzero` input exits 7; Ctrl+E emits 16 output records then exits 2; `fixture.crash-provider` exits provider 37 | Protocol/controller and burst-exit PTY evidence |
| Lifecycle | Controller start/stop/restart, crash detection and fresh handshake | Controller integration tests |
| Terminal / cleanup | Active-session outer exit closes provider registry before daemon teardown; canonical/echo and full termios, alternate screen, cursor, mouse, usable terminal | Reference PTY runner; legacy showcase runner |
| Installation boundaries | New-root-only manifest/launcher generation, local binary copy, no auto-start; this is not registry artifact installation | Setup unit tests; existing installer suites remain authoritative |

Diff/Code views are framework primitives, not new adapter wire message types. Heatmap consumes Metric observations; Timeline consumes Event observations. The reference adapter does not invent `heatmap`, `timeline`, `code` or `diff` envelopes.

## Deterministic controls and legacy fault fixtures

- `--id`: the provider identity must match its manifest. IDs/capabilities remain opaque to the host/UI.
- `--event-release PATH`: first startup batch is immediate; the second waits until this file exists. Marker waiting must not block protocol input or shutdown.
- `--action-release PATH`: reference Delta waits for this file rather than a wall-clock guess; pending delayed work is bounded. The controller still times out an action after two seconds. Release within that window for success; a later release cannot change an already-failed host operation.
- `--action-marker PATH`: append-only action dispatch evidence. Cancelled UI confirmation must leave it unchanged; it is not an audit/security boundary.
- `--session-marker PATH`: current provider session registry, not a request log. Empty after close/exit is provider-side release evidence. Killing a process outside its cleanup path is not proof of marker cleanup.

Reference mode admits at most **1024 unique RPC request IDs per process**, including failed requests. Further new IDs receive `fixture_request_limit`; repeated admitted IDs receive `duplicate_request`. Session messages and shutdown remain available, and restart creates a fresh ID budget. Delta has one pending slot: another concurrent Delta receives `fixture_action_busy`. These are reference-fixture bounds, not a new host-wide protocol limit or an idempotency guarantee.

Existing modes remain specialized negative/pressure fixtures rather than separate reference products: `normal`, `process`, `bad-protocol`, `bad-id`, `malformed`, `crash`, `timeout`, `hold`, `duplicate-capabilities`, `empty-capabilities`, `shared-capabilities`, `events`, `live-events`, `semantic-events`, `observability-events`, `actions`, `sessions`, `delayed-sessions`, `stress-events`, `out-of-order`, `unknown-response`, `crash-after-handshake`, `crash-on-request`.

`test.slow` and timeout/crash/malformed modes intentionally do not behave like a healthy adapter. Invoke them only in isolated tests. No claim of adapter sandboxing, publisher authenticity, action-specific permissions, native shell support or cross-language conformance is made.

## Automated acceptance

```sh
cargo test -p dragonstui-adapter-host --test reference_mock --test reference_controller
python3 -m unittest discover -s tools -p 'test_reference_mock*.py'
python3 -m unittest discover -s tools -p 'test_showcase_pty_smoke.py'
python3 tools/reference_mock_pty_smoke.py --controller target/release/dragonstui-adapter --mock target/release/dragonstui-adapter-host-mock --showcase target/release/dragonstui-showcase --exit q
python3 tools/reference_mock_pty_smoke.py --controller target/release/dragonstui-adapter --mock target/release/dragonstui-adapter-host-mock --showcase target/release/dragonstui-showcase --exit ctrl-c
```

The PTY runner also accepts `sigterm` and `sighup`. It creates its own private root and authenticated controller daemon, drives one reference provider through the real showcase, checks marker state before teardown, and asserts no fixture processes remain. Marker waits continue draining terminal output. Failures print the reconstructed current screen and only fixture action/session state—never endpoint tokens. Successful JSON reports contain actual checks and unchanged binary hashes.

The UI may report **authoritative inactivity without an exit code** when the bounded controller event window loses a terminal notification. This is the existing M65 contract, not a success-code substitution. After Ctrl+E, the PTY runner requires the session browser, an empty producer registry and either explicit code `2` or the controller-backed inactivity status. Any explicit different code fails. Its JSON distinguishes `exit_code: 2` from `authoritative_inactivity` with `exit_code: null`; it never counts the latter as proof of code 2. Exact provider nonzero codes and burst output are separately verified by the real protocol/controller tests. Browser return or a disconnect message alone is insufficient.

Run the repository's full Rust gates at final delivery. Passing these M66 fixtures is not a certificate that an arbitrary third-party adapter conforms to protocol v1.
