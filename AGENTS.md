# DragonsTUI

Rust 2024 workspace for an explicit immediate-mode terminal UI framework plus an optional process-isolated adapter host. Rendering stays one-way: application state → layout/primitives → `Frame` → `Buffer` → diff → terminal. Application code owns UI state, focus, input routing, overlays, and redraw policy.

## Start here

Read only the area relevant to the task. Reuse current context; inspect the affected diff/symbols and reread only if inputs changed, details are missing, or context was lost. This is a reference map, not a per-edit reading checklist:

- `Cargo.toml` and `.github/workflows/ci.yml` — workspace, feature gate, required CI commands.
- `README.md` and `CONTRIBUTING.md` — supported behavior and change-scope rules.
- `src/lib.rs` — public API boundary and module map.
- `docs/public-api.md` and `docs/architecture/component-model.md` — immediate-mode API/architecture decisions.
- `docs/terminal-compatibility.md` — terminal, Unicode, mouse, and PTY evidence limits.
- For adapter work: `docs/architecture/adapter-host.md`, `docs/adapter-protocol-v1.md`, and `docs/adapter-management.md` before changing protocol, controller, or showcase integration.

## Layout and entry points

- `src/lib.rs`: public framework exports; core modules remain private.
- `src/main.rs`: default `dragons_tui` dashboard; `src/bin/dragonstui_showcase.rs`: optional adapter-aware showcase (`adapter-showcase`).
- `src/{frame,buffer,terminal,runtime}.rs`: render and terminal boundary; `src/{layout,focus,event,viewport,scrollbar}.rs`: shared composition state.
- `tests/`: focused integration tests by primitive; `examples/`: root-export consumer examples.
- `crates/dragonstui-adapter-host/`: protocol, supervised process runtime, controller IPC, discovery/install/management, and host integration tests. Binaries: `dragonstui-adapter`, `dragonstui-adapter-host-mock`.
- `tools/pty_smoke.py`: dependency-free POSIX PTY lifecycle acceptance; `tools/showcase_pty_smoke.py`: showcase acceptance.

Use nested `AGENTS.md` only if a future large subsystem needs local instructions; keep this root file brief.

## Verification and local authority

- During implementation, run affected tests and relevant static checks. Run the required full CI gate set at the delivery boundary, not after every edit. A passing broad suite already covers its included focused tests; do not repeat them without a new diagnostic purpose.
- Prose/skill-only changes need link, schema and command-consistency checks, not application rebuilds. Harness changes require harness tests and affected acceptance; rebuild binaries only when their build inputs changed. Explicit user-requested fresh gates still apply.
- Local fixture tests and their task-owned temporary directories/processes may be created, exercised, repaired and cleaned up without repeated conversational permission once isolation is established. Do not assume all tests are isolated; inspect unfamiliar setup/cleanup paths. Keep runtime/tool approvals enabled.
- Continue the requested implementation through verification and fixes. Stop for production access, unrelated/destructive data changes, secrets, paid services, scope expansion or a real product decision. Commit/push only when the current task authorizes them. Never resume an old milestone during an audit or handoff.
- Preserve existing uncommitted work. Mechanical lint/format fixes do not need invented failing behavior tests; behavioral fixes should have focused regressions. Confirm exact test filters actually ran tests.
- For PTY failures, retain the current screen and authoritative fixture state before changing waits. Do not retry solely to obtain a passing run. Keep terminal restoration/process cleanup requirements for lifecycle changes.
- Update the current task checkpoint when state changes. Label historical CI/artifacts separately; do not claim earlier binaries validate later source edits. Give concise milestone/blocker updates, not narration for each tool call.

## Command reference

Run from the repository root with stable Rust plus `rustfmt` and `clippy` (edition 2024 support required). Required delivery gates are in `.github/workflows/ci.yml`; focused tests and interactive launch commands below are alternatives for the relevant task, not an all-at-once checklist:

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace --features adapter-showcase
cargo clippy --workspace --all-targets -- -D warnings
cargo check --features adapter-showcase --bin dragonstui-showcase
# Focused alternative; included in workspace tests above:
cargo test --features adapter-showcase --bin dragonstui-showcase
cargo clippy --features adapter-showcase --bin dragonstui-showcase -- -D warnings
cargo build
cargo run --release
cargo run --release --features adapter-showcase --bin dragonstui-showcase
python3 tools/pty_smoke.py --exit q -- target/debug/dragons_tui
python3 tools/pty_smoke.py --exit ctrl-c -- target/debug/dragons_tui
```

## Code and test conventions

- Keep render APIs explicit over a caller-supplied `&mut Frame` and `Rect`; collection/view state uses explicit `*State` values. Do not add a widget tree, VDOM, automatic event bubbling, or framework-owned application state.
- Keep terminal-specific ANSI/cursor work in `src/terminal.rs`; `Runtime` owns previous-frame diffing and normalized event polling, not application policy.
- Preserve Unicode display-width, grapheme-aware editor behavior, and `Buffer` wide-cell continuation invariants; do not byte-index text.
- Add deterministic focused tests in `tests/<primitive>.rs`; put adapter-host integration tests under `crates/dragonstui-adapter-host/tests/`.
- Keep changes focused. Avoid drive-by formatting, dependency updates, unrelated refactors, and undocumented pre-1.0 public API breaks.

## Architecture and authority boundaries

- The core `dragons_tui` crate must not gain mandatory registry, network, downloader, checksum, installer, or adapter-runtime dependencies. Keep those in `dragonstui-adapter-host` and optional showcase integration.
- The controller daemon owns adapter runtime lifecycle. TUI/CLI paths use authenticated loopback typed controller clients; do not create a second in-process manager or directly drive provider processes from the UI.
- Adapter protocol v1 is newline-delimited JSON over supervised child stdin/stdout. Keep envelope, identity, capability, and request-correlation fields typed; payload, labels, streams, and kinds stay opaque. Do not infer product semantics from them.
- Preserve bounded queues/history and terminal restoration. Essential UI interactions need keyboard routes even when mouse support is present.

## Traps

- `dragonstui-showcase` requires `--features adapter-showcase`; it is not on the default binary path.
- PTY helpers validate lifecycle/protocol and terminal restoration, not visual correctness in a named terminal emulator.
- Controller endpoint tokens are private local state: never print, commit, pass in CLI arguments, or expose them through UI/logging.
- `target/`, `.hermes/`, `.DS_Store`, and Python `__pycache__/` are ignored local/generated data. Do not stage generated output or credentials.
