# Adapter Distribution and Management

## Scope

Adapter distribution and lifecycle management live in the independent `dragonstui-adapter-host` workspace crate. The core `dragons_tui` framework has no mandatory registry, HTTP, checksum, installer, or CLI dependency. The showcase connects to the host only behind its optional `adapter-showcase` feature.

No Docker, Git, PostgreSQL, Kubernetes, process, port, logs, database, or other domain adapter is included in this repository.

## Registry and installation

A provider-neutral `Registry` document contains adapter entries, releases, and exact `os`/`architecture` artifacts. The supported normalized platform names are `macos`, `linux`, and `windows`, with `aarch64` and `x86_64` architectures. Artifact sources are local `file://` paths or HTTPS URLs; the model does not depend on GitHub.

Installation selects an explicit requested version or the latest compatible SemVer release, then selects an exact platform artifact. It streams bytes through a configurable cap (default 64 MiB), computes SHA-256 while writing private staging output, validates expected size and digest, and atomically installs the discoverable manifest/executable layout. Partial or failed staging does not become an installed adapter.

**Install does not start an adapter.** Execution requires a separate explicit lifecycle command.

`adapter-install.json` records installed version, selected platform, artifact source, optional registry source, and expected SHA-256.

## Integrity boundary

SHA-256 verifies that downloaded bytes match the registry metadata. It does **not** prove publisher identity, registry authenticity, adapter safety, or absence of malicious code. There is no publisher-signature verification or sandboxing. Adapters execute as processes with the permissions of the user running DragonsTUI.

## Update and remove

Update stages and validates the replacement before atomic installation, preserving the existing installed version when download or verification fails. Update and remove first unregister controller-owned runtime state, so they stop a running adapter and clear lifecycle, capability, and diagnostic state. Remove is limited to a direct adapter directory beneath the configured local root.

## CLI

`dragonstui-adapter` is a plain-terminal management binary; it never enters alternate-screen mode.

```text
dragonstui-adapter search [query] --registry <path-or-https-url>
dragonstui-adapter list
dragonstui-adapter info <id>
dragonstui-adapter install <id> --registry <path-or-https-url> [--version <semver>]
dragonstui-adapter update <id> --registry <path-or-https-url>
dragonstui-adapter remove <id> --yes
dragonstui-adapter start <id>
dragonstui-adapter stop <id>
dragonstui-adapter restart <id>
```

Lifecycle calls use a controller daemon with authenticated loopback JSON Lines IPC. Its token is stored only in a private endpoint file and is not put in command-line arguments or CLI output.

## Showcase adapter section

Build the optional showcase integration with:

```text
cargo run --features adapter-showcase --bin dragonstui-showcase -- --adapter-root <local-adapter-root>
```

Section **8 — Adapters** is available from keyboard, visible header tab, and command palette in English and Turkish. Discovery is metadata-only and never starts an adapter. Valid discovered entries display as Stopped; unsupported protocol entries display as Incompatible; invalid discovery states remain explicit.

The responsive inspector presents selected adapter metadata: ID, name, installed version, protocol, discovery state, executable path, and discovery error. When a local authenticated controller has runtime state for the adapter, it also shows runtime version, state, PID, uptime, capabilities, pending requests, event queue usage, dropped events, last error, and stderr diagnostic tail. Runtime fields display `--` when unavailable; no runtime metric is fabricated.

The optional TUI section provides registry-backed install/update selection, explicit start/stop/restart actions, and confirmed remove actions. Runtime lifecycle and diagnostics use `ControllerManagementClient` over authenticated local controller IPC; the TUI does not create an in-process runtime authority. Install, update, and remove retain their existing host-side filesystem/transaction boundaries.

Section 8 also provides a Capability Browser. `C` switches between adapter and capability browsing; capability rows show an opaque runtime capability identifier and provider count, while the selected row shows provider ID, display name, and current diagnostics state. The browser rebuilds from the existing discovered-adapter rows and typed controller diagnostics snapshots, so it represents only adapters currently reporting capabilities at runtime. It does not merge manifest declarations with runtime data, add RPC calls or polling, invoke a capability, or implement live-data streaming.

Adapter distribution and management is complete through M43. The next phase begins with M44 Background Tasks and Live Data Channels.
