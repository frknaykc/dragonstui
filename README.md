# DragonsTUI

![DragonsTUI banner](assets/lastbanner.png)

**English** | [Türkçe](README.tr.md)

[![CI](https://github.com/frknaykc/dragonstui/actions/workflows/ci.yml/badge.svg)](https://github.com/frknaykc/dragonstui/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/frknaykc/dragonstui)](LICENSE)
[![Rust 2024](https://img.shields.io/badge/Rust-2024-dea584?logo=rust)](Cargo.toml)
[![GitHub stars](https://img.shields.io/github/stars/frknaykc/dragonstui?style=flat)](https://github.com/frknaykc/dragonstui/stargazers)

DragonsTUI is an open-source **immediate-mode Rust terminal UI framework** for building interactive terminal applications where rendering, layout, focus, and input routing stay explicit in the application.
The core crate is `dragons_tui` (`package name: dragons_tui`, `version 0.1.0`, MIT).
The optional `dragonstui-adapter-host` crate adds **process-isolated adapter hosting**, local registry/installer flows, and an authenticated controller surface for external providers.

## Category and core problem

DragonsTUI is for terminal application developers who need:

- deterministic redraw behavior,
- explicit ownership of UI state,
- bounded terminal interaction policy,
- and optional external capability access without embedding those providers into the core UI engine.

It is not a backend service and does not require a network to run the core dashboard.

## What is included

DragonsTUI has two related parts:

1. **Core framework (`dragons_tui`)**
   - Rendering pipeline: `application state → layout → explicit render → Frame → Buffer → diff → terminal`.
   - No retained component tree, no `Widget` trait, no automatic event bubbling, and no framework-owned application state.
   - Primitives for text, panel layout, lists/tables/trees, editors, overlays, focus, and simple visual widgets.

2. **Optional adapter host workspace (`dragonstui-adapter-host`)**
   - Newline-delimited JSON protocol v1 over process stdin/stdout.
   - Local installer and CLI (`dragonstui-adapter`) for discover/install/update/remove/start/stop/restart.
   - Authenticated local controller daemon for lifecycle and diagnostics.
   - Reference mock (`dragonstui-adapter-host-mock`) used only as a fixture provider.

## In short (project identity)

If you need the key identity in one read:

- Project name: **DragonsTUI**.
- Category: **Immediate-mode terminal UI framework + optional adapter host runtime**.
- Primary users: **Rust developers building terminal UIs with explicit control**.
- Solves: **UI composition/own-state complexity and optional capability integration without hard-wiring providers into the core engine**.
- Core install path: source checkout with `cargo`, or native release archives.
- Self-hosting: yes (local binaries and local source build).
- API model: library API, CLI, and binaries.
- Not included by default: no MCP server and no implicit provider sandbox.

## Screenshots (non-authoritative; images are illustrative)

### Opening splash

![DragonsTUI loading splash screen shown at startup in a terminal UI](assets/dragonstui-loading.png)

### Showcase overview

![DragonsTUI showcase overview panel with tabs and command controls in a terminal UI](assets/dragonstui-showcase.png)

### Widgets and panels

![DragonsTUI widget demo showing text, list, table, tree, and viewport controls](assets/dragonstui-widgets.png)

These screenshots show demo data and local fixtures. They are not live provider telemetry.

## Quick Start

### 1) Build and run from source

```sh
git clone https://github.com/frknaykc/dragonstui.git
cd dragonstui
cargo build

# default dashboard
cargo run --release
```

`cargo run --release` runs the `dragons_tui` dashboard.

```sh
# adapter-aware demo
cargo run --release --features adapter-showcase --bin dragonstui-showcase
```

### 2) Use optional adapter root with source build

```sh
mkdir -p "$HOME/.local/share/dragonstui/adapters"
cargo run --release --features adapter-showcase --bin dragonstui-showcase -- --adapter-root "$HOME/.local/share/dragonstui/adapters"
```

If the root is empty, `dragonstui-showcase` shows an empty Adapters section by design.

### 3) Minimal adapter CLI check (evidence-style example)

```sh
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" list
```

Expected result pattern: command output shows adapter list columns and no rows on an empty root; stderr provides first-run guidance for creating/using a root and installing providers.

### 4) Release binaries (v0.1.0)

Current documented packages:

| Platform | Archive |
| --- | --- |
| Apple Silicon macOS | `dragonstui-v0.1.0-macos-arm64.tar.gz` |
| Linux x86_64 (GNU libc) | `dragonstui-v0.1.0-linux-x86_64.tar.gz` |

SHA-256 verification is checksum-based integrity only, not publisher signing.

```sh
# macOS example
mkdir -p dragonstui-0.1.0-macos-arm64
shasum -a 256 -c dragonstui-v0.1.0-macos-arm64.tar.gz.sha256
tar -xzf dragonstui-v0.1.0-macos-arm64.tar.gz -C dragonstui-0.1.0-macos-arm64
cd dragonstui-0.1.0-macos-arm64
```

See [Installation and first run](docs/installation.md) for full checksum commands, PATH setup, update, uninstall, and troubleshooting.

## Interfaces and usage paths

### Library API (`dragons_tui`)

The framework is consumed as a Rust library from `dragons_tui::*`.
See [docs/public-api.md](docs/public-api.md) for the current public surface.
Examples in `examples/` are runnable and cover explicit rendering, layout, input, table, and canvas flows.

### CLI (`dragonstui-adapter`)

Management commands are text and plain terminal oriented:

```text
dragonstui-adapter search [query] --registry <source>
dragonstui-adapter list
dragonstui-adapter info <id>
dragonstui-adapter install <id> --registry <source> [--version <semver>]
dragonstui-adapter update <id> --registry <source>
dragonstui-adapter remove <id> --yes
dragonstui-adapter start <id>
dragonstui-adapter stop <id>
dragonstui-adapter restart <id>
```

See [docs/adapter-management.md](docs/adapter-management.md) for the exact command behavior and lifecycle details.

### Binaries

- `dragons_tui` (default dashboard)
- `dragonstui-showcase` (UI demo + optional adapter section)
- `dragonstui-adapter` (plain-terminal CLI)
- `dragonstui-adapter-host-mock` (protocol fixture provider)

### MCP and other external protocol support

The repository documentation does not define an MCP (Model Context Protocol) server or MCP toolset. Protocol integration is through the DragonsTUI adapter protocol v1 in `dragonstui-adapter-host`.

## How it works

### Core framework rendering path

```
application state
  └─ layout calculation
      └─ primitive.render(...)
          └─ Frame
              └─ Buffer
                  └─ diff(previous, current)
                      └─ runtime/terminal output
```

The application owns layout, focus order, event routing, overlay priority, redraw policy, and terminal cursor behavior.

### Optional adapter flow

```text
showcase/CLI
   -> ControllerManagementClient (authenticated loopback IPC)
      -> local controller daemon
         -> adapter process (protocol v1 over stdin/stdout)
```

Install actions do not start adapters. Start/restart/stop/diagnostics are explicit.

## Use cases

- Build terminal dashboards or inspectors with precise screen control.
- Render structured terminal UI elements (lists, tables, trees, viewported panels).
- Add optional provider-based capabilities via adapter registry and runtime lifecycle controls.
- Run protocol conformance and lifecycle experiments using the reference mock provider.
- Inspect provider logs, metrics, status, timeline/error streams through the showcase inspector views.

## Requirements and compatibility

### Required for all users

- Rust toolchain supporting **edition 2024** (no MSRV is declared).
- Interactive terminal with ANSI support, cursor and alternate-screen behavior, and Unicode-capable input/output for full feature operation.
- 24-bit SGR color output is used for Dragonfire theme values; there is no palette fallback.

### Supported installation targets

- macOS release archives: Apple Silicon (`aarch64`) only.
- Linux release archives: `x86_64` GNU libc 2.35 compatible environment.
- Core framework is built from source and does not hard-code a required OS in library logic, but release bundles are not listed for Windows / Intel macOS / Linux ARM64.
- Terminal details and fallback behavior are documented in [docs/terminal-compatibility.md](docs/terminal-compatibility.md).

### Optional adapter requirements

- **Docker adapter** (separate domain adapter): Python 3.10+ and Docker CLI/Engine.
- **Non-interactive agent adapters** and optional provider docs have separate runtime requirements in their own guides.

## Configuration, data, and credentials

- Core dashboard and showcase do **not** use a persisted UI config file.
- `dragonstui-showcase` does not read adapter root unless `--adapter-root` is provided.
- `dragonstui-adapter` defaults CLI root to `./adapters` in current working directory.
- The controller writes local endpoint state (including tokens) under `<root>/.controller/endpoint.json`; tokens are private local credentials.
- Do not print, share, or pass endpoint tokens via issue reports.

## Limitations (verified from docs and code)

- No trusted sandbox model for adapters; provider processes run with user permissions.
- No automatic restart policy in core flow.
- No automatic provider start on showcase install.
- No GUI/web runtime and no terminal database/emoji/font negotiation layer.
- OSC 8 hyperlinks are deferred; no `TerminalCapabilities` API yet.
- No automatic update/daemon shutdown command for the controller.
- No MCP server documented in repository.
- Release bundles include only four binaries and the repository does not include domain adapters by default (no bundled Git/PostgreSQL/Kubernetes/port/db adapters).

## When DragonsTUI is a good fit

Choose DragonsTUI when you need:

- explicit terminal UI control over rendering and state,
- a lightweight Rust library instead of a retained component framework,
- and optional local provider lifecycle control for external capabilities.

## When to consider another approach

Prefer other projects if you need:

- automatic VDOM/component-tree behavior,
- a hosted SaaS model instead of local self-hosted binaries,
- a built-in MCP server or pre-integrated proprietary adapter marketplace,
- guaranteed Windows-specific terminal guarantees as a first-class supported target.

## Project status and maturity

Versioned at `0.1.0` with active pre-1.0 status (as declared in docs).
Release packaging and adapter-host behavior are documented in docs, with explicit checks and update/uninstall procedures.

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cargo check --features adapter-showcase --bin dragonstui-showcase
cargo test --features adapter-showcase --bin dragonstui-showcase
cargo clippy --features adapter-showcase --bin dragonstui-showcase -- -D warnings
```

## Documentation map

- [User guide](docs/user-guide.md): 10-minute onboarding flow.
- [Installation and first run](docs/installation.md): releases, checksums, roots, update/uninstall.
- [Public API](docs/public-api.md): library surface and explicit ownership model.
- [Adapter management](docs/adapter-management.md): registry/install/update/remove and CLI behavior.
- [Protocol v1](docs/adapter-protocol-v1.md): message envelope and message contract.
- [Protocol SDK specification](docs/adapter-sdk-specification.md): cross-language guidance.
- [Adapter architecture](docs/architecture/adapter-host.md) and [component model](docs/architecture/component-model.md).
- [Terminal compatibility](docs/terminal-compatibility.md): real tested matrix and protocol boundaries.
- [Reference mock](docs/reference-mock-adapter.md): isolated fixture provider for experimentation.
- [Docker adapter](docs/docker-adapter.md): optional real provider and operational requirements.
- [Non-interactive agent adapters](docs/agent-adapters.md): optional local provider examples.

## Contributing and operations docs

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening PRs.

## License

Licensed under the [MIT License](LICENSE).