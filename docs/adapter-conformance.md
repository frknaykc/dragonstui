# Adapter Conformance Suite (M67)

`tools/adapter_conformance.py` is a dependency-free **POSIX developer test peer** for an explicitly supplied adapter executable. It checks the existing [protocol v1](adapter-protocol-v1.md), selected application scenarios, graceful shutdown and a fresh-process restart. It is not a controller-management command, installer, sandbox, security audit, publisher certificate, or M68 SDK specification.

## Run it

Requires Python **3.10+** on macOS or Linux. Building the bundled reference provider also requires the workspace Rust toolchain. No Docker, Git service, network endpoint or third-party Python package is needed.

```sh
cargo build -p dragonstui-adapter-host --bin dragonstui-adapter-host-mock

# Basic lifecycle only: no capability, action or session is invoked automatically.
python3 tools/adapter_conformance.py --expect-id reference --expect-version 1.0.0 \
  -- target/debug/dragonstui-adapter-host-mock --mode reference --id reference

# Explicit reference scenarios: RPC, declared errors, actions, events and session.
python3 tools/adapter_conformance.py --expect-id reference --expect-version 1.0.0 \
  --profile tools/fixtures/reference_conformance.json \
  -- target/debug/dragonstui-adapter-host-mock --mode reference --id reference
```

For an external adapter, replace the executable/arguments and expected identity; write a profile using **that adapter's** declared contracts. `test.echo`, fixture action IDs, echo output, and five typed observation variants are expectations of the example profile, not mandatory adapter capabilities. Do not use the gated M66 reference fixture here: unreleased gates intentionally exceed the test deadline.

The executable is resolved before launch; remaining arguments are passed literally, with **no shell interpretation**. Each run creates a private temporary working directory shared by its two successive provider processes. Use absolute paths for script/configuration arguments. Profiles are read from the caller's directory before launch. The runner does not attach to the controller daemon or read/write the configured adapter store.

**Only run trusted adapters with disposable test configuration.** A private working directory does not restrict filesystem, environment, network or user permissions. The adapter is started twice; profiles may invoke real side effects. Do not supply production credentials or production service arguments. Process-group cleanup covers the original group, not descendants that deliberately create a new session/group. SIGKILL of the runner cannot be cleaned up by Python; SIGINT/SIGTERM/SIGHUP are handled through a cancellation flag and bounded peer cleanup.

## What is checked

| Surface | Always / opt-in | Evidence and limits |
| --- | --- | --- |
| JSON Lines / wire types | Always, for received frames | Adapter-origin message direction, protocol 1, typed fields, identifiers, option/default behavior, observations and correlation/order. Unknown additive fields remain accepted. |
| Handshake | Always | Expected adapter ID; nonempty version and optional exact expected version; unique nonempty capability list. Wire versions are not forced into a new SemVer policy. |
| Declarations | Always | Decode actions/sessions; check their capability references against advertised capabilities, as required by the existing v1 document. This suite check is distinct from the runtime's more permissive wire handshake. |
| RPC / actions | Profile `requests` | Unique IDs; pipelined requests allow out-of-order replies. Assert response/error/either and optional payload/error-code equality. No unsolicited unknown/duplicate reply IDs. A global error without an ID is valid and counted without retaining its message. |
| Confirmation intent | Profile action | An action declaring `confirmation_required` needs profile `confirmed: true`. Ambiguous action identities cannot be selected. This is explicit noninteractive test intent, not UI-dialog verification, permission or authorization. |
| Events | Profile `events` | Minimum event count and optional typed observation kinds/generic event requirement. All received events are decoded even without an event scenario. No domain/payload heuristics. |
| Interactive session | Profile `session` | Declared capability, open correlation, exact provider session ownership, optional input/output assertion, resize **dispatch**, close followed by matching `session_exit`. Resize has no wire acknowledgement; applying dimensions or terminal emulation is not proven. |
| Shutdown | Always | `shutdown_ack`, no trailing protocol output, EOF and zero process exit within the phase budget. Forced kill is not a successful graceful shutdown. |
| Restart | After successful first cycle | New provider process, handshake and shutdown; application requests are **not repeated**. This is lifecycle restart, not a stress/crash-recovery certificate. |
| Cleanup | Every acquired peer | Reaped leader, forced-cleanup state and cleanup errors are explicit. A cleanup error cannot produce PASS. |

Missing optional scenarios appear as `skipped`, not passed. A provider without actions, observations or sessions is valid. The result covers only **the supplied scenarios and observed frames**; it does not certify every declared capability, unobserved future traffic, generic responsiveness under arbitrary load, or Windows compatibility. The M66 controller/UI acceptance remains separate.

## Profile format

The profile is a JSON object with `schema_version: 1`. Unknown profile fields, duplicate keys, invalid identifiers, nonfinite values, over-budget outgoing frames and invalid dimensions are rejected before launch. This is a test-configuration format, not a new adapter wire contract.

- `requests`: at most 32 objects. Required: unique `name`, declared `operation`, opaque `payload`, `expect` (`response`, `error`, `either`). Optional: declared `action`, boolean `confirmed`, `response_payload` (only with `response`), `error_code` (only with `error`). Requests are pipelined, not ordered application dependencies. Use separate profiles if an operation requires the result of another. Payload assertions compare canonical JSON without confusing `true` with `1`; numeric representation/adapter normalization can matter. Omit exact payload equality when only correlation/outcome is required.
- `session`: required `capability`, `rows`, `columns` (1–65535). Optional `input` (at most 8192 UTF-8 bytes), nonempty `expect_output_contains` (at most 8192 UTF-8 bytes, matched across chunks with bounded retention), `resize` containing rows/columns, and `exit_code` (i32 or null). Omit `exit_code` to accept any valid provider code; null requires no code. The chosen scenario expects the session to remain active until close; use a suitable nonterminating input. It is not an arbitrary scripting engine.
- `events`: positive `minimum` (at most 4096); optional unique `observation_types` chosen from `log`, `metric`, `status`, `event`, `error`; optional boolean `require_generic`.

See [the runnable reference profile](../tools/fixtures/reference_conformance.json). Declaration-dependent profile errors necessarily occur after handshake but before requested actions/sessions are dispatched.

## Bounds and output

`--timeout-ms` sets each phase budget: default 2000, allowed 100–30000. Transport reads/writes are nonblocking and drain both output channels during writes. Each provider cycle admits at most 4096 incoming frames and 4 MiB combined stdout/stderr traffic; a complete frame, including its newline, is at most 65536 bytes. The profile file is at most 128 KiB. Cleanup has its own bounded drain/reap budget. These are **runner limits**, not new production host limits or statements that an adapter exceeding them is unsafe. A timeout/budget failure can mean the profile/environment is unsuitable, not malformed protocol. OS executable lookup/spawn and filesystem operations still depend on the operating system.

Stdout contains one JSON result with `schema_version`, `status`, `scope: requested_scenarios_only`, checks and bounded observation counters. Counters include frames observed in both lifecycle cycles; the event scenario is satisfied during the first cycle. No raw request payload, provider payload, error message, stderr tail or executable argument is copied into diagnostic text. Raw malformed frames are not echoed. Argparse syntax errors use normal usage/stderr output rather than the semantic JSON report.

| Exit | Meaning |
| --- | --- |
| 0 | All requested checks passed; inspect `skipped` for untested surfaces. |
| 1 | Protocol/scenario/deadline/cleanup failure. Inspect the check name and stable code; not every failure is a wire violation. |
| 2 | CLI/profile configuration error. |
| 128 + signal | Catchable interruption; JSON status `interrupted` and cleanup evidence. |

## Maintaining the suite

```sh
python3 -m unittest discover -s tools -p 'test_adapter_conformance*.py'
cargo test -p dragonstui-adapter-host --test conformance_wire_parity -- --nocapture
```

The real Rust provider test requires a built debug mock or `DRAGONSTUI_CONFORMANCE_MOCK` pointing to a built executable; without one it reports an explicit unittest skip. Delivery must exercise it, not count the skip as acceptance. The independent minimal Python fixture demonstrates that the suite does not require reference-only names. Negative tests exercise malformed messages, metadata/correlation errors, no-read/no-shutdown peers, wrong session identity, missing release, trailing output, stderr pressure, traffic limits, inherited pipes and cancellation cleanup. Fault-injection unit tests are distinguished from real provider runs.

The Unix Rust integration test invokes `python3` and compares the Python parser/validator with the actual `serde_json::from_str::<ProtocolMessage>` implementation over a finite corpus. It is a test-time Python requirement, not a runtime dependency of the host or core framework. Recognized duplicate typed fields are rejected while opaque/additive duplicates retain the codec's tolerated behavior. Number decoding follows the current default serde_json feature configuration, including negative zero and overflow behavior; dependency/feature changes require rerunning parity, not assuming compatibility. The corpus is regression coverage, not proof over every JSON input.

M68 language SDK/specification work and M69–M71 stress, recovery and host security-limit work remain separate milestones.
