# DragonsTUI Adapter Protocol v1

Protocol v1 is newline-delimited JSON over a supervised adapter child process's stdin and stdout. Exactly one JSON object is emitted per line. The host writes `hello`, `request`, `session_open`, `session_input`, `session_resize`, `session_close`, and `shutdown`; adapter stdout returns `adapter_info`, `response`, `error`, `event`, `session_opened`, `session_output`, `session_exit`, and `shutdown_ack`. Adapter stderr is diagnostics only and is never interpreted as protocol.

Every envelope has an explicit numeric `protocol` field. The M24–M34 host supports `1`; compatibility is established during handshake rather than assumed from a static manifest.

## Envelope types

| Type | Direction | Typed fields | Flexible field |
| --- | --- | --- | --- |
| `hello` | host → adapter | `protocol`, `host_version` | — |
| `adapter_info` | adapter → host | `protocol`, `id`, `version`, `capabilities`, optional `actions`, optional `sessions` | — |
| `request` | host → adapter | `protocol`, `id`, `operation`, optional `action` | `payload` |
| `response` | adapter → host | `protocol`, `id` | `payload` |
| `error` | adapter → host | `protocol`, optional `id`, `code`, `message` | — |
| `event` | adapter → host | `protocol`, `stream`, `kind`, optional `observation` | `payload` |
| `session_open` | host → adapter | `protocol`, `id`, declared `capability`, `rows`, `columns` | — |
| `session_opened` | adapter → host | `protocol`, request `id`, provider `session_id` | — |
| `session_input` | host → adapter | `protocol`, `session_id`, `data` | — |
| `session_resize` | host → adapter | `protocol`, `session_id`, `rows`, `columns` | — |
| `session_close` | host → adapter | `protocol`, `session_id` | — |
| `session_output` | adapter → host | `protocol`, `session_id`, `data` | — |
| `session_exit` | adapter → host | `protocol`, `session_id`, optional `exit_code` | — |
| `shutdown` | host → adapter | `protocol` | — |
| `shutdown_ack` | adapter → host | `protocol` | — |

`payload` is JSON to keep adapter-specific domain data outside the host's generic model. Envelope, identity, capability, stream, and request-correlation fields remain typed.

## Adapter actions and confirmation

An adapter may declare ordered `actions` in `adapter_info`. Each action has a producer-owned `id`, human-readable `label`, optional `description`, an existing capability `operation`, and an optional `confirmation_required` boolean. An action invocation is a normal `request` carrying the declared `operation`, opaque `payload`, and optional `action` identity. The host never derives action semantics from an adapter name, action ID, label, capability text, stream, kind, or payload keys.

`confirmation_required: true` is producer-declared **UI confirmation policy**: a compliant UI must require a distinct user confirmation before sending that invocation. Omission defaults to `false`; an alarming-looking identifier remains directly invokable when the producer declares no confirmation, while a harmless-looking identifier can require confirmation. The UI captures the adapter ID, action ID, and payload when opening confirmation, so selection changes cannot redirect the eventual request.

Confirmation is not authorization. It prevents accidental UI dispatch only; it does not grant permissions, prove an adapter is safe, or provide an adapter-side idempotency guarantee. Existing authenticated loopback controller IPC continues to authenticate callers before dispatch, but protocol v1 has no action-specific permission or RBAC authority to enforce. No such claim is made by `confirmation_required`.

## Interactive sessions

An adapter may declare ordered `sessions` in `adapter_info`. Each declaration contains an existing capability, producer-owned human-readable `label`, and optional `description`. A host may present only these declarations; it never infers an interactive surface from adapter identity, capability text, action labels, streams, kinds, or payloads.

Opening a declaration sends `session_open` with that exact capability and the dimensions of the rendered host area. `session_opened` correlates the request ID to a provider-owned `session_id`. Subsequent `session_input`, `session_resize`, and `session_close` messages carry that session identity. Input and output are opaque text at this boundary: the host does not parse commands, derive terminal semantics, or emulate a provider terminal.

`session_output` streams text for the matching active session. `session_exit` is authoritative terminal state and may carry an optional provider exit code; there is no synthetic success response for close. A compliant host must keep session ownership tied to both adapter and session identity, reject input/resize after a close is pending, and bound retained output/events.

`sessions` is optional and additive, so existing adapters decode with no declared session surfaces and retain their lifecycle, capability, RPC, action, and observability behavior. Session envelopes are sent only after an explicit host open request, so an adapter that does not declare a surface is never asked to implement one.

## Observability event semantics

An event's `kind` remains an opaque producer label used for generic routing and filtering. It is **not** a log level, metric name, status state, or UI widget selector. An adapter can optionally attach an `observation` when it declares a capability-neutral meaning that future observability projections may consume without examining arbitrary payload keys. Omitted `observation` means **Generic/Unclassified**.

`observation` is an internally tagged JSON object with a required `type`:

| Type | Required fields | Optional fields | Purpose |
| --- | --- | --- | --- |
| `log` | `text` | `severity`, `timestamp_millis` | Human-readable log record. |
| `metric` | `name`, JSON numeric `value` | `unit`, `timestamp_millis` | One typed numeric series sample. |
| `status` | `entity`, `check`, `status` | `timestamp_millis` | Generic entity/check state observation. |
| `event` | `title` | `detail`, `timestamp_millis` | Chronological human-readable observation. |
| `error` | `message` | `signature`, `stack`, `timestamp_millis` | Generic error with textual stack lines. |

`severity` is one of `trace`, `debug`, `info`, `warning`, `error`, or `critical`. `status` is one of `ok`, `warning`, `error`, or `unknown`. These are deliberately small generic enums; no product, transport, database, or framework state is encoded.

`timestamp_millis`, when present, is a producer-declared Unix epoch millisecond timestamp. The host never fabricates it from receipt time. When absent, the event has only its producer stdout order; application retained-history entries may add their own local sequence for UI selection, but that sequence is not a producer timestamp and has no cross-adapter ordering meaning.

Metric `value` is a `serde_json::Number`, so valid wire values are JSON numbers and `null`, strings, `NaN`, and infinities are rejected rather than coerced. `stack` is a vector of producer textual lines; the protocol does not parse language-specific frames. `signature` is optional producer context for a future grouping projection and does not perform grouping itself.

There is no `heatmap` or `timeline` wire type. The M53–M58 showcase projects named Metric samples into a bounded generic heatmap and projects explicit Event observations into its Timeline. Timeline orders timestamped Event observations by producer timestamp (stable retained sequence breaks ties); entries without a timestamp follow in retained arrival order. This keeps protocol semantics about observations rather than widgets.

### Compatibility and versioning

`observation` is additive and optional, so protocol v1 and its handshake version remain unchanged. Existing adapters can continue emitting `stream`, `kind`, and `payload` only; a current host decodes those events as Generic/Unclassified with all original values unchanged. A new adapter can attach typed metadata without duplicating or replacing its opaque `payload`.

The current serde policy ignores unknown additive fields within a recognized observation, but rejects an unknown `observation.type` cleanly as malformed protocol input. Older v1 hosts use the same tolerant unknown-field behavior and therefore ignore the new optional `observation` field while retaining their existing event fields. No capability negotiation or new endpoint is required.

The semantic contract backs M53 Log, M54 metric graph, M55 heatmap-from-metrics, M56 status matrix, M57 chronological Timeline, and M58 error/stack projections. All are derived from the showcase's bounded retained history: Error grouping prefers `signature`, falls back to `message`, and its count/first/last values may decrease when retained source entries are evicted.

## Identifiers

- `AdapterId`: one lower-case ASCII segment beginning with `[a-z0-9]`, then lower-case ASCII alphanumerics, `_`, or `-`.
- `Capability`: one or more such segments joined by `.`; for example `containers.logs` or `test.echo`.
- `ActionId`: one or more such segments joined by `.`; it is an opaque producer identity, not a policy selector.
- `RequestId`: a non-empty bounded ASCII correlation token.
- `SessionId`: one lower-case ASCII segment using the same validation as `AdapterId`; it is provider-owned and is not a terminal, shell, or product identity.

Unknown message types, malformed JSON, and invalid typed identifiers are rejected. A validly decoded protocol number different from `1` is handled by compatibility negotiation, not silently accepted as running.

## Example

```json
{"type":"request","protocol":1,"id":"req-42","operation":"containers.list","payload":{}}
```

Protocol v1 does not define Docker, Git, database, Kubernetes, or any other domain payload schema.

## Runtime delivery and ordering

Adapter stdout is decoded through a bounded host ingress queue. When the queue is full the reader blocks, allowing the OS pipe to backpressure the adapter rather than dropping response data. Host event queues are independently bounded; their current policy is drop-oldest, with dropped-event counters exposed in diagnostics.

Message order is preserved for a single adapter's stdout stream. The host does not assign a cross-adapter global event order. Requests return a correlation ID immediately; callers poll the host and retrieve already-completed response/error outcomes without blocking a terminal UI loop.
