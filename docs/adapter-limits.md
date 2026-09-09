# Adapter host limits (M71)

These are host implementation limits, not a new protocol version. They apply in
`dragonstui-adapter-host`; the core rendering crate gains no dependency or runtime
authority. The controller remains the lifecycle owner. No automatic restart,
heartbeat or retry policy is introduced.

| Boundary | Policy |
| --- | --- |
| Child stdout JSON Lines | 1 MiB per wire line, including LF/CRLF; bounded read before JSON decoding |
| Host-to-child messages | 1 MiB including LF; bounded serialization completes before any stdin write |
| Child stderr | 4 KiB per wire line; oversized lines are drained without retaining the entire line and replaced by a diagnostic marker; existing tail line count still applies |
| Manifest | 64 KiB UTF-8 document, including whitespace; bounded disk reads for discovery/update and bounded manifest serialization during installation |
| Manifest executable | At most 4096 encoded path bytes, no NUL; existing relative normal-component validation and discovery canonical containment remain in force |
| Streaming rate | Default 100,000 `event`/`session_output` envelopes per adapter per fixed one-second ingress window |
| Retained request records | Admission stops at 128 combined pending requests, completed responses, failures and unclaimed session-open acknowledgements; draining a result releases admission |
| Timeout arguments | Handshake, RPC/session-open deadlines, response waits and process grace/kill waits clamp to 300 seconds; zero remains zero |

`AdapterRuntimeConfig::event_rate_limit` and
`AdapterProcessConfig::event_rate_limit` let a trusted embedding host choose the
streaming budget. Zero disallows streaming envelopes. Responses and session
lifecycle envelopes do not spend this budget. Windows reset on the first stream
envelope after a second has elapsed; two bursts across the boundary are possible.
This measures reader-observed ingress, not the producer's wall-clock send rate.
Existing bounded stdout backpressure can slow observation of a producer.

## Failure behavior

A stdout byte/rate violation is retained outside the ordinary stdout queue in a
single atomic notification. The reader stops. On the next runtime pump, even when
completed response storage is full, the host closes stdin, kills/reaps the direct
child and records a terminal crash with an explicit limit diagnosis. Pending
requests fail once and subsequent polling preserves that diagnosis. Other
runtimes are unaffected; restart requires an explicit lifecycle action. Previously
completed responses remain available. This policy does not silently drop the
excess stream and continue execution.

Outbound serialization rejection writes no partial frame and returns a failed
request without classifying a healthy child as crashed. The caller still owns
its original payload allocation. Invalid manifest data is classified per candidate
rather than hiding healthy discovery results.

## Scope and compatibility

Protocol envelopes, capability identifiers, correlation fields and opaque payload
semantics are unchanged. Existing response/event/session queue settings retain
their semantics; the new request admission bound additionally covers timeout and
crash failure retention. Callers that submit more than 128 unconsumed requests must
handle `RpcError::Backpressure`. The new `ProcessError::LimitExceeded` and
`ManifestError::TooLarge` variants extend the pre-1.0 public error enums.

The M69 stress fixture explicitly chooses a 10,000,000-envelope benchmark budget,
not the production default. Published M69 measurements are historical evidence,
not measurements of M71 source.

These controls are not a sandbox or a total process-memory/CPU guarantee. Decoded
JSON has allocation overhead. Trusted embedding configuration and caller-owned
values are outside these input byte budgets. The host does not pin executable
inodes against concurrent local filesystem replacement or add process-tree
containment. Direct process configuration remains a trusted explicit executable
selection, separate from manifest-relative discovery. OS component/path limits
can be stricter than the manifest path budget.

Timeout clamping prevents oversized duration arithmetic; it does not make
synchronous stdin writes cancellable or impose a universal shutdown deadline.
Lifecycle enforcement requires continued host polling. Stderr draining does not
impose a diagnostic throughput limit or a reader-thread join guarantee. See also
[crash/recovery boundaries](adapter-crash-recovery.md).

## Verification

### M74 controller IPC transport

Controller requests and responses have a separate 64 MiB encoded JSON budget
(excluding the terminating LF), not the 1 MiB child-message limit. Aggregated
diagnostics can legitimately contain multiple child frames. Reads are bounded
before decoding; serialization finishes within its byte budget before writing.
Oversized responses produce an explicit error rather than a truncated success.
Drain-style responses already consumed from controller state are not replayed
automatically after an encoding or delivery failure.

Each accepted connection (client: completed connection) has one five-second
absolute exchange deadline; partial progress does not renew it. Socket waits
use at most 10 ms slices, polling the controller between server-side waits so
ordinary runtime servicing continues. Timeout/disconnect closes that exchange,
not the controller. The single-connection server is not a fair concurrent
dispatcher. Synchronous command execution and JSON decoding are not preempted;
their surrounding deadline checks are not a universal command/runtime deadline.
Transport buffers are bounded, not all controller state or decoded allocations.
Connect retries remain separately bounded by the existing client policy.

Small in-memory codec/deadline tests cover the transport boundaries; ordinary
controller integration and CLI lifecycle tests cover successful wire exchanges.
This is not an adversarial load, OS socket-timing or real-time scheduling claim.

### Earlier child/runtime coverage

Small deterministic unit fixtures cover wire-line/writer boundaries, diagnostic
truncation, fixed-window admission, timeout clamping and retained-failure admission.
Integration tests cover a zero stream budget with ordinary mock traffic, direct
child termination, healthy-peer RPC, stable diagnosis, full response storage,
manifest byte boundaries and executable-path validation. No third-party services
or adversarial traffic generators are required.
