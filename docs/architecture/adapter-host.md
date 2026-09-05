# DragonsTUI Adapter Host

## Decision

M24 keeps the existing root `dragons_tui` package, its public API, examples, and showcase in place. An additive Cargo workspace member, `crates/dragonstui-adapter-host`, owns the adapter-host foundation.

```text
showcase / application (optional adapter integration)
            │
            ├──► dragons_tui framework
            │
            ▼
dragonstui-adapter-host
            │ JSON Lines over child stdin/stdout
            ▼
external adapter executable
```

`dragons_tui` never depends on the host. Framework-only consumers therefore retain the current terminal/UI dependency set and do not pull in serialization, discovery, or process-runtime dependencies.

## Why external processes

Adapters run as ordinary child executables, not as in-process Rust plugins. A child boundary isolates adapter crashes, dependency versions, and language runtimes from both the core framework and the host. The host can supervise, stop, kill, restart, and diagnose each process without letting an adapter crash DragonsTUI.

## Why no Rust dynamic-library ABI

Rust does not provide a stable general-purpose ABI for independently-built dynamic-library plugins. Loading `.dylib`, `.so`, or `.dll` files would additionally couple memory ownership, panic behavior, dependency versions, and security boundaries. M24–M34 deliberately use no `libloading`, dynamic library, Wasm, or embedded-scripting runtime.

## Why capability-driven routing

Application code asks whether an adapter supports an extensible capability identifier such as `containers.logs`; it does not branch on adapter names such as `docker`. This permits multiple future providers to expose the same operation without putting provider-specific policy in the host or UI.

## Why JSON Lines Protocol v1

Protocol v1 sends one JSON object per newline over piped stdin/stdout. It is language independent, human-debuggable, fixture-friendly, and avoids ABI coupling. Adapter stdout is protocol-only; stderr is captured as bounded diagnostics and is never parsed as protocol input. A focused serialization dependency belongs solely to the host crate.

## Scope boundary

The host foundation provides local discovery, supervised child-process communication, handshake compatibility, capabilities, generic RPC, events, bounded queues, and diagnostics. The distribution layer additionally provides provider-neutral registry parsing, bounded SHA-256-verified staging/atomic installation, update/remove coordination, local install provenance, and a plain-terminal management CLI. It does not implement a real Docker, Git, database, process, deployment, log, or Kubernetes adapter.

## Registry metadata

The distribution layer begins with a provider-neutral `Registry` model in the adapter-host crate. It describes validated adapter entries, releases, and exact platform artifacts using normalized `macos`, `linux`, or `windows` operating systems and `aarch64` or `x86_64` architectures. Artifact sources are generic `file://` or HTTPS URLs; no provider such as GitHub is a registry-model dependency. Parsing rejects duplicate adapter IDs, duplicate release versions, duplicate platform artifacts, incompatible protocol releases, invalid checksums, and unsafe/malformed sources before any installation behavior exists.

## Local layout and discovery

`LocalAdapterRoot` scans immediate subdirectories only. Each candidate uses `adapter.json` plus a manifest-relative executable:

```text
adapters/
  example/
    adapter.json
    adapter-executable
```

Discovery never executes an adapter. It reports `Valid`, `InvalidManifest`, `MissingExecutable`, or `UnsupportedProtocol`. IDs must be unique; executable paths must be non-empty relative normal path components, and the resolved executable must remain beneath its adapter directory after canonicalization.

## Installation integrity and trust limits

The adapter-host installer streams `file://` or HTTPS artifact bytes through a configured size cap while computing SHA-256. It writes only to a private staging directory, compares the computed digest and optional expected size with registry metadata, then atomically installs the manifest, executable, and `adapter-install.json` provenance record. Failed validation removes staging output; installation never starts an adapter process.

`adapter-install.json` records the adapter ID/version, optional registry-document source, artifact source, expected SHA-256, and selected platform. SHA-256 establishes **integrity relative to the registry metadata**: it proves the staged bytes matched the declared digest. It does **not** establish publisher identity, registry authenticity, adapter safety, or absence of malicious code. Adapters run with the permissions of the DragonsTUI user; this host provides no sandboxing or publisher-signature verification.

## Runtime lifecycle

The `AdapterManager` supervises many independent `AdapterRuntime` values. Observable states are `Discovered`, `Starting`, `Handshaking`, `Running`, `Stopping`, `Stopped`, `Incompatible`, `Failed`, and `Crashed`.

Starting pipes all three standard streams: stdin is host-to-adapter JSON Lines, stdout is protocol-only JSON Lines, and stderr is a bounded diagnostics tail. The child never inherits DragonsTUI's terminal streams. Stop sends `shutdown`, closes stdin, waits for a bounded grace period, then kills as a fallback. Drop performs the same bounded cleanup.

The host sends `hello` and accepts `adapter_info` only when the negotiated protocol, manifest/runtime identity, version, and unique non-empty capability list are compatible. Timeout, malformed output, exit-before-handshake, mismatch, and duplicate/empty capabilities become failure states; none can make the UI process crash.

## Capabilities, RPC, and events

`Capability` is a validated extensible dotted identifier. `CapabilityRegistry` maps capability to zero or more providers and removes entries on stop, crash, and restart. The manager makes no adapter-name-based routing decisions.

`request` returns immediately with a generated request ID. Application code continues its own event loop and calls `poll`, then retrieves completed outcomes by ID. Responses and adapter errors are correlated independently of arrival order. Unknown response IDs are counted. A crash turns all pending requests into deterministic crash failures.

`event` messages are generic (`stream`, `kind`, JSON `payload`) and are annotated with their emitting adapter. They may additionally carry an optional typed `Observation` (`Log`, `Metric`, `Status`, `Event`, or `Error`), which is explicit producer-declared data semantics rather than a UI choice. Omitted metadata remains Generic/Unclassified; the host does not infer semantics from adapter identity, stream, kind, or payload keys. Per-adapter event order follows that adapter's stdout order. There is intentionally no cross-adapter global ordering guarantee. See [Adapter Protocol v1](../adapter-protocol-v1.md#observability-event-semantics) for the exact additive wire contract and compatibility policy.

Adapter-declared actions use the same controller-owned authority chain as generic RPC: Showcase → authenticated typed controller IPC → `AdapterController` → `AdapterManager` → `AdapterRuntime`. `AdapterInfo.actions` and `Request.action` are optional v1 extensions, so adapters without actions retain their prior lifecycle, capability, RPC, and observability behavior. The Showcase retains at most one pending confirmation record containing the selected adapter/action identity and opaque payload. `confirmation_required` is a producer-declared UI policy, not an authorization system; action-specific permission enforcement does not exist at this boundary. See [Adapter Protocol v1](../adapter-protocol-v1.md#adapter-actions-and-confirmation).

Provider-declared interactive sessions use the same controller-owned authority chain: Showcase → authenticated typed controller IPC → `AdapterController` → `AdapterManager` → `AdapterRuntime`. `AdapterInfo.sessions` is optional v1 metadata; the manager validates an open capability against the selected provider declaration and owns the `(AdapterId, SessionId)` association. The showcase hosts at most one active session, forwards normalized input and rendered-host dimensions through bounded workers, and treats provider output, exit, and disconnect as typed lifecycle events. It neither parses command semantics nor acts as a terminal emulator. Close-pending suppresses additional input and resize requests; retained host output and event queues are bounded. See [Adapter Protocol v1](../adapter-protocol-v1.md#interactive-sessions).

## Backpressure and diagnostics

Decoded stdout ingress uses a bounded synchronous queue. When it is full, the reader blocks and the child pipe applies backpressure; response envelopes are not silently discarded. Runtime and manager event queues are bounded and use **drop-oldest** overflow, exposing capacity, current length, and dropped-event counts. Stderr uses a bounded drop-oldest line tail. This bounds host memory while preserving process isolation and diagnostics.

`AdapterDiagnostics` exposes adapter identity, version/protocol, lifecycle state, pid, uptime, runtime capabilities, last error, stderr tail, pending request count, and event-queue counters. The authenticated loopback controller IPC exposes a serializable diagnostics snapshot to optional host consumers without exposing its endpoint credential. The feature-gated showcase renders this snapshot in its responsive adapter inspector and leaves unavailable runtime fields as `--`; discovery itself still never starts an adapter.
