# DragonsTUI Performance Measurements

## Scope

These numbers describe one local machine and are not cross-machine claims. They measure the current immediate-mode pipeline:

```text
application data → Frame / Buffer → diff → changed-cell terminal encoding
```

The M19 decision remains unchanged: no widget tree, retained component model, dynamic dispatch requirement, allocator pool, or unsafe code was added for performance work.

## Environment

| Property | Value |
| --- | --- |
| Operating system | macOS 26.6.2 (25G83) |
| Architecture | arm64 |
| CPU | Apple M4 Max |
| Rust | rustc 1.95.0 (LLVM 22.1.2) |
| Cargo | 1.95.0 |
| Build | `cargo run --release --bin dragonstui_measure` |
| Timing | `std::time::Instant`, seven samples per case, median reported |
| Terminal writer | in-memory `Vec<u8>`; no terminal emulator, I/O, or network latency |

The reproducible harness is `src/bin/dragonstui_measure.rs`. Run `cargo run --release --bin dragonstui_measure` for measurements or append `-- --list` to inspect its scenario groups. It uses `std::hint::black_box`, bounded scenario-specific iteration counts, and reports nanoseconds per operation.

## Scenarios

The harness covers 80×24, 120×40, 200×60, and 300×100 where a full size comparison is useful:

- buffer construction and clear; frame construction;
- identical, one-cell, sparse (5%), and full-frame diffing;
- in-memory terminal encoding for one-cell, sparse, and full changed-cell sets;
- layout; plain `Text`; `RichText`; grapheme-heavy `TextArea` rendering;
- `Table`, visible `Tree`, `Viewport`, `Canvas`, and `Sparkline` rendering;
- repeated streaming append plus viewport redraw; and
- representative 10 FPS and 20 FPS animation state updates.

Render construction, diffing, and terminal encoding are timed as separate operations where practical. The streaming measurement intentionally includes 64 append-and-viewport-render updates; it is a batch cost, not a live FPS claim.

## Baseline

All values below are median ns/op from the pre-optimization release run.

| Scenario | 80×24 | 120×40 | 200×60 | 300×100 |
| --- | ---: | ---: | ---: | ---: |
| Buffer construction | 2,413 | 3,050 | 8,228 | 20,806 |
| Buffer clear | 1,511 | 2,933 | 8,539 | 21,408 |
| Frame creation | 1,418 | 3,017 | 8,318 | 20,710 |
| Diff, unchanged | 6,057 | 15,589 | 37,692 | 94,315 |
| Diff, one cell | 6,107 | 15,469 | 38,264 | 94,331 |
| Diff, sparse | 6,959 | 17,071 | 40,529 | 105,962 |
| Diff, full | 11,502 | 26,618 | 63,663 | 167,568 |
| Encode, one cell | 124 | 123 | 108 | 119 |
| Encode, sparse | 3,293 | 7,479 | 18,587 | 45,834 |
| **Encode, full** | **58,080** | **141,816** | **354,147** | **895,311** |
| Layout | 43 | 46 | 44 | 44 |
| Plain text render | 1,438 | 3,132 | 8,561 | 20,993 |
| Rich text render | 1,629 | 3,548 | 8,893 | 20,935 |
| Grapheme-heavy TextArea render | 7,190 | 9,000 | 14,740 | 25,997 |
| Table render | 9,453 | 16,071 | 29,053 | 53,099 |
| Tree render | 3,811 | 5,657 | 10,894 | 23,151 |
| Viewport render | 3,594 | 7,007 | 14,638 | 29,976 |
| Canvas render | 4,962 | 12,518 | 32,846 | 82,001 |
| Sparkline render | 1,490 | 3,237 | 8,927 | 22,189 |

At 120×40, the streaming batch measured 262,142 ns/op; animation-state batches measured 358 ns/op at 10 FPS and 323 ns/op at 20 FPS.

## Bottleneck Analysis

The dominant demonstrated issue was full-frame terminal encoding, not layout or one-cell updates. At 300×100, a full diff took 167,568 ns/op while encoding its 30,000 cells took 895,311 ns/op. The previous renderer emitted a `MoveTo` command before every visible changed cell, including adjacent cells in the same row.

Sparse encoding did not show a reliable large improvement opportunity: changed cells are intentionally separated by unchanged cells in that scenario. Identical-frame and one-cell diffing remain linear scans, but their measured absolute costs did not justify a more complicated dirty-region or retained-tree design. Canvas and streaming costs scale with deliberate work/data allocation, but no measurement isolated a framework-level reuse change that would be simpler than the current direct API.

## Optimization Performed

`render_changed_cells` now tracks the expected terminal position after each visible cell:

- adjacent changed cells in the same row are emitted without another cursor move;
- style transitions still reset/apply styles before the next printed character;
- wide lead cells advance the expected coordinate by two columns;
- wide continuations remain unprinted;
- gaps, row changes, and coordinate-overflow cases retain an explicit `MoveTo`.

This is a local encoder change. It introduces no public API, heap requirement, unsafe code, or change to `Frame`, `Buffer`, diff ordering, clipping, style semantics, or M19's immediate-mode model.

Focused regressions verify adjacent style boundaries and wide-cell coalescing, while existing tests retain RGB, attribute-reset, cursor, and wide-continuation coverage.

## Before / After: Full In-Memory Encoding

| Terminal size | Before ns/op | After ns/op | Reduction | Speedup |
| --- | ---: | ---: | ---: | ---: |
| 80×24 | 58,080 | 17,862 | 69.2% | 3.25× |
| 120×40 | 141,816 | 43,700 | 69.2% | 3.25× |
| 200×60 | 354,147 | 108,083 | 69.5% | 3.28× |
| 300×100 | 895,311 | 270,423 | 69.8% | 3.31× |

Post-optimization measurements for the remaining scenarios stayed in the same order of magnitude; expected timing noise is normal for short wall-clock samples. Full-frame encoding is now materially cheaper while the actual terminal remains the ultimate throughput constraint.

## Intentionally Rejected

- **Buffer object pools or frame reuse:** construction/clear costs did not justify extra ownership/lifetime machinery.
- **Dirty-region or retained component tree:** unchanged and one-cell diff costs alone do not justify invalidation architecture, and M19 explicitly rejects speculative retained UI state.
- **Unsafe buffer/diff code:** no measured need outweighs the correctness risk.
- **ANSI run/string builder redesign:** cursor coalescing solved the measured dominant cost without duplicating Crossterm's style/Unicode behavior.
- **Canvas-specific reuse:** no measured framework bug or allocation path established a simpler safe optimization.

## Residual Risks

- Results exclude terminal emulator, PTY transport, remote-session, and font-rendering costs; real terminal throughput may dominate further.
- `diff` remains an O(width × height) scan by design.
- Rendering rich Unicode and `TextArea` currently performs allocations inherent to their existing APIs; this milestone did not find a measured, low-complexity extraction that preserves their semantics.
- The harness is a regression-oriented local measurement tool, not a statistical benchmarking framework or universal performance certification.
