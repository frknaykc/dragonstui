# Installation and first run

For a short explanation of the applications, a keyboard tour, and the adapter
workflow, see the [first 10 minutes user guide](user-guide.md).

This guide covers the v0.1.0 native bundles prepared by R1 and the current R2
first-run behavior. **There is no public release download yet.** Obtain the
matching archive and checksum from the project's **Release packaging** Actions
run (the `release-*` CI artifacts), or build a local bundle using
[release packaging](release-packaging.md). Public release assets remain an
explicit R6 publication step.

## Choose a package

| System | Archive | Runtime baseline |
| --- | --- | --- |
| Apple Silicon Mac | `dragonstui-v0.1.0-macos-arm64.tar.gz` | macOS 14+, ARM64 |
| Linux PC/server | `dragonstui-v0.1.0-linux-x86_64.tar.gz` | Ubuntu 22.04 x86_64, glibc 2.35 and system libgcc |

Linux is a GNU libc build, not static musl. Other compatible distributions may
work but are not independently verified. macOS Intel, Linux ARM64, Windows,
and older OS versions are outside this binary matrix. Check your machine with
`uname -s` and `uname -m`. Rust is not needed to run a native bundle.

Both interactive UIs need a real terminal with working input, ANSI output, and
Unicode support. A larger terminal makes the showcase easier to explore; the
README screenshots use 160 × 48 cells. See
[terminal compatibility](terminal-compatibility.md) for evidence and limits.

## Verify, extract, and install

Use a directory containing the archive and its matching `<archive>.sha256`
file. If downloaded as an Actions artifact ZIP, unpack that outer ZIP first.
Choose **one** platform block below. Commands use a POSIX-compatible shell
(such as zsh or bash); each block stops on a failed check and extracts into a
new directory. Do not run an unverified executable.

macOS:

```sh
(
    set -eu
    archive=dragonstui-v0.1.0-macos-arm64.tar.gz
    shasum -a 256 -c "$archive.sha256"
    tar -tzf "$archive"
    mkdir dragonstui-0.1.0-macos-arm64
    tar -xzf "$archive" -C dragonstui-0.1.0-macos-arm64
)
```

Linux:

```sh
(
    set -eu
    archive=dragonstui-v0.1.0-linux-x86_64.tar.gz
    sha256sum -c "$archive.sha256"
    tar -tzf "$archive"
    mkdir dragonstui-0.1.0-linux-x86_64
    tar -xzf "$archive" -C dragonstui-0.1.0-linux-x86_64
)
```

Require an `OK` checksum result and a successful block before continuing. A
pre-existing extraction directory makes `mkdir` fail deliberately: use a new
empty directory rather than merging old and new files. The archive's flat
inventory is exactly these four executables plus `LICENSE` and `README.md`:

| Executable | Purpose |
| --- | --- |
| `dragons_tui` | Default dashboard; not the screenshot showcase |
| `dragonstui-showcase` | Screenshot showcase and optional adapter interface |
| `dragonstui-adapter` | Plain-terminal adapter management CLI and controller entry point |
| `dragonstui-adapter-host-mock` | Protocol fixture provider, not a normal interactive application |

SHA-256 detects corruption and checks consistency with the supplied checksum;
it is **not a publisher signature**. Obtain both files from a source you trust.
Do not extract an unexpected archive inventory or ignore a checksum failure.
macOS bundles are not signed or notarized. If macOS blocks an unsigned binary,
review its provenance and use your normal macOS security approval process;
do not disable system-wide protections.

Change into the successfully extracted directory for your platform:

```sh
# macOS; on Linux use dragonstui-0.1.0-linux-x86_64 instead.
cd dragonstui-0.1.0-macos-arm64
```

Install only the four executable files into your user-owned bin directory:

```sh
(
    set -eu
    mkdir -p "$HOME/.local/bin"
    for bin in dragons_tui dragonstui-showcase dragonstui-adapter dragonstui-adapter-host-mock; do
        install -m 0755 "./$bin" "$HOME/.local/bin/$bin"
    done
)
export PATH="$HOME/.local/bin:$PATH"
command -v dragonstui-showcase
dragonstui-showcase --version
dragonstui-adapter --help
```

No `sudo` is needed. If any of these destination names already exists, inspect
it before replacing it; use the update procedure below for an existing
DragonsTUI installation. Keep the extracted `LICENSE` and `README.md` for
reference. Add the same `export PATH="$HOME/.local/bin:$PATH"` line once to the
startup file actually read by your shell (for example `~/.zshrc` for interactive
zsh or `~/.bashrc` for interactive bash). Open a new shell and check
`command -v dragonstui-showcase` again. All four binaries support `--help` and
`--version` without entering an interactive terminal session.

## First screen: the showcase

To open the interface pictured in the README, run:

```sh
dragonstui-showcase
```

The opening splash transitions to **Overview**; `Enter` or `Space` skips the
wait. Use `1`–`8` or the visible header tabs to select sections, `Tab` to move
focus, and arrow keys to navigate the focused control. `Ctrl+P` opens the
command palette; `m` opens the demonstration modal. Use `q` or `Ctrl+C` to quit
and restore the terminal (leave a focused text-entry/session mode first if it
captures ordinary character keys).

Press **8** for **Adapters**. Without a root, discovery is disabled; with an empty
root, there are no installed adapters. Neither is a failed installation. The showcase has **no
adapter root by default**. It does not implicitly scan `./adapters` or a home
directory. Its widget/demo data is not live provider telemetry.

### Optional: choose a stable adapter data root

On **both macOS and Linux**, this guide recommends
`$HOME/.local/share/dragonstui/adapters` as a stable, user-owned location:

```sh
mkdir -p "$HOME/.local/share/dragonstui/adapters"
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" list
dragonstui-showcase --adapter-root "$HOME/.local/share/dragonstui/adapters"
```

For an empty root, CLI `list` prints its column header with no adapter rows and
first-run guidance on stderr;
Section 8 remains empty. Merely discovering metadata does not execute adapters.
A valid discovered adapter normally appears as Stopped when no live controller
state is available; incompatible or invalid entries are reported separately.

**These paths are a documentation convention, not automatic application
behavior or XDG support.** DragonsTUI does not resolve `XDG_DATA_HOME` or create
this recommended root at startup. Pass the same absolute root on each relevant
invocation. In particular:

- `dragonstui-adapter` defaults to **`./adapters`**, relative to the current
  working directory, unless `--root` is supplied.
- `dragonstui-showcase` only uses a root explicitly supplied with
  **`--adapter-root`**; it does not inherit the CLI default.
- Installing the binaries into `~/.local/bin` does not configure either root.

## Configuration and data

The dashboard and showcase currently have **no persisted application
configuration file, saved Settings preferences, or saved UI/session history**.
Showcase Settings changes are in-memory and reset on restart. Retained live
history and session scrollback are bounded in-memory views, not a durable log
archive. There is no application config file to create for first run.

Adapter use is different: the explicitly selected root contains installed
adapter directories, their manifests/executables and `adapter-install.json`
installation metadata. Controller use may also create
`<root>/.controller/endpoint.json`. That file contains a **private local
credential**: do not print it, attach it to bug reports, commit it, or put its
token in command-line arguments. Keep the root user-owned and protect any
backups. External providers and Hermes may have their own configuration,
credentials, and persistent data; those are not managed by these UI settings.

## Adapters are optional and require trust

The four native binary bundles do not include domain providers. Installing
them neither installs a provider into your adapter root nor downloads one
automatically. There is no default registry for an empty first run. A reviewed
source checkout includes a separately installed [Docker adapter](docker-adapter.md)
with its own Python/Docker requirements and explicit container binding.

To add a provider, first choose and review a trusted registry (a local registry
file or HTTPS URL), its publisher, and the provider's own requirements. See
[adapter management](adapter-management.md) for `search`, `install`, `update`,
`remove`, and explicit lifecycle commands. Registry SHA-256 and size checks
verify artifact bytes against metadata, not publisher identity or code safety.
Adapters are external processes running with your user permissions, **not
sandboxed plugins**. Do not execute an untrusted provider.

**Install does not start an adapter.** After installing a trusted adapter, use
its actual ID in a separate command:

```sh
# Replace <adapter-id> with the installed ID; angle brackets are not literal.
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" start <adapter-id>
```

The CLI connects to an authenticated loopback controller, creating the daemon
when no endpoint exists. The controller, not the UI, owns runtime lifecycle.
Showcase lifecycle controls use an existing controller connection; if none is
available, explicitly start the installed adapter with the CLI first. Closing
the showcase is not a request to stop daemon-owned adapters.

The installed `dragonstui-adapter-host-mock` executable speaks the host's JSON
Lines protocol over stdin/stdout. Running it directly is **not** the way to
open another interactive UI or shell, and putting it on PATH does not register
it as an installed adapter. For an isolated mock setup and explicit lifecycle
exercise, use the [reference mock guide](reference-mock-adapter.md).

## The default dashboard is a separate application

```sh
dragons_tui
```

This opens the default dashboard, not the showcase and not its Section 8.
It opens without adapters, a registry, credentials, or repository assets.
Its initial output includes demonstration data and its agent starts Stopped.

The dashboard's explicit agent-start action currently launches the external
command **`hermes --cli`** with piped input/output. Hermes is optional, is not
bundled, and is not installed or started just by opening the dashboard. The
agent table's labels do not imply bundled Codex/Claude integrations. If you
choose to use the start action (`Enter` while the agent table is focused), a
compatible `hermes` must already be on PATH and configured independently.
Otherwise expect a visible Start failed error; the UI itself can still run.
Do not supply provider secrets merely to explore the dashboard or showcase.

## Update the installed binaries

There is no automatic self-updater. Obtain the replacement archive and its
matching checksum for your platform, verify and extract into a **new** directory
using the procedure above, and read that version's release/compatibility notes.
Do not mix binaries from different bundles.

Before replacing files:

1. Stop each running adapter with the existing installed CLI and its original
   root, for example `dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" stop <adapter-id>`.
   Repeat for any other roots you used. Resolve errors before proceeding.
2. Stop any dashboard-started agent and quit all running dashboard/showcase
   windows. Do not assume quitting the showcase stops adapters.
3. If a controller daemon was started, arrange for that specific daemon to exit
   and verify it has exited before replacing `dragonstui-adapter`. **The public
   CLI has no controller shutdown subcommand.** `stop <id>` stops an adapter,
   not the daemon; the hidden `controller-daemon` entry point is not a shutdown
   command. An authenticated shutdown exists in the Rust controller API, not as
   a packaged end-user command. Use an existing trusted controller integration
   if you have one, or identify the exact process for your root using OS process
   tools after stopping its adapters. Do not use a broad process-name kill.
   If you cannot establish ownership and a safe stop, defer the update.

Once processes are stopped, change into the newly verified extraction directory
and repeat the same four-file `install -m 0755` loop above. This is four separate
file replacements, not an atomic multi-file update: stop and resolve any copy
failure before launching anything. Run `--version` on each of the four installed
executables and confirm all match the replacement bundle, then reopen the UI.
Restart desired adapters explicitly. Your selected adapter root is retained;
binary replacement is not an adapter update or a data migration.

The daemon removes its endpoint after a catchable Unix `SIGTERM` or `SIGHUP`.
An uncatchable termination (such as `SIGKILL`) can still leave one behind; follow
the controller troubleshooting guidance below rather than blindly deleting it.

## Uninstall

First follow the same adapter/UI/controller stop precautions as for an update,
while the management CLI is still installed. Then remove **only** the four
executables installed by this guide:

```sh
rm -i "$HOME/.local/bin/dragons_tui" \
      "$HOME/.local/bin/dragonstui-showcase" \
      "$HOME/.local/bin/dragonstui-adapter" \
      "$HOME/.local/bin/dragonstui-adapter-host-mock"
```

Review each prompt. Do not delete `~/.local/bin` or `~/.local/share`: they may
contain unrelated applications. Keep the PATH entry if other programs use it.
**Adapter data is retained by default**, including the recommended root and any
custom roots. If you separately want to remove an installed adapter, use the
CLI's `remove <id> --yes` with its explicit root before uninstalling the CLI,
after reviewing the data involved. External provider/Hermes data is separate.

## Troubleshooting

| Symptom | Check and next step |
| --- | --- |
| `command not found`, or an unexpected version | Use `command -v dragonstui-showcase` (or the affected binary); ensure `~/.local/bin` is on PATH and not shadowed by another installation. Try the explicit `"$HOME/.local/bin/dragonstui-showcase" --version`, then reopen the shell after correcting its startup file. |
| Permission denied when launching | Check `test -x "$HOME/.local/bin/dragonstui-showcase"`. Reinstall the verified file with mode `0755`. Check directory traversal permissions and whether the filesystem is mounted `noexec`; do not solve this by running the UI as root. On macOS, distinguish Gatekeeper approval from Unix executable permissions. |
| `Exec format error`, bad CPU type, missing loader/library | Compare `uname -s`, `uname -m`, and the package table. Use the matching OS/architecture bundle and runtime baseline; Linux requires GNU libc, not musl. Changing permissions will not fix the wrong architecture. |
| No TTY, raw-mode/input error, or UI fails in a pipe/CI log | Run `dragons_tui` or `dragonstui-showcase` in an interactive terminal with stdin/stdout attached, not redirected or piped. `--help`, `--version`, and adapter CLI commands do not require alternate-screen UI mode. |
| Empty Section 8 or no CLI rows | Normal on a fresh install. Check that you passed the same absolute root to CLI `--root` and showcase `--adapter-root`; PATH does not set an adapter root. No providers are bundled or automatically downloaded. |
| Missing root / could not read adapter root | Discovery reads an existing directory. Create your intended root with `mkdir -p "$HOME/.local/share/dragonstui/adapters"`, then pass it explicitly. Check for a typo or an unintended current directory before creating anything elsewhere. |
| Root/installation permission error | Use a directory owned by your user; check parent-directory access, free space, and write permissions. Do not use `sudo`, make the root world-writable, or recursively change unrelated directories. |
| Registry parse, download, version, or platform error | Check the explicit registry source, adapter ID, requested version, and availability of an exact OS/architecture artifact. There is no implicit public registry or fallback to an older release just to match the platform. |
| Package or adapter checksum/size mismatch | Stop. Re-obtain the artifact and metadata from the trusted source and investigate the mismatch. Do not edit the expected checksum or bypass verification to make installation succeed. |
| Missing controller, unavailable controller, authentication failure, timeout, or invalid endpoint | Confirm the root, user account, and intended daemon. A fresh root needs an explicit CLI `start` for a trusted installed adapter before showcase runtime controls are useful. For an existing endpoint, investigate the reported error; do not spawn a competing daemon or delete the endpoint to bypass authentication. |
| Adapter reports Failed/Incompatible or will not start | Inspect Section 8 metadata and diagnostics, protocol version, executable path, and the provider's own runtime requirements. Missing live diagnostics are not proof that no process exists: CLI list/info can fall back to discovery state when a live-state lookup fails. Restart only explicitly after resolving the cause. |

For controller recovery, **never remove `<root>/.controller/endpoint.json`
merely because authentication failed or a request timed out**. It is private
credential-bearing state. Only after independently confirming that the original
daemon has stopped and the endpoint is genuinely stale should you explicitly
remove that one endpoint file for the correct root. Do not print its contents
or remove the adapter root. Normal authenticated daemon shutdown removes its
endpoint; catchable Unix `SIGTERM` and `SIGHUP` also do so. Uncatchable
termination (for example `SIGKILL`) need not do so.

When reporting a problem, include the affected binary/version, OS/architecture,
terminal, command with sensitive values redacted, and error text. Do not attach
endpoint files, tokens, provider credentials, or unreviewed adapter output.
