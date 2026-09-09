# DragonsTUI

![DragonsTUI banner](assets/lastbanner.png)

**English** | [Türkçe](README.tr.md)

An explicit immediate-mode Rust terminal UI framework with a process-isolated, capability-driven adapter host for interactive developer tooling.

[![CI](https://github.com/frknaykc/dragonstui/actions/workflows/ci.yml/badge.svg)](https://github.com/frknaykc/dragonstui/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/frknaykc/dragonstui)](LICENSE)
[![Rust 2024](https://img.shields.io/badge/Rust-2024-dea584?logo=rust)](Cargo.toml)
[![GitHub stars](https://img.shields.io/github/stars/frknaykc/dragonstui?style=flat)](https://github.com/frknaykc/dragonstui/stargazers)


DragonsTUI is for terminal applications that need direct control over rendering, state, input routing, and terminal output. The core framework stays dependency-light and explicit; the optional adapter host adds supervised external processes, generic capabilities, diagnostics, and local management tooling without embedding domain-specific integrations into the UI engine.

## Why DragonsTUI?

Most terminal applications eventually need application-specific layout, focus, event routing, and redraw policy. DragonsTUI keeps those decisions visible:

```text
application state → layout → explicit primitive rendering → Frame → Buffer → diff → terminal
```

There is no retained component tree, virtual DOM, automatic event bubbling, or framework-owned application state. Applications compose primitives and own the state that drives them.

## Features

- Explicit immediate-mode rendering with frame-buffer diffing and ANSI terminal output.
- Unicode-width-aware text plus grapheme-aware text input and multiline editing.
- Layout, panels, rich text, lists, tables, trees, viewports, focus state, mouse input, modals, and a command palette.
- Canvas, animation, spinners, progress bars, gauges, sparklines, and a Dragonfire showcase theme.
- A process-isolated adapter host with JSON Lines protocol v1, handshakes, generic RPC, events, bounded queues, and diagnostics.
- Provider-neutral adapter registry metadata and SHA-256-verified, atomically staged installation.
- Authenticated local controller daemon with typed management IPC for runtime start, stop, restart, and diagnostics.
- An optional adapter-aware showcase that keeps core framework consumers free of adapter-management dependencies.
- Generic semantic observability projections for producer-declared Logs, Metrics, Status, Events, and Errors.
- Adapter conformance tooling, multi-adapter stress coverage, explicit crash recovery, and enforced protocol/resource limits.

## Screenshots

### Opening splash

![DragonsTUI loading splash](assets/dragonstui-loading.png)

### Main showcase

![DragonsTUI showcase Overview](assets/dragonstui-showcase.png)

The Overview screen appears after the splash transition.

### Widgets

![DragonsTUI widgets, table, tree and viewport](assets/dragonstui-widgets.png)

These screenshots were refreshed on September 9, 2026 from the running `dragonstui-showcase` release binary in macOS Terminal at 160 × 48 cells, including the corrected opening-title alignment. They show local fixture/demo data, not live provider telemetry. Terminal fonts and colors can differ on other systems.

## Quick Start

New here? Follow the [first 10 minutes user guide](docs/user-guide.md) to learn
which application to open, navigate the showcase, and add a trusted adapter.

For packaged binaries, start with [Installation and first run](docs/installation.md):
user-local installation, shared adapter paths, empty states, update and uninstall.
Public release assets are not published yet; current packages are CI artifacts.

DragonsTUI declares Rust edition 2024 and does not currently declare an MSRV. Use a Rust toolchain that supports edition 2024.

```sh
git clone https://github.com/frknaykc/dragonstui.git
cd dragonstui
cargo build
cargo run --release
```

`cargo run --release` starts the default dashboard (`dragons_tui`), not the screenshot showcase. To open the interface pictured above, use the feature-gated command below.

The repository also includes focused examples under [`examples/`](examples/): direct rendering, layout, input, tables, animation, and Braille canvas drawing.

## Showcase

The `dragonstui-showcase` binary demonstrates the framework primitives and the optional adapter-aware interface:

```sh
cargo run --release --features adapter-showcase --bin dragonstui-showcase
```

To inspect a local adapter root without executing discovered adapters:

```sh
cargo run --release --features adapter-showcase --bin dragonstui-showcase -- --adapter-root <path>
```

Controls verified in the running showcase:

- `Enter` or `Space` continues from the opening splash.
- `1`–`8` select sections; visible header tabs also support mouse selection.
- `Tab` moves focus; arrow keys navigate or edit the focused primitive.
- `Ctrl+P` opens the command palette; `m` opens the modal.
- `q` or `Ctrl+C` exits and restores the terminal.

## Architecture

The core framework is independent of the adapter ecosystem. Its optional application integration follows this runtime path:

```text
showcase / application
        ↓
ControllerManagementClient
        ↓ authenticated local IPC
controller daemon
        ↓
AdapterManager
        ↓
adapter process
```

The controller daemon is the runtime lifecycle authority. The showcase uses the typed client for start, stop, restart, and diagnostics rather than creating an in-process runtime manager. Installer, update, and remove operations retain their host-side filesystem and transaction boundaries.

## Adapter Ecosystem

`dragonstui-adapter-host` runs adapters as supervised external processes rather than in-process plugins. The child boundary isolates crashes, dependencies, and language runtimes from the framework and controller.

Adapters describe generic capabilities through protocol v1. Names such as `containers.logs` are capability examples, not built-in integrations. The optional [Docker adapter](docs/docker-adapter.md) provides real container data, confirmed lifecycle actions and a target-bound text shell as a separate Python executable. Install it explicitly from a reviewed checkout; the four native binary bundles do not install domain providers. There is no bundled Git, PostgreSQL, Kubernetes, process, port or database adapter. The Section 8 Capability Browser groups live controller diagnostics by opaque capability contract and lists the adapters currently reporting each contract; it does not invoke capabilities or consume their data.

The bundled **reference mock adapter** exercises RPC, observability, actions and interactive echo sessions without Docker, Git or external services. It is a fixture provider, not a real shell or domain adapter. See the [reference mock guide](docs/reference-mock-adapter.md) for isolated setup and end-to-end acceptance.

External developers can implement the existing contract using the [Adapter SDK Specification](docs/adapter-sdk-specification.md), including Rust/Go/Python portability guidance, and run explicitly selected protocol/lifecycle scenarios with the POSIX [Adapter Conformance Suite](docs/adapter-conformance.md). Unrequested surfaces are reported as skipped; a passing scenario report is not a sandbox, security certificate or complete adapter certification.

The [multi-adapter stress harness](docs/adapter-stress-testing.md) checks bounded event overflow and correlated RPC under local mock load, with opt-in release CPU/RSS measurements.

[Crash and recovery hardening](docs/adapter-crash-recovery.md) covers terminal failure classification, stable diagnostics, backpressure and explicit recovery with healthy-peer isolation. It adds no automatic restart policy or general process-tree containment guarantee.

[Adapter host limits](docs/adapter-limits.md) document bounded wire/manifest reads, stream-rate termination, request admission, timeout and executable-path budgets, including compatibility and enforcement limits.

[Adapter host performance](docs/adapter-performance.md) provides an opt-in release measurement matrix for serialization, RPC, streaming and scheduling, with local raw results and explicit measurement limits.

[Adapter ecosystem showcase](docs/adapter-ecosystem-showcase.md) follows an isolated registry CLI install through real TUI observations, actions, provider crash, explicit restart and diagnostics, with reconstructed PTY frames and reproducible acceptance commands.

[Adapter release readiness](docs/adapter-release-readiness.md) records the M74 source audit, controller/CLI boundary fixes, local package verification and remaining release/platform limits. It is not a public release announcement.

[Release packaging](docs/release-packaging.md) defines the v0.1.0 macOS ARM64/Linux x86_64 bundles, SHA-256 verification, isolated package smoke tests and tag-gated GitHub Release workflow. R1 prepares and tests the pipeline without publishing; the first public release remains an R6 action.

Read the details:

- [Adapter host architecture](docs/architecture/adapter-host.md)
- [Adapter protocol v1](docs/adapter-protocol-v1.md)
- [Adapter distribution and management](docs/adapter-management.md)

## Project Status

DragonsTUI is under active development and remains pre-1.0. The core framework, adapter-host foundations, distribution, observability, actions and developer-tooling views are implemented. The reference mock, conformance suite and SDK specification are complete through M68; M69–M71 add stress coverage, crash recovery and protocol/resource limits. The SDK specification does not include published language SDKs.

| Area | Status |
| --- | --- |
| Framework foundation | Complete |
| Adapter host foundation | Complete |
| Distribution and management | Complete (M35–M43) |
| Generic live data | Complete (M44–M47) |
| Generic inspector UX | Complete (M48–M52) |
| Observability | Complete (M53–M58) |
| Adapter actions | Complete (M59–M62) |
| Developer tooling views | Complete (M63–M65) |
| Reference mock adapter | Complete (M66; locally verified) |
| Adapter conformance suite | Complete (M67; locally verified) |
| SDK specification | Complete (M68; specification only, no published language SDKs) |
| Multi-adapter stress testing | Complete (M69; release measurement matrix, local gates and remote CI passed) |
| Crash and recovery hardening | Complete (M70; local recovery regressions and required gates verified) |
| Protocol and security limits | Complete (M71; local limits regressions, required gates and independent review verified) |
| Adapter host performance | Complete (M72; release baseline, local gates and independent review verified; no production optimization claimed) |

Adapter distribution and management includes registry/install/update/remove integrity boundaries, CLI and TUI management, typed authenticated controller IPC, per-adapter lifecycle conflict protection, real PTY acceptance, and M43 capability discovery. Generic live data transports adapter events away from the UI thread into bounded retained history, derives opaque text and identity filters, and supports pause/follow selection without stopping ingestion. Generic Inspector UX provides reusable layout, viewport, property, and structured-data primitives. The optional showcase projects only producer-declared `Observation` variants into a Log Viewer, time-series graph, heatmap, status matrix, Timeline, and Error/Stack Trace view; it never derives those classes from arbitrary payload JSON, stream, or `kind` text. Each projection is rebuilt from the retained 16-entry live history, so it does not create an unbounded telemetry store. M59–M60 add producer-declared generic action metadata and confirmation policy through the authenticated controller path; confirmation is UI protection against accidental dispatch, not a permission system.

## Development

Run the workspace checks before opening a change:

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cargo check --features adapter-showcase --bin dragonstui-showcase
cargo test --features adapter-showcase --bin dragonstui-showcase
cargo clippy --features adapter-showcase --bin dragonstui-showcase -- -D warnings
```

Additional technical notes cover the [immediate-mode decision](docs/architecture/component-model.md), [public API](docs/public-api.md), [performance measurements](docs/performance.md), and [terminal compatibility](docs/terminal-compatibility.md).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) for issue expectations, focused PR guidance, local checks, and adapter protocol compatibility requirements.

## License

Licensed under the [MIT License](LICENSE).