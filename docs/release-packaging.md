# Release packaging (R1)

R1 prepares native v0.1.0 bundles and a gated GitHub Release workflow. It does
**not** create a version tag, GitHub Release, or crates.io publication. The first
public release remains an explicit R6 action. Current workspace package versions
are both `0.1.0`; binaries obtain their version from Cargo at compile time.

## Supported package targets

| Package | Rust target | Build and separate smoke runner | Runtime baseline |
| --- | --- | --- | --- |
| `dragonstui-v0.1.0-macos-arm64.tar.gz` | `aarch64-apple-darwin` | `macos-14` (Apple Silicon) | macOS 14+, ARM64; system libraries only |
| `dragonstui-v0.1.0-linux-x86_64.tar.gz` | `x86_64-unknown-linux-gnu` | `ubuntu-22.04` | Ubuntu 22.04 x86_64; glibc 2.35 and system libgcc |

Linux uses GNU libc, not a static musl build. Other distributions with compatible
system libraries may work but are not independently verified. macOS Intel,
Linux ARM64, Windows and older OS versions are outside the R1 binary matrix.
GitHub-hosted runner images include development tools; the separate smoke jobs
perform no Rust builds, hide development-tool paths and user configuration, and
reject non-system binary dependencies. This is a fresh-runner package test, not
an assertion that every consumer machine or named terminal emulator was tested.

Each archive has a flat, exact inventory:

- `dragons_tui`: default dashboard.
- `dragonstui-showcase`: optional adapter-aware showcase.
- `dragonstui-adapter`: adapter management CLI/controller entry point.
- `dragonstui-adapter-host-mock`: reference/mock provider for local testing.
- `LICENSE` and `README.md`: MIT license and installation instructions.

No adapter installation, registry, credentials or repository-relative assets are
required to open the dashboard. Real providers and their dependencies are not
bundled. All four executables expose `--help` and `--version` without entering a
terminal session. The original executable names are preserved.

## Local packaging

Prerequisites: stable Rust with edition 2024 support, Python 3.11+, and the native
platform toolchain. Run at the repository root. For macOS ARM64:

```sh
MACOSX_DEPLOYMENT_TARGET=14.0 cargo build --locked --release --workspace --features adapter-showcase --bins --target aarch64-apple-darwin
python3 tools/release_package.py build --version 0.1.0 --platform macos-arm64 --bin-dir target/aarch64-apple-darwin/release --output target/dist
python3 tools/release_package.py verify --version 0.1.0 --platform macos-arm64 --archive target/dist/dragonstui-v0.1.0-macos-arm64.tar.gz --checksum target/dist/dragonstui-v0.1.0-macos-arm64.tar.gz.sha256
```

For Linux, build natively on Ubuntu 22.04 with target
`x86_64-unknown-linux-gnu`, omit `MACOSX_DEPLOYMENT_TARGET`, and use
`linux-x86_64` plus `target/x86_64-unknown-linux-gnu/release` in the helper.
CI builds from a fresh checkout without restoring a build cache. Local builds
may reuse Cargo outputs. `--locked` preserves Cargo.lock resolution. Stable Rust
is not pinned, so this does not promise reproducible compilation across future
toolchains; packaging identical binary/document inputs is byte-reproducible.

## Download and verify

The [installation and first-run guide](installation.md) covers PATH setup,
config/data locations, empty adapter states, troubleshooting, update and uninstall.
Its full text is included in each bundle's `README.md` for offline use.

During R1, download the two `release-*` artifacts from the **Release packaging**
Actions run. These are CI artifacts, not public Release assets. After R6, the
same tarballs and checksum files will be attached to the versioned GitHub Release.
Each tarball has its own `<archive>.sha256` file. Keep it beside the tarball:

```sh
# macOS
shasum -a 256 -c dragonstui-v0.1.0-macos-arm64.tar.gz.sha256
mkdir dragonstui-0.1.0
cd dragonstui-0.1.0
tar -xzf ../dragonstui-v0.1.0-macos-arm64.tar.gz
./dragons_tui --version
./dragons_tui
```

Linux uses `sha256sum -c dragonstui-v0.1.0-linux-x86_64.tar.gz.sha256` and the
Linux archive. Follow the included README to install into `$HOME/.local/bin`.
SHA-256 detects corruption; it is not a publisher signature. macOS signing and
notarization are not provided in R1. Browser quarantine/Gatekeeper behavior is
not covered by Actions artifact download and may require the user's normal
macOS security approval for unsigned software.

## Automation and publication boundary

[release.yml](../.github/workflows/release.yml) runs on relevant branch pushes,
pull requests, manual dispatch, and `v*` tag pushes:

1. Check both Cargo package versions; tag names must exactly match `v<version>`.
2. Run packaging regressions and required Rust delivery gates.
3. Build the four release binaries independently for both native targets.
4. Create exact-inventory tarballs and SHA-256 manifests; upload CI artifacts.
5. Download each artifact on a separate native runner. Verify checksum,
   inventory/modes, architecture and system-only linked libraries; extract outside
   the checkout; execute every binary's version/help with fresh HOME/XDG state and
   a system-only PATH. Run the packaged dashboard in a PTY, quit with `q`, and
   verify terminal restoration and process cleanup.
6. **Only for a pushed version tag**, and only after both platform smoke jobs and
   quality gates pass, create a GitHub Release with both archives and checksums.
   `--verify-tag` prevents the publishing command from creating a missing tag.

Branch/PR/manual runs cannot reach publication. Read-only permissions are the
default; only the tag-gated publishing job receives `contents: write`.
Publication failure is a failed run, not a silently accepted partial release.
If a release already exists, inspect it rather than overwriting it automatically.
No tag-creation command is run as part of R1. The actual GitHub Release API write
remains intentionally unexercised until R6.

## Evidence

The current R1 checkpoint records exact local verification and CI run/commit
identities. M74's [readiness evidence](adapter-release-readiness.md) is historical
source-audit evidence, not proof for later R1 binaries. A dry-run is accepted only
when both downloaded native bundles pass and the `publish` job is skipped.
