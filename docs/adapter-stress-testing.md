# Multi-adapter stress testing

M69 exercises the real `AdapterManager` / `AdapterRuntime` / supervised-child boundary with independent local mock processes. It does not drive controller IPC, render the showcase, contact external providers, or establish terminal responsiveness or production capacity.

## Repeatable workloads

The mock's `--mode stress-requests` is a separate fixture mode. Each `test.stream` request emits 128 indexed `stress` events followed by one generic `test/started` event and a correlated response. `test.echo` returns its payload. It retains no growing request-ID history. The reference mode and its 1,024-ID lifetime admission limit are unchanged. No shell or external service is required.

The integration test creates a unique temporary discovery root and starts independently identified children. Each round submits one stream and one identity/round-tagged echo to every adapter before polling. Two scenarios use identical workloads:

- **Fast consumer:** drain the manager event queue after every poll; require zero drops.
- **Held consumer:** poll RPC continuously but retain events until the entire workload finishes; require exactly 32 retained events and explicitly counted drop-oldest overflow.

Every scenario requires `emitted = delivered + dropped`, all correlated responses, valid adapter provenance, bounded observed manager queue length, no remaining pending/completed RPC entries, and all adapters still running. Stop then verifies Stopped diagnostics, absent child PIDs and cleared capability providers. The supervisor separately requires the owned process group to be gone.

Bounds are deliberately small and unchanged across scale: decoded ingress 8, runtime event queue 8, response queue 4, manager event queue 32. There are at most two in-flight requests per adapter. Manager polling drains the runtime event queue; its diagnostic drop counter must remain zero. Ingress occupancy is **not** instrumented: 8 is its configured capacity, not a measured peak. The test never increases capacities to make overflow disappear.

The default Rust regression uses 4 adapters × 32 rounds × 2 scenarios and is included in workspace CI:

```sh
cargo test -p dragonstui-adapter-host --test multi_adapter_stress -- --nocapture
python3 -m unittest discover -s tools -p test_adapter_stress.py -v
```

## CPU / RSS measurement

Run the opt-in release matrix on macOS or Linux with Rust, Python 3 and POSIX `ps`:

```sh
python3 tools/adapter_stress.py --output .hermes/progress/m69-resources.json
```

Defaults: 1, 4 and 8 adapters; 5,000 rounds per scenario; 120-second deadline per case. Overrides are bounded: `--adapters` 1–16, `--rounds` 1–20,000, `--timeout` 1–600. Very short runs fail rather than invent measurements when fewer than two complete process samples are available. Build has a separate 300-second timeout. The runner compiles the exact release test through Cargo's JSON artifact output; it does not guess a test-binary filename or silently use debug code.

The runner samples only the announced test-host PID and its fixture provider PIDs about every 100 ms. CPU is the cumulative `ps time` delta between the first and last complete live sample divided by elapsed sample-window time; 100% means one core, so threaded host CPU may exceed 100%. Linux counters may have one-second granularity. Zombie samples are excluded because macOS resets their CPU/RSS counters to zero. At least two complete samples are mandatory; no missing metric becomes a fabricated zero.

RSS is resident memory in bytes (POSIX `ps` KiB × 1,024), reported separately for host and summed providers with first/last and **sampled** peak values. It is not allocator usage, a lifetime maximum, private memory, or a leak proof. Host measurements include the polling driver, test framework, bounded-by-round-count latency samples and runtime threads. The driver intentionally polls without sleeping to stress throughput. Startup/shutdown may intersect the sample window; CPU is not a full-lifetime total or a per-scenario comparison. RSS may grow due to allocator warmup and retained latency samples without event queues growing.

A new POSIX session owns the test and children. Timeouts/failures terminate only that process group and remove its private temporary root. A JSON report records pass/fail, complete cases, bounded failure logs/resource samples, release test-binary and relevant source SHA-256 hashes. Report creation after build is guaranteed by `finally`; a build failure itself is a nonzero command error, not a measured case. Resource sampling and the larger matrix are opt-in rather than timing-sensitive CI budgets.

## Initial local evidence

Release run on macOS 26.6.2 / arm64, 5,000 rounds per scenario. Both scenarios passed for every scale; 16,770,000 events and 260,000 RPC responses in total. Fast-consumer drops: zero. Held-consumer queue peak/retained count: 32 at every scale. All fixture groups exited.

| Adapters | Fast events/s | Held events/s | Fast RPC p95 (µs) | Host sampled peak RSS (MiB) | Providers sampled peak RSS (MiB) | Host CPU (one-core %) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1,002,409 | 1,021,893 | 144 | 3.02 | 4.17 | 178.3 |
| 4 | 433,645 | 520,081 | 1,799 | 4.30 | 16.59 | 235.8 |
| 8 | 417,083 | 447,003 | 3,222 | 5.27 | 32.56 | 279.0 |

These are a single local observation, not portable thresholds. Lower aggregate throughput at higher adapter counts is visible rather than hidden by tuning. No scheduler/queue redesign is included in M69. A longer soak, large-payload profile, controller/TUI latency, crash-under-load and real-provider behavior require separate evidence.
