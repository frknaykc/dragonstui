# DragonsTUI Adapter Protocol v1

Protocol v1 is newline-delimited JSON over a supervised adapter child process's stdin and stdout. Exactly one JSON object is emitted per line. The host writes `hello`, `request`, and `shutdown`; adapter stdout returns `adapter_info`, `response`, `error`, `event`, and `shutdown_ack`. Adapter stderr is diagnostics only and is never interpreted as protocol.

Every envelope has an explicit numeric `protocol` field. The M24–M34 host supports `1`; compatibility is established during handshake rather than assumed from a static manifest.

## Envelope types

| Type | Direction | Typed fields | Flexible field |
| --- | --- | --- | --- |
| `hello` | host → adapter | `protocol`, `host_version` | — |
| `adapter_info` | adapter → host | `protocol`, `id`, `version`, `capabilities` | — |
| `request` | host → adapter | `protocol`, `id`, `operation` | `payload` |
| `response` | adapter → host | `protocol`, `id` | `payload` |
| `error` | adapter → host | `protocol`, optional `id`, `code`, `message` | — |
| `event` | adapter → host | `protocol`, `stream`, `kind`, optional `observation` | `payload` |
| `shutdown` | host → adapter | `protocol` | — |
| `shutdown_ack` | adapter → host | `protocol` | — |

`payload` is JSON to keep adapter-specific domain data outside the host's generic model. Envelope, identity, capability, stream, and request-correlation fields remain typed.

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
- `RequestId`: a non-empty bounded ASCII correlation token.

Unknown message types, malformed JSON, and invalid typed identifiers are rejected. A validly decoded protocol number different from `1` is handled by compatibility negotiation, not silently accepted as running.

## Example

```json
{"type":"request","protocol":1,"id":"req-42","operation":"containers.list","payload":{}}
```

Protocol v1 does not define Docker, Git, database, Kubernetes, or any other domain payload schema.

## Runtime delivery and ordering

Adapter stdout is decoded through a bounded host ingress queue. When the queue is full the reader blocks, allowing the OS pipe to backpressure the adapter rather than dropping response data. Host event queues are independently bounded; their current policy is drop-oldest, with dropped-event counters exposed in diagnostics.

Message order is preserved for a single adapter's stdout stream. The host does not assign a cross-adapter global event order. Requests return a correlation ID immediately; callers poll the host and retrieve already-completed response/error outcomes without blocking a terminal UI loop.
