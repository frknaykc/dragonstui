# Adapter Host Performance (M72)

## Reproduce

Run from the repository root with a stable Rust toolchain:

```sh
cargo test -p dragonstui-adapter-host --release --test performance -- --ignored --nocapture
```

The opt-in test emits 18 `M72_RESULT` JSON records and three `M72_CLEANUP` records. Repeat three times to inspect run-to-run variation. Normal workspace tests compile the harness and run its percentile helper test, but skip timing workloads. There are no performance thresholds in CI. A debug build explicitly rejects the measurement test.

The harness creates unique temporary adapter roots, copies the workspace mock executable into them, and starts only those local fixtures. The manager stops and reaps children before the fixture root is removed, including during assertion unwinding. No registry, network, installed provider, controller credentials, or TUI state is used.

## Workloads and interpretation

- **Serialization:** 1,000 encode and 1,000 decode iterations of a typed v1 request for each 64-byte, 4-KiB and 64-KiB ASCII text value. `black_box` retains measured work; a round-trip assertion checks equality. Wire byte count includes the JSON envelope, but excludes the newline. This measures serde allocation/encoding/decoding, not the bounded production writer, pipe transport or allocator profiling.
- **RPC:** 100 rounds of one outstanding echo request per adapter, for 1/4/8 adapters and 64-byte/4-KiB text values. Latency runs from before payload cloning and dispatch through response consumption. Samples include provider execution, pipe/reader scheduling, manager polling and harness overhead. They are not pure host service time.
- **Streaming:** 100 stream requests per adapter, each producing 129 events before its response. Assertions check response identity/payload, event ownership, exact event totals, zero manager drops and completion counts for every adapter.
- **Scheduling:** 100 idle manager polls at zero and 1-ms **per-adapter** timeout. Workload polling uses zero timeout; `poll_total_ns` is wall time spent inside calls, not CPU time or an exclusive profiler sample. Sequential polling order and OS scheduling affect observed latency. Completion counts are not a proof of starvation freedom under arbitrary sustained load.

Startup uses an ordered echo-response barrier and is excluded. All request waits have a 10-second deadline; normal stop uses a 300-ms grace period. Queues remain bounded: ingress 8, runtime events 8, responses 4, manager events 32. The benchmark explicitly raises its own ingress event budget to 10,000,000/s, just as the M69 stress workload does; production defaults are unchanged. Test assertions and per-poll clock reads contribute overhead. This short workload is a reproducible diagnostic baseline, not a saturation or capacity benchmark.

## Local reference evidence

[Machine-readable results](adapter-performance-results.json) contain three consecutive local release runs, platform/toolchain identity and SHA-256 hashes of the measured host source, test, manifests and lockfile. They describe this source snapshot, not subsequent edits or other machines. No synthetic/mock timing values are substituted for observed timings.

| Workload | 1 adapter | 4 adapters | 8 adapters |
| --- | --- | --- | --- |
| 64-byte echo p95, µs (range across runs) | 56–69 | 30–98 | 58–154 |
| 4-KiB echo p95, µs | 43–48 | 36–77 | 70–90 |
| Stream response p95, µs | 230–347 | 1,118–1,575 | 3,236–3,315 |
| Idle zero-timeout poll, µs/call | 0.305–0.615 | 0.863–1.284 | 1.745–3.462 |
| Idle 1-ms/adaptor poll, ms/call | 1.258–1.261 | 5.004–5.062 | 10.028–10.037 |

Each run completed 3,900 measured RPCs and delivered 167,700 stream events without drops; all 13 fixture adapters stopped. JSON encode cost ranged from 146–367 ns for 64-byte text, 1,336–2,588 ns for 4-KiB text and 17,676–19,633 ns for 64-KiB text. Decode measurements, throughput, maxima and individual run values are in the JSON artifact.

## Optimization decision

No production optimization is justified by these measurements alone. Idle nonzero polling grows with adapter count because the public argument is a per-adapter timeout, not a total call budget. Replacing that behavior would change the API contract. Callers that own their event-loop pacing can use zero-timeout polling, but an unpaced busy loop consumes CPU; the benchmark deliberately uses one and does not recommend it for production.

Stream response latency grows in the multi-adapter fixture with tiny bounded ingress queues. These end-to-end observations do not isolate a removable host bottleneck from provider serialization, pipe backpressure and OS reader scheduling. The next optimization investigation should profile that path before choosing a change. M72 therefore adds reproducible evidence without changing queue sizes, dropping responses, weakening limits, changing controller authority, or claiming an unmeasured speedup.

M69 remains the longer stress/resource evidence; this harness does not replace its held-consumer tests or establish new CPU/RSS, real-provider, controller IPC, UI frame-time, fairness, or cross-platform performance guarantees.
