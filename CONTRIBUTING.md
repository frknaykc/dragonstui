# Contributing to DragonsTUI

Thanks for contributing.

## Before opening an issue

- Search existing issues first.
- Include the Rust toolchain, terminal environment, a minimal reproduction, and expected versus actual behavior for bugs.
- Keep proposals grounded in the project's explicit immediate-mode architecture. Do not assume a retained component tree, VDOM, or universal widget trait.

## Pull requests

- Keep each PR focused on one concern.
- Add or update deterministic tests for behavior changes.
- Preserve public API compatibility unless the change explicitly documents a pre-1.0 break.
- Avoid drive-by formatting, dependency upgrades, or unrelated refactors.
- For adapter protocol or controller IPC changes, preserve compatibility deliberately and explain any wire-level impact.

## Local checks

Run the relevant commands before opening a PR:

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cargo check --features adapter-showcase --bin dragonstui-showcase
cargo test --features adapter-showcase --bin dragonstui-showcase
cargo clippy --features adapter-showcase --bin dragonstui-showcase -- -D warnings
```

## Scope boundaries

The core `dragons_tui` framework intentionally has no mandatory registry, networking, downloader, checksum, installer, or adapter-runtime dependency. Keep adapter and distribution work in `dragonstui-adapter-host` and optional application integration.

By contributing, you agree that your contributions may be distributed under the [MIT License](LICENSE).
