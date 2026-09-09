# User guide: your first 10 minutes

DragonsTUI is a Rust toolkit for building terminal interfaces. The included
**showcase** is an interactive demonstration and an optional interface for
external adapters; it is not a ready-made Docker, Git, or database manager.
You can explore it without an account, credentials, or adapters.

This is a suggested 5–10 minute reading and exploration route, not a measured
usability result. Installation and acquiring a trusted provider are separate
prerequisites and can take longer.

## 1. Open the right application (minute 0–1)

Install the native binaries using [Installation and first run](installation.md).
Public release downloads are not available yet; that guide explains CI packages
and supported platforms. Then, in a real terminal, run:

```sh
dragonstui-showcase
```

From a source checkout, the equivalent is:

```sh
cargo run --release --features adapter-showcase --bin dragonstui-showcase
```

| Name | What it is for |
| --- | --- |
| `dragonstui-showcase` | The interface in the README screenshots; start here |
| `dragons_tui` | A separate dashboard demo, not the showcase; its optional agent-start action requires an independently configured external Hermes |
| `dragonstui-adapter` | Adapter installation, inspection and lifecycle commands in your shell |
| `dragonstui-adapter-host-mock` | A protocol test provider, not an interactive application or shell |

Opening either UI does not automatically start an external agent or install an
adapter. You do not need to set up Hermes just to explore the showcase.

## 2. Take a keyboard tour (minutes 1–3)

1. Press `Enter` or `Space` to leave the splash and see **Overview**.
2. Use the numbered header sections (`1`–`8`) to explore the demonstrations.
   The header tabs also accept mouse selection.
3. Press `Tab` to move focus. Use arrow keys in the focused control rather than
   expecting every arrow key to change the whole page.
4. Open the command palette with `Ctrl+P` to see available commands; `m` opens
   the demonstration modal. Follow the visible prompts for the active overlay.
5. Press `8` to open **Adapters**. An empty list is normal on a fresh installation.

Use `q` or `Ctrl+C` to exit and restore the terminal outside an active adapter
session. A hosted session owns those keys: `q` is text and `Ctrl+C` reaches the
remote process. Close it with `Alt+X` first, then quit the showcase.
A larger terminal makes the panels easier to read; the README screenshots use
160 × 48 cells, not a mandatory minimum.

**What you are seeing:** widget/demo values are not live system telemetry.
Settings and UI history are not saved between launches. With a running adapter,
live views contain bounded, in-memory provider output, not a durable log archive.

## 3. Understand adapters and choose a root (minutes 3–4)

An **adapter** is an external program that speaks the DragonsTUI protocol.
A **registry** lists available adapter versions and platform-specific artifacts.
A **root** is the directory containing your installed adapters. A **controller**
is the local daemon that owns their running processes; closing the showcase
does not stop them.

No domain adapters or default registry are bundled. Capability names are
provider-declared contracts, not proof of a built-in integration. You can stop
here and continue exploring the UI without installing anything.

For adapter use, choose one stable root and pass it to both programs:

```sh
mkdir -p "$HOME/.local/share/dragonstui/adapters"
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" list
dragonstui-showcase --adapter-root "$HOME/.local/share/dragonstui/adapters"
```

The CLI prints its header and empty-root guidance when nothing is installed.
Section 8 stays empty. Discovery reads metadata; it does not execute providers.
Without `--adapter-root`, showcase discovery is disabled. Without `--root`, the
CLI instead uses `./adapters` in the current working directory.

The path above is a recommendation for both macOS and Linux, **not automatic
XDG/config discovery**. Installing binaries on PATH does not configure this root.

## 4. Add a trusted adapter (minutes 4–7, once a provider is available)

Obtain a registry path or HTTPS URL from a publisher you trust, review the
provider's requirements, and confirm it supports your platform. Adapters run
with your user permissions: process isolation is **not a sandbox**. SHA-256
checks verify bytes against registry metadata, not the publisher or code safety.

The following are **command templates**, not a bundled registry or an available
sample adapter. Replace `REGISTRY_SOURCE` with your actual registry path/HTTPS
URL, and `ADAPTER_ID` with an ID returned by search. Do not paste the placeholders
unchanged. Run each command separately and resolve any error before continuing.

```sh
dragonstui-adapter search --registry REGISTRY_SOURCE
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" install ADAPTER_ID --registry REGISTRY_SOURCE
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" list
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" info ADAPTER_ID
```

After a successful install, the ID should appear in `list`; `info` shows its
metadata. **Install does not start it.** If you choose to run it:

```sh
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" start ADAPTER_ID
dragonstui-showcase --adapter-root "$HOME/.local/share/dragonstui/adapters"
```

The CLI establishes the controller when no endpoint exists. In Section 8, select
the adapter to inspect its metadata and available runtime diagnostics. Runtime
fields can be unavailable (`--`); do not interpret missing diagnostics as proof
that no process exists. `C` switches between adapter and capability browsing.
The Capability Browser lists reported contracts/providers; it does not invoke
capabilities. What other live views/actions expose depends on the provider.

For real container data and a text shell, follow the optional
[Docker adapter guide](docker-adapter.md). It requires Python, Docker access and
an explicitly bound container; installing DragonsTUI alone does not enable it.

For a service-free exercise, the [reference mock guide](reference-mock-adapter.md)
provides a separate, isolated source-checkout exercise. It uses fixture data and
echo sessions, not a real service or shell. Installing the mock executable on
PATH alone does not register an adapter. Do not invent a registry URL to fill
this gap.

## 5. Stop deliberately and know where to go next (minutes 7–10)

Quit the showcase, then explicitly stop an adapter you started (replace the ID):

```sh
dragonstui-adapter --root "$HOME/.local/share/dragonstui/adapters" stop ADAPTER_ID
```

This stops the adapter, not the controller daemon. There is no public CLI daemon
shutdown command. Before replacing or removing the application binaries, follow
the [update and uninstall precautions](installation.md#update-the-installed-binaries).
Do not delete controller endpoint files to work around authentication failures.
They contain private credentials and must not be shared in bug reports.

### If something does not match the guide

| Symptom | First check |
| --- | --- |
| Wrong screen | Run `dragonstui-showcase`, not `dragons_tui` |
| Command not found | Check `command -v dragonstui-showcase` and the installation guide's PATH instructions |
| UI fails in a pipe or CI log | Open it in a real interactive terminal; use `--help` for non-interactive usage |
| Adapter visible in CLI but not UI | Use the same absolute root with `--root` and `--adapter-root`; reopen the showcase with that root |
| Installed but no live data | Installation does not start providers; inspect start errors and provider capabilities |
| Incompatible/failed adapter | Check metadata, protocol/platform compatibility and the provider's dependencies before an explicit restart |
| Registry/checksum/controller error | Stop and use [troubleshooting](installation.md#troubleshooting); do not bypass checks or delete credentials |

For a useful issue report, include binary version, OS/architecture, terminal,
steps and redacted error text. Exclude tokens, endpoint files, provider secrets
and unreviewed adapter output.

### Choose your next document

- **Install, update or uninstall the application:** [Installation and first run](installation.md).
- **Manage adapters:** [Adapter management](adapter-management.md), including update/remove semantics.
- **Try a local fixture provider:** [Reference mock adapter](reference-mock-adapter.md).
- **Build an adapter:** [Adapter SDK specification](adapter-sdk-specification.md) and [conformance suite](adapter-conformance.md).
- **Build your own TUI:** [Public API](public-api.md) and the repository's [examples](../examples/).
- **Check terminal support:** [Terminal compatibility](terminal-compatibility.md).

You have completed this introduction when you can open the showcase, move
between sections, explain why an empty Adapters screen is normal, choose a
shared root, and distinguish **install**, **start**, **stop**, and **quit UI**.
Successfully installing a real provider additionally requires a trusted registry
and a compatible artifact; this guide does not claim those are bundled.
