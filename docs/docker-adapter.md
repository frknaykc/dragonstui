# Docker adapter (R4)

The Docker adapter is the first real domain provider for DragonsTUI. It lists
containers and exposes inspection, recent logs, metrics, lifecycle actions and
an interactive text shell. It uses the **existing generic protocol v1**:
DragonsTUI core, the controller and the showcase do not parse Docker payloads or
contain Docker-specific UI branches.

This is a separate, single-file Python executable, not a fifth native application
binary and not a Docker Engine plugin. The controller owns its process. Docker
CLI subprocesses are owned by the provider, never launched directly by the UI.

## Requirements and authority

- macOS or Linux, Python **3.10+**, Docker CLI and an accessible Docker Engine.
  Docker Desktop is supported through its CLI; Windows hosts are not supported.
- The controller's environment must be able to find `python3` and `docker`.
  `docker_bin` can specify a trusted absolute Docker CLI path.
- Docker context, host and TLS environment are inherited from the controller.
  Configure an explicit `context` when multiple engines are available. The
  adapter does not manage Docker credentials or start Docker Desktop.
- For interactive exec, the selected container must be running and provide
  `/bin/sh` and Linux `/proc`. Distroless images and Windows containers do not
  provide this shell contract. Use an init/reaper in containers that spawn jobs.
- Docker access is powerful host-level authority; the adapter is **not a sandbox**.
  The shell runs as the container's configured user, with its existing permissions.
  There is no automatic privileged exec, host mount, image download or container
  creation. Only connect engines and containers you are authorized to manage.

## Install from a reviewed checkout

There is no public R4 download or published registry yet. With the four DragonsTUI
application binaries already installed (see [installation](installation.md)), run
these commands from the repository root:

```sh
mkdir -p target
python3 -m tools.packaging.docker_adapter_package --output target/docker-adapter-0.1.0
ADAPTER_ROOT="$HOME/.local/share/dragonstui/adapters"
dragonstui-adapter --root "$ADAPTER_ROOT" install docker \
  --registry "$PWD/target/docker-adapter-0.1.0/registry.json"
dragonstui-adapter --root "$ADAPTER_ROOT" info docker
```

The packaging command refuses an existing output directory. It creates the
standalone executable, its SHA-256 file and a local registry. The installer
verifies the artifact; **installation does not execute it**. The registry's
`file://` paths refer to that package directory, so do not move it before install.
Checksums establish integrity against this registry, not independent publisher
trust. Read the source before granting it Docker access.

Python script artifacts are listed for macOS/Linux on ARM64/x86_64. This does
not bundle Python or Docker and does not replace platform acceptance testing.

## Bind a container before enabling actions and shell

Without configuration the adapter is useful for read-only container discovery.
It does not guess a target from the first row, the current selection or a name
in an event. Container actions and sessions are declared only when a target is
configured. The current generic session-open contract carries a capability and
terminal dimensions, **not a container-selection payload**.

Choose your own container and obtain its full ID without reading its environment:

```sh
docker container ls --all --no-trunc
# Replace the example name with a container you are authorized to operate.
CONTAINER="your-container-name"
CONTAINER_ID="$(docker container inspect --format '{{.Id}}' "$CONTAINER")"
export CONTAINER_ID ADAPTER_ROOT
python3 - <<'PY'
import json, os, pathlib, re
container = os.environ["CONTAINER_ID"]
if not re.fullmatch(r"[0-9a-f]{64}", container):
    raise SystemExit("Expected one full container ID")
path = pathlib.Path(os.environ["ADAPTER_ROOT"]) / "docker" / "docker-config.json"
with path.open("x", encoding="utf-8") as output:
    json.dump({"container": container}, output)
    output.write("\n")
path.chmod(0o600)
PY
```

This refuses to overwrite an existing config. The optional sidecar
`<adapter-root>/docker/docker-config.json` accepts only these keys:

| Key | Meaning |
| --- | --- |
| `container` | Name or ID resolved **once at provider launch** to a full ID. Prefer a full ID. |
| `context` | Explicit Docker context; otherwise use the controller's Docker environment. |
| `docker_bin` | Trusted Docker executable, default `docker` on PATH. |
| `poll_interval` | Optional number: `0` disables automatic polling, `1`–`3600` sets seconds (default `5`). Other keys above are strings. |

CLI options override sidecar values when testing the provider directly. Secrets,
TLS material and arbitrary Docker arguments do not belong in this file.

Start the **adapter process**, then open the showcase with the same root:

```sh
dragonstui-adapter --root "$ADAPTER_ROOT" start docker
dragonstui-showcase --adapter-root "$ADAPTER_ROOT"
```

**`dragonstui-adapter start/stop/restart docker` controls the provider process,
not a container.** Container lifecycle commands are the provider's declared
`docker.start`, `docker.stop`, `docker.restart` actions inside the showcase.

To change the binding, close active sessions, stop the provider, edit its
sidecar and start it again. Already-open sessions and declared actions never
follow a mutable list selection. A removed/recreated container gets a new ID;
operations on the old binding fail rather than silently selecting its replacement.
One provider instance has one action/session target. Multiple targets require
separate adapter roots/controller instances with this version; multi-container
selection in a single session browser is not implemented.

## Use it in the showcase

1. Dismiss the splash with Enter, press **8** for Adapters and select Docker.
   If stopped, **s** starts the provider; this does not start its bound container.
2. Press **a** for Adapter Actions. Choose with arrows and Enter:
   - `docker.list`: refresh the bounded container list.
   - `docker.inspect`: safe state/identity projection of the bound target.
   - `docker.logs`: explicitly read recent application logs.
   - `docker.metrics`: request a real Docker stats sample.
   - `docker.start`, `docker.stop`, `docker.restart`: bound-container operations.
3. Mutations have **confirmation_required** declarations. Inspect the target ID
   and description, confirm with Enter or cancel with Escape. The host's
   confirmation dialog is an interaction safeguard, not Docker authorization.
4. Close Actions with **a** and press **o** for Observability. Logs and Metrics
   display typed events; Status shows running state and Timeline carries the
   generic list/inspection/lifecycle details. These are live provider values, not
   the showcase's static demo dataset. Polling is bounded and does not fetch logs.
5. Close Observability with **o**, press **h** for Sessions and Enter on the shell.
   Type shell commands and Enter. Resize the outer terminal to forward dimensions.
   **Alt+x** explicitly closes the hosted shell. Outside the session, **q** exits
   the showcase. Inside the hosted session, **q/Q are input** and **Ctrl+C** goes
   to the remote shell. Close with Alt+x before using the global quit shortcuts.

Unsent session input is buffered in order up to 4096 UTF-8 bytes. A busy worker
does not discard ordinary typing bursts; overflow or disconnect is reported.
Close discards locally unsent input. Requests already accepted by the worker
are not replayed after an uncertain remote result.

The session is a real Docker `exec -it` PTY, with `TERM=dumb`. It is intended for
text commands, not `vim`, `top` or other full-screen applications. The showcase
is a text host, **not an ANSI terminal emulator**. Session output may include
terminal control characters that are not rendered as a full-screen terminal.

## Contract for adapter clients

All requests use the typed v1 envelope and request correlation described in the
[protocol](adapter-protocol-v1.md). Payload contents below are Docker-provider
semantics only; the framework keeps them opaque.

| Operation / action ID | Request payload | Result / observation |
| --- | --- | --- |
| `docker.list` | `null` or `{}` | Up to 100 projected container rows, truncation flag; timeline/status events. |
| `docker.inspect` | Optional full `container` ID for generic RPC | Safe identity/state projection; timeline event. |
| `docker.logs` | Optional full `container` ID, `tail` integer 1–200 (default 50) | Bounded recent text and truncation flag; last 32 single-line log observations with control characters escaped. |
| `docker.metrics` | Optional full `container` ID | Numeric Docker stats values with units; metric events. |
| `docker.start` | Empty payload and matching declared action | Bound-target completion or correlated error. |
| `docker.stop` | Empty payload and matching declared action | Bound-target completion or correlated error; **1 second stop grace**. |
| `docker.restart` | Empty payload and matching declared action | Bound-target completion or correlated error; **1 second stop grace**. |
| Session capability `docker.exec` | Typed session-open dimensions, no payload | One target-bound interactive `/bin/sh` session; input/resize/close/exit. |

**All declared actions accept only null/empty payloads**: they cannot override
its immutable target. Generic read RPCs may name a full ID. Mutation operations
without a matching action declaration are rejected. No arbitrary host shell,
Docker command arguments or arbitrary exec command payload is accepted.

Inspect deliberately omits environment values, labels, mounts and command lines.
Application logs and interactive shell output can still contain sensitive data:
request them explicitly and treat UI history as sensitive. The adapter does not
claim to redact arbitrary application text.

Start/stop/restart are not retried automatically. Request IDs are retained within
a bounded provider lifetime so duplicates cannot repeat a mutation; admission
stops after 4,096 IDs and requires a deliberate provider restart. This is not a
persistent exactly-once guarantee across provider restarts.

### Stop grace and uncertain outcomes

The current generic host action deadline is two seconds. Stop/restart therefore
advertise and use a **one-second** grace period; Docker may force-kill a container
that has not stopped in that interval. Do not use these actions on production
services that require a longer graceful shutdown. Use your normal Docker
administration procedure for such services. A slow/remote Engine can still
exceed the host deadline. On timeout, inspect actual container state before
retrying: failure to receive a result does not prove the mutation did not happen.

### Session cleanup boundary

Normal close, EOF, shutdown and handled termination signals must close the
owned remote shell, not just terminate the local Docker CLI. An unverified
cleanup is an error, not a successful session exit. Intentionally detached jobs,
a hostile shell changing its process identity, provider `SIGKILL`, Docker daemon
failure and network loss are outside guaranteed cleanup. The provider cannot
provide container-wide process containment. Use an init/reaper for orphaned
processes, and inspect your own container if cleanup is reported unverified.

## Update, remove and troubleshoot

- Close sessions and stop the provider before replacement. Back up its
  `docker-config.json` separately: adapter update replaces the installed directory.
  Generate a reviewed new registry artifact, use the normal adapter update path,
  then restore the reviewed sidecar before starting it. A same-version install is
  not a version upgrade; see [adapter management](adapter-management.md).
- Removing the adapter removes its installed directory/config; it does **not**
  delete containers or images. Stop it first, then use the normal explicit
  `dragonstui-adapter --root "$ADAPTER_ROOT" remove docker --yes` flow.
- No Docker row: check the shared root and executable permissions.
- Provider startup failure: check Python, Docker PATH/context, permissions,
  Docker Desktop/Engine availability and whether the bound full ID still exists.
- No container actions/session: configure a target and restart the provider.
- Exec failure: check that the target is running and supplies Linux `/proc` and
  `/bin/sh`; check Docker exec permissions and reported cleanup errors.
- Metrics unavailable: stopped containers and unsupported stats output are
  errors, never fabricated zero-valued samples.
- Controller authentication error: do not delete endpoint state to bypass it;
  follow [installation troubleshooting](installation.md).

## Reproduce local acceptance

Only run this against an Engine on which you may create temporary containers.
The image must **already exist locally** and provide the shell utilities used by
the fixture; the harness refuses to pull. The tested fixture command uses a
read-only, resource-bounded, non-root container with no network or host mounts.
It removes only its own randomly named, ownership-labelled container.

```sh
cargo build --locked --workspace --features adapter-showcase --bins
python3 -m unittest discover -s tools/tests -t . -p 'test_docker_adapter*.py' -v
python3 -m tools.acceptance.docker_adapter_smoke --image redis:7-alpine --output target/r4-docker-evidence
```

The evidence directory must be new. It holds a machine-readable report and
reconstructed showcase frames, not terminal-emulator screenshots. A passing
local run is not evidence for an untested platform, remote engine or later
source change. No public release or tag is created by these commands.
