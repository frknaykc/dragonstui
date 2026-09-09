# Adapter Host Release Readiness (M74)

## Status and scope

**LOCALLY VERIFIED — not a public release approval.** The audited base is `47eee26057e5eb1dd4480e3d34bec2c8396b2cf6` (M73). M74 applies approved narrow corrections to protocol documentation, controller/CLI authority boundaries and packaging. The [evidence record](adapter-release-evidence.json) identifies verified source and release binaries. No commit, final-snapshot remote CI, public upload, third-party certification or new SDK package is claimed.

## Evidence matrix

| Area | Evidence | Result / limit |
| --- | --- | --- |
| Workspace | Fresh local format/check/test/clippy gates | 423 Rust tests passed, zero failed, one opt-in performance benchmark ignored. M73 [remote CI 34358699831](https://github.com/frknaykc/dragonstui/actions/runs/34358699831) remains historical evidence. |
| Optional host boundary | Root manifest and default dependency tree | Host remains optional; core JSON support is still mandatory for the structured-data primitive. Architecture prose corrected. |
| Protocol / SDK | [Protocol](adapter-protocol-v1.md), [SDK specification](adapter-sdk-specification.md), [conformance](adapter-conformance.md) | Payload-omission documentation corrected against native parity: 597 cases, zero mismatches. Session-ID retention, post-handshake profile exit codes and first-cycle declaration checks clarified. No decoder changes or published SDK packages. |
| CLI / installer / controller | [Management](adapter-management.md), [limits](adapter-limits.md), fresh integration tests | Approved fixes below verified; independent CLI/process/installer/package source review found no confirmed blocker. SHA-256 establishes relative artifact integrity, not publisher identity. |
| TUI and sessions | Fresh M74 conformance and ecosystem `q` / Ctrl+C runs | Rebuilt release binaries passed real CLI installation, observations/actions, provider crash, explicit TUI restart, diagnostics, session termination and terminal/process cleanup. This does not verify TUI install/update/remove selectors. |
| Performance | [M72 baseline](adapter-performance.md) | Historical measurements, not measurements of the changed transport/process/installer snapshot. No speedup or new timing baseline claimed. |
| Packaging | Offline workspace package creation and compilation, default and `adapter-showcase` | Both archives verified; README/LICENSE inventory and local-state exclusion checked. No registry upload, global install or distribution-platform certification. |

## Boundary corrections

- **Controller transport:** separate 64 MiB encoded JSON budget in both directions; absolute five-second exchange deadline; socket waits use at most 10 ms slices with continued controller polling. Authentication follows bounded decoding. Synchronous command execution, polling and JSON parsing are not preempted; this is not a fair concurrent dispatcher or total-memory guarantee. Drain-style responses are not replayed automatically after an encoding/delivery error. See [precise IPC limits](adapter-limits.md#m74-controller-ipc-transport).
- **CLI endpoint consistency:** reject non-loopback endpoints before connecting. Lifecycle uses `ControllerManagementClient`; inspection uses the diagnostics facade. An existing endpoint probe failure returns an error without deleting it or spawning a replacement daemon.
- **Maintenance:** propagate status/unregister failures before installed-file mutation. When an endpoint exists, only an authoritative missing-state response permits proceeding without unregister; otherwise unregister must explicitly complete. Update verifies and stages the replacement before stopping the old runtime. Expanded tests cover registry/checksum failures, failed authenticated maintenance, endpoint preservation and successful running update.
- **Credential delivery:** Unix endpoint temporary files use exclusive creation with `0600` before writing token bytes. Provider launch removes the reserved daemon bootstrap variable after configured environment overrides. Bootstrap still uses environment rather than argv; this does not isolate same-user processes or guarantee non-Unix ACLs.
- **Installer claims:** document two-renames/best-effort rollback, ordinary staging permissions and highest-SemVer-before-compatibility-check behavior. Do not promise power-loss atomicity, guaranteed rollback, owner-only staging or automatic older-compatible fallback. Concurrent maintenance and initial daemon-election locking remain outside the guarantee.

## Packaging preflight

Run without publication or registry credentials:

```sh
cargo metadata --no-deps --format-version 1
cargo tree -p dragons_tui --edges normal
cargo package --workspace --offline --allow-dirty
cargo package --workspace --features adapter-showcase --offline --allow-dirty
```

The initial root-only preflight failed because the optional path dependency lacked a version requirement. It now declares host version `0.1.0`; the host package includes README and the same MIT license/copyright text as the root. Cargo's temporary workspace registry verified the optional dependency without claiming crates.io availability. `--allow-dirty` includes intended pending changes; it does not upload anything. Public publication still needs dependency-first registry availability and the chosen distribution target's install path.

## Fresh verification

All required Rust format/check/clippy gates passed after the material source changes; the feature-enabled workspace suite passed **423 tests**, with one opt-in timing workload ignored. This includes all seven expanded CLI and two process tests plus shared IPC integration. The latter resolves the implementation worker's intermediate duplicate-import compile conflict. Independent read-only CLI/process/installer/package review found no confirmed acceptance blocker; main-agent IPC review checked codecs, deadline accounting, continued polling and connection-local failure handling against the limitations above.

The existing Python suite passed **95 tests**. Two new deterministic baseline-PTY tests passed after proving and fixing a negative `select()` timeout at deadline crossing. No wait was increased. The original helper did not retain its failure screen; its traceback and deterministic RED result identify a harness clock-arithmetic failure, not an application-rendering failure. Both baseline PTY exit routes then passed.

Fresh release controller/mock/showcase binaries passed reference conformance and both ecosystem PTY exits (`q`, Ctrl+C), including authoritative fixture state and terminal/process cleanup. Both offline workspace package modes compiled successfully. Source hashes, binary identities and selected acceptance results are in the evidence record.

## Explicit non-claims and delivery boundary

- M74 source acceptance is locally verified; Phase 11 is not declared closed. Commit, remote CI and public release are separate delivery steps.
- Registry OS/architecture labels do not establish Windows support or named-terminal acceptance; see [terminal compatibility](terminal-compatibility.md). Local POSIX PTY checks and reconstructed historical M73 frames are not emulator screenshots or general Unicode proof.
- M73/M74 install a fixture-owned launcher through the real CLI, not a portable production provider package. Provenance acceptance checks file existence, not every field independently.
- Process isolation is not a sandbox, publisher-signature verification, action-specific RBAC or process-tree containment. Same-user filesystem access remains possible.
- Codec/deadline unit tests and ordinary integration are not adversarial socket timing/load tests or a hard real-time guarantee. Input/queue budgets are not total CPU/RAM limits or universally cancellable stdin writes; see [crash/recovery](adapter-crash-recovery.md).
- No external dependency-advisory audit or independent published-crate availability verification was performed. Prior green CI is not a security certification. Historical M72 timings and committed M73 frames were not rewritten.
