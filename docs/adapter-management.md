# Adapter Distribution and Management

## Scope

Adapter distribution and lifecycle management live in the independent `dragonstui-adapter-host` workspace crate. The core `dragons_tui` framework has no mandatory registry, HTTP, checksum, installer, or CLI dependency. The showcase connects to the host only behind its optional `adapter-showcase` feature.

The optional [Docker adapter](docker-adapter.md) is included as a separately installed Python executable in the source checkout, not in the four native binary bundles. No Git, PostgreSQL, Kubernetes, process, port or database adapter is bundled.

## Registry and installation

A provider-neutral `Registry` document contains adapter entries, releases, and exact `os`/`architecture` artifacts. The supported normalized platform names are `macos`, `linux`, and `windows`, with `aarch64` and `x86_64` architectures. Artifact sources are local `file://` paths or HTTPS URLs; the model does not depend on GitHub.

Installation selects an explicit requested version or the highest parseable SemVer release, then requires an exact platform artifact in that release. It does not fall back to an older release for platform compatibility or filter releases by protocol compatibility. It streams bytes through a configurable cap (default 64 MiB), computes SHA-256 while writing non-discoverable staging output, validates expected size and digest, and publishes the initial manifest/executable directory with a rename. Staging permissions follow the local filesystem creation policy; hidden staging is not a permission sandbox. Partial or failed verification does not become an installed adapter.

**Install does not start an adapter.** Execution requires a separate explicit lifecycle command.

`adapter-install.json` records installed version, selected platform, artifact source, optional registry source, and expected SHA-256.

## Integrity boundary

SHA-256 verifies that downloaded bytes match the registry metadata. It does **not** prove publisher identity, registry authenticity, adapter safety, or absence of malicious code. There is no publisher-signature verification or sandboxing. Adapters execute as processes with the permissions of the user running DragonsTUI.

## Update and remove

Update selects the highest parseable SemVer release newer than the installed version, then requires its exact platform artifact. Both the CLI and embedding management coordinator stage and validate the replacement before unregistering runtime state, preserving the installed version and running provider on download or verification failure. Update and remove unregister controller-owned state before filesystem mutation. CLI maintenance aborts on controller status/authentication/transport errors or unexpected responses; only an absent endpoint or an explicit missing-state reply permits proceeding without unregister.

Replacement uses two renames (installed directory to backup, then staging to installed directory), with best-effort rollback if the second rename fails. It is not a single atomic exchange, crash-durable transaction or cross-client maintenance lock. CLI update leaves the adapter stopped after unregister, including a commit failure; restart is explicit. The embedding `AdapterManagement` coordinator separately attempts to restore a previously running adapter after commit or commit failure. Remove is limited to a direct adapter directory beneath the configured local root.

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

Start/stop/restart use `ControllerManagementClient` and the authoritative daemon over authenticated loopback JSON Lines IPC. All CLI endpoint-file paths validate a loopback address before connecting. The token is bootstrapped through `DRAGONSTUI_CONTROLLER_TOKEN` and retained in daemon memory/environment and the endpoint file, not command-line arguments or CLI output. Provider launches explicitly remove that reserved environment key, including configured overrides. This is not isolation from same-user filesystem access.

On Unix, endpoint temporary files are created exclusively with mode `0600` before credential bytes are written, then renamed into place. Non-Unix permission behavior follows the operating system's creation policy; no Windows ACL guarantee is claimed. A failed probe of an existing controller preserves its endpoint and fails rather than spawning a competing daemon. If the endpoint is genuinely stale, confirm the original daemon has stopped before explicitly removing that local state; never remove it merely because an authentication error or timeout occurred.

## Showcase adapter section

Build the optional showcase integration with:

```text
cargo run --features adapter-showcase --bin dragonstui-showcase -- --adapter-root <local-adapter-root>
```

Section **8 — Adapters** is available from keyboard, visible header tab, and command palette in English and Turkish. Discovery is metadata-only and never starts an adapter. Valid discovered entries display as Stopped; unsupported protocol entries display as Incompatible; invalid discovery states remain explicit.

The responsive inspector presents selected adapter metadata: ID, name, installed version, protocol, discovery state, executable path, and discovery error. When a local authenticated controller has runtime state for the adapter, it also shows runtime version, state, PID, uptime, capabilities, pending requests, event queue usage, dropped events, last error, and stderr diagnostic tail. Runtime fields display `--` when unavailable; no runtime metric is fabricated.

The optional TUI section provides registry-backed install/update selection, explicit start/stop/restart actions, and confirmed remove actions. Runtime lifecycle and diagnostics use `ControllerManagementClient` over authenticated local controller IPC; the TUI does not create an in-process runtime authority. Install, update, and remove retain their existing host-side filesystem/transaction boundaries.

Section 8 also provides a Capability Browser. `C` switches between adapter and capability browsing; capability rows show an opaque runtime capability identifier and provider count, while the selected row shows provider ID, display name, and current diagnostics state. The browser rebuilds from the existing discovered-adapter rows and typed controller diagnostics snapshots, so it represents only adapters currently reporting capabilities at runtime. It does not merge manifest declarations with runtime data, add RPC calls or polling, invoke a capability, or implement live-data streaming.

Adapter distribution and management is complete through M43. Subsequent layers add bounded generic live history/search/follow (M44–M47), inspector primitives (M48–M52), producer-declared observability projections (M53–M58), actions/operations/notifications (M59–M62), and developer-tooling views including interactive hosting (M63–M65). The [reference mock adapter](reference-mock-adapter.md) combines these adapter contracts without requiring a domain service. It does not turn fixture echo sessions into a shell or add action-specific authorization.
