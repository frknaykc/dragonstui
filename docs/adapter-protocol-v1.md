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
| `event` | adapter → host | `protocol`, `stream`, `kind` | `payload` |
| `shutdown` | host → adapter | `protocol` | — |
| `shutdown_ack` | adapter → host | `protocol` | — |

`payload` is JSON to keep adapter-specific domain data outside the host's generic model. Envelope, identity, capability, stream, and request-correlation fields remain typed.

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
