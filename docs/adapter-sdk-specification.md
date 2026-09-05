# Adapter SDK Specification (M68)

## Status and scope

This is the implementation specification for external protocol v1 adapters, not a published Rust crate, Go module or Python package. It consolidates the existing contract without changing protocol v1 or promising a stable in-process ABI. The project remains pre-1.0. Rust, Go and Python are implementation choices, not separately certified SDKs.

Use this document with the [wire message reference](adapter-protocol-v1.md), [host architecture](architecture/adapter-host.md), [management guide](adapter-management.md) and [conformance runner](adapter-conformance.md). Domain payload schemas belong to the adapter author.

**Rule categories:** “wire” describes current decoding; “runtime” describes host enforcement; “producer guidance” describes interoperable implementation behavior; “suite” describes selected M67 acceptance checks. Guidance is not a claim that the runtime rejects every violation. The implementation anchors are [protocol.rs](../crates/dragonstui-adapter-host/src/protocol.rs), [runtime.rs](../crates/dragonstui-adapter-host/src/runtime.rs), [process.rs](../crates/dragonstui-adapter-host/src/process.rs) and [manifest.rs](../crates/dragonstui-adapter-host/src/manifest.rs). Resolve discrepancies against the current source and regression tests rather than silently introducing a stricter v1 dialect.

## Transport and ownership

- The adapter is a supervised executable. Read UTF-8 JSON Lines from stdin; write exactly one JSON object followed by newline per stdout frame. Encode embedded newlines inside strings. No startup banner, progress display or logging belongs on stdout; stderr is diagnostics only.
- Flush each completed frame. Serialize concurrent writers through one bounded output path so bytes from separate frames cannot interleave. Keep processing input while work is pending; bound worker count, pending operations, output and diagnostics. Do not replace backpressure with unlimited queues.
- The controller daemon owns application runtime lifecycle. An adapter implementation does not need controller endpoint credentials, a second manager, TUI state or a dependency on the core `dragons_tui` crate. The conformance runner is a separate explicit test peer, not an alternative application manager.
- A child process boundary is not a sandbox. Adapter code executes with the user's permissions. Do not depend on inherited terminal access or print sensitive input into diagnostics. Provider-owned subprocesses and resources remain the provider's cleanup responsibility.

Host frame/queue/time budgets are implementation/configuration policy, not negotiated protocol constants. M67's 65536-byte frame limit (including newline), 4 MiB traffic budget and phase deadlines are **runner** limits. Do not advertise them as production host limits. Bounded queues do not establish a universal wall-clock bound for all OS spawn/write/wait operations.

## Handshake and declarations

1. Read `hello` (`protocol`, `host_version`). The supported wire protocol is numeric integer `1`; adapter release version and host release version are not protocol negotiation values.
2. Return `adapter_info` with `protocol`, `id`, nonempty `version` and a nonempty list of unique capabilities. Runtime identity must equal the installed manifest ID.
3. Publish only explicit optional actions/sessions. Begin request/session processing after the handshake; do not invent work from capability names.

Runtime `validate_info` checks protocol, manifest ID, nonempty version and unique nonempty capabilities. It does **not** require SemVer or equality with the manifest version, trim whitespace from the version, enforce nonempty labels, or validate action/session declaration uniqueness and capability references. Distribution release selection uses SemVer separately. Producer guidance is to use meaningful versions/labels, unambiguous action identities, and only advertised capability references; M67 checks references and rejects ambiguous action selection. Do not mistake a decoded declaration for proof that its implementation works.

A minimal valid producer handshake (the capability is illustrative, not mandatory):

```json
{"type":"adapter_info","protocol":1,"id":"example","version":"1.0.0","capabilities":["example.echo"]}
```

## Wire types and portable encoding

The [envelope table](adapter-protocol-v1.md#envelope-types) defines all 15 message types and directions. Every envelope includes `type` and `protocol`. The wire decoder represents protocol as u32; runtime compatibility requires `1`. Do not coerce booleans, strings or fractional numbers into integer fields.

| Field/type | Current constraint |
| --- | --- |
| AdapterId, SessionId | 1–64 ASCII bytes; first `[a-z0-9]`, remainder `[a-z0-9_-]`. |
| Capability, ActionId | One or more dot-separated segments, each following the preceding rule. No separate total-length bound in the identifier constructor; transport budgets still apply. |
| RequestId | 1–128 ASCII bytes from `[A-Za-z0-9._:-]`. |
| Session rows/columns | Wire u16 (0–65535). Producer guidance: use positive geometry; suite profiles require 1–65535. Wire representability alone is not operation admission. |
| Observation timestamp_millis | Optional u64, 0–18446744073709551615; producer Unix epoch milliseconds. |
| Session exit_code | Optional i32, -2147483648–2147483647; absence is not synthetic zero. |
| Metric value | JSON number accepted by current serde_json::Number; no null, string, boolean, NaN or infinity. |
| Payload | Opaque JSON value: object, array, string, number, boolean or null; not necessarily an object. |

### Missing versus null

- Optional `Option` fields accept omission or null: request `action`, error `id`, event `observation`, session `exit_code`, declaration `description`, and optional observation scalar metadata.
- Default vectors (`adapter_info.actions`, `adapter_info.sessions`, error observation `stack`) accept omission as empty, but reject null. Required `capabilities` is an array, not null.
- `confirmation_required` defaults to false on omission; null is invalid.
- Current serde `Value` payload fields decode an omitted payload as null. Producers should nevertheless emit `payload` explicitly, including explicit null, rather than relying on this decoder behavior.
- Required strings remain strings; wire typing alone does not mean all strings must be nonempty. In particular, stream/kind, labels and diagnostic text are not typed identifiers.

### JSON interoperability

Producer guidance: emit unique object keys, valid Unicode scalar text, finite JSON numbers and canonical string enum values. Avoid negative zero for integer fields. The current decoder rejects unknown message/observation variants and recognized duplicate typed fields, while tolerating unknown additive fields and some duplicates in opaque/additive objects. This is not a blanket duplicate-key rejection guarantee. Never use duplicate keys to convey semantics.

Default serde_json number conversion is not arbitrary-precision JSON arithmetic. Python's arbitrary-sized integers and Go's default float64 decoding do not reproduce it automatically. Do not round u64 timestamps through float64. Near floating overflow, even a value another parser considers finite may be rejected by the current Rust codec. Invalid surrogate escapes must be rejected even in values later overwritten by duplicate keys. Although serde accepts some unit-enum object representations, emit severity/status as the documented strings for portability.

The [native parity corpus](../crates/dragonstui-adapter-host/tests/conformance_wire_parity.rs) compares actual Rust and Python acceptance/rejection over 597 cases. It is neither universal codec equivalence nor bit-exact numeric round-trip proof. Changes to serde_json features/dependencies require renewed parity evidence. A general JSON Schema alone would not express all these lexical and runtime rules.

## Request lifecycle and errors

A `request` carries the host correlation ID, declared operation and opaque payload; an action invocation additionally carries its producer action ID. Respond with the **same** ID in `response` or a correlated `error`. Producer guidance: one terminal outcome per accepted request; allow independently executing requests to finish out of order and preserve their identities. Do not use a UI row, currently selected adapter or completion order as ownership.

A global `error` may omit its ID or use null; it is not a completion of every pending request. Error code/message strings have no standardized domain taxonomy. Handle unsupported operations in the provider rather than inferring domain behavior from identifiers. A host timeout does not prove the operation had no side effects: v1 has no general request cancellation, replay, transaction or idempotency-key guarantee. Restart and retry policy must not silently repeat non-idempotent work. Host crash handling resolves pending callers as failures, not successful results.

```json
{"type":"request","protocol":1,"id":"req-1","operation":"example.echo","payload":{"text":"hello"}}
```

```json
{"type":"response","protocol":1,"id":"req-1","payload":{"text":"hello"}}
```

```json
{"type":"error","protocol":1,"id":"req-2","code":"unsupported_operation","message":"Operation is not implemented"}
```

The example error code is provider-defined, not a reserved SDK constant.

## Optional surfaces

**Actions:** declare ordered metadata with ID, label, operation, optional description and confirmation flag. Invocation is ordinary RPC. `confirmation_required` means a distinct UI confirmation before dispatch; it is not authorization, RBAC or provider-side idempotency. An SDK must not infer confirmation from an alarming name. A noninteractive M67 action scenario records explicit `confirmed: true` intent; it does not test UI dialogs.

**Events:** emit `stream`, `kind`, opaque `payload` and optionally a typed observation. The five observation types and enums are defined in the [observation reference](adapter-protocol-v1.md#observability-event-semantics). Missing observation means Generic/Unclassified. Stream/kind/payload keys never select UI semantics. Stdout order is local to one adapter; there is no global order or replay guarantee. Host retained event queues may drop oldest events and expose counters. Do not treat the event stream as a lossless audit log.

**Sessions:** declare capability/label metadata; correlate `session_opened.id` with `session_open.id` and allocate a provider-owned session ID. IDs need be distinct for concurrently active sessions in that provider, not globally across adapters. Output/input are text, not an implicit shell, command parser, binary PTY or terminal-emulation API. Route all follow-ups by the provider session ID and all host ownership by the adapter/session pair. Preserve authoritative `session_exit` independently of droppable output. Reject further input/resize once close is pending; do not treat a successful close write as release evidence. Resize has no wire acknowledgement. Late unclaimed opens may receive cleanup requests; do not assume the UI still wants them. On close, release resources and report terminal state; no invented successful RPC response replaces `session_exit`.

An adapter without these optional surfaces remains valid. Do not require reference fixture action names, an echo session or every observation kind.

## Shutdown, EOF and restart

Producer guidance: on `shutdown`, stop admitting work, release sessions/resources, emit and flush `shutdown_ack`, then finish without trailing protocol output and exit successfully. Treat stdin EOF or host disappearance as a cleanup trigger, including when shutdown was not received. Ensure provider children cannot keep pipes open indefinitely. Do not require another host message after shutdown: the production stop path closes stdin immediately after sending it.

Distinguish enforcement: production `AdapterProcess::stop` waits for child exit and uses kill fallback; it does not prove an acknowledgement was consumed. M67 specifically requires ACK, no trailing output, pipe EOF and zero exit within its budget, and reports forced/failed cleanup separately. The harness cleans its original process group, not escaped descendants; production supervision must not inherit a stronger process-tree guarantee from that harness.

A fresh restart is a new process and handshake. It does not restore requests or sessions automatically. M67 repeats handshake/shutdown on the second process, not the selected application operations. Durable recovery and stress behavior require separate acceptance and are not delivered by this specification.

## Language implementation guidance

These are design requirements for prospective libraries, not APIs or packages available to install today.

| Language | Encoding and type guidance | Lifecycle guidance |
| --- | --- | --- |
| Rust | Existing host protocol types are a source reference; use typed IDs and explicit serde fields/defaults. Do not add a mandatory core-framework dependency. Verify actual serde_json feature behavior before claiming codec parity. | Own and join workers; one bounded frame writer; handle stdin EOF and release owned children. No dynamic-library ABI is required. |
| Go | Use explicit message structs/tag dispatch; distinguish omitted/null/default fields. Use json.Number/RawMessage where needed rather than decoding all payload numbers through float64; validate ranges and Unicode explicitly. Standard decoder acceptance is not v1 parity evidence. | Bound goroutines/queues, serialize and flush stdout; configure any line-reader limit deliberately rather than inheriting an accidental scanner ceiling. Coordinate cancellation and child reaping. |
| Python | Reject bool where integer is required; enforce typed ranges, finite numbers and Unicode. Default json.loads duplicate handling loses evidence; default json.dumps permits nonfinite values unless disabled. M67 parser helpers are test tooling, not a published runtime SDK. | Flush stdout; bound queues/workers; keep diagnostics on stderr. Coordinate EOF, signals and provider-child cleanup without relying on interpreter exit. |

A future reusable SDK should expose explicit handshake metadata, operation dispatch with correlation, optional action/event/session helpers and owned shutdown. It should leave domain policy, secrets, retries, persistence and application authorization to the provider. Public API names and package publication are outside M68.

## Packaging and compatibility

Installable adapters use `adapter.json` and a manifest-relative executable beneath one adapter directory. Discovery validates metadata/path containment without executing the provider. Minimal manifest:

```json
{"id":"example","name":"Example adapter","version":"1.0.0","protocol_version":1,"executable":"example-adapter"}
```

Optional manifest fields are `description`, `homepage` and `author`. The executable must be executable on its target platform; relative paths may not escape the adapter directory. Runtime capabilities come from the handshake, not this manifest. See [distribution integrity and trust limits](adapter-management.md#integrity-boundary) before publishing: a checksum is not publisher authentication or a sandbox.

Within v1, tolerate additive unknown fields in recognized messages but do not assume older hosts understand new message/observation variants. Existing actions/sessions/observations are optional additions, not a general feature-negotiation service. Protocol-number changes require explicit host support; this document does not allocate v2 or silently redefine v1. Record the host revision, platform and scenario profile alongside compatibility claims.

## Acceptance checklist

1. Exercise handshake, declared identity/capabilities, graceful shutdown and fresh start with the [M67 runner](adapter-conformance.md#run-it).
2. Supply adapter-specific profiles for intended operations/actions/events/sessions; inspect skipped checks. Test side effects only with disposable local configuration and trusted executables.
3. Cover provider error paths, correlation under concurrent completion, bounded output pressure, EOF/cancellation and resource release in the provider's own tests. M67 does not implement arbitrary workflow scripting or every failure scenario.
4. For decoder implementations, use the native parity corpus as finite regression evidence, not proof that default language codecs are equivalent.
5. Record executable/source revision, profile, host revision, OS, results and explicit limitations. POSIX M67 evidence does not certify Windows, production integrations, UI rendering, security or recovery under arbitrary load.

Reference commands (repository root):

```sh
python3 tools/adapter_conformance.py --help
python3 -m unittest discover -s tools -p 'test_adapter_conformance*.py'
cargo test -p dragonstui-adapter-host --test conformance_wire_parity -- --nocapture
```

For built reference-provider acceptance, use the exact build/run commands and prerequisites in the [conformance guide](adapter-conformance.md#run-it). No new dependency, runtime implementation or language SDK binary is shipped by M68. M69–M71 stress/recovery/security-limit work remains separate.
