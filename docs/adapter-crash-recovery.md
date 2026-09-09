# Adapter crash and recovery

M70 hardens the existing supervised runtime and manager without adding automatic restart, heartbeat or retry policy. The controller remains the owner of runtime lifecycle; applications use its typed clients. Direct runtime/manager tests are internal boundary tests, not a second UI execution path.

## Failure contract

- A request deadline expires that request; it does not prove the provider has crashed. An unresponsive provider can be explicitly stopped or restarted through the existing controller.
- EOF, malformed JSON and stdout read failures terminalize the current runtime generation. The first decoder/read failure retains its `RpcError::Failed` detail; pending requests become `RpcError::Crashed`. Later pumps cannot accept buffered replies as recovery.
- Repeated crash polling preserves the first diagnostic and adds no duplicate `Crashed` entries to runtime state history.
- A full response queue returns backpressure, not a crash verdict. Draining retained responses allows processing to continue. An independently observed process exit still causes a crash even under backpressure.
- The manager removes failed capability registrations, emits one disconnect per running-to-failed transition, and fails pending operations/sessions through existing paths. Other providers remain usable.
- An unexpected exit with status zero is still a failure of a running provider; zero is not a protocol shutdown acknowledgement.
- Recovery is explicit. Restart stops/reaps the old supervised child, starts a new generation, and republishes capabilities after a valid handshake. Merely polling a failed provider does not restart it.

A protocol-failed provider may remain an OS process until explicit stop/restart or owner drop. Logical failure is not an immediate kill policy. No automatic crash-loop scheduler or backoff has been added.

## Verification

Run the focused public-boundary regressions:

```sh
cargo test -p dragonstui-adapter-host --test rpc --test manager
```

They are included in the required workspace test gate. New M70 cases cover:

- 100 terminal polls with stable history and first error;
- malformed output followed by a valid response, which must not revive the runtime;
- a one-slot response queue remaining healthy until drained;
- nine explicit failure/recovery cycles across malformed output and zero/nonzero exits, with a healthy peer, correlated outcomes, stable diagnostics, single disconnects and restored capability registration;
- a blocked `test.slow` request timing out, peer RPC continuing, and explicit restart completing within a three-second fixture allowance.

On POSIX, the recovery tests use `ps` to verify that the old direct child PID no longer exists after restart (including absence of an unreaped zombie). These are local, finite fixture assertions, not performance budgets.

## Evidence limits

The fixtures use small requests and no descendant processes. They do not establish a general hard shutdown deadline, process-tree containment, saturated-stdin cancellation, hostile oversized-frame handling or leak-free long-running recovery. Process stdin writes remain synchronous; a blocked write is not made cancellable by these changes. Reader-thread lifetime and whole-process resource measurements are not instrumented here. Controller/TUI end-to-end crash acceptance and named-terminal restoration are not newly exercised by these manager/runtime tests. See [the host architecture](architecture/adapter-host.md), [management](adapter-management.md) and [protocol v1](adapter-protocol-v1.md) for the existing boundaries.
