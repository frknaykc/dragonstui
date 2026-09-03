# DragonsTUI Component Model

## Context

DragonsTUI is an immediate-mode terminal UI library. Each application redraw creates a `Frame`, derives `Rect`s with `Layout`, paints primitives directly into that frame, and hands the resulting buffer to `Runtime` for diff-based terminal output. The demo dashboard follows that model in `src/main.rs`.

M19 assesses whether the current repository has a concrete need for a common component or widget abstraction. This record is deliberately based on the checked-in primitives, tests, and dashboard rather than on a speculative React-like architecture.

## Evidence

### Rendering and layout foundations

| Primitive | Rendering / state evidence | Frame and area contract |
| --- | --- | --- |
| `Frame` | Owns the mutable `Buffer`; it is the immediate drawing target, not a widget. | Created from terminal `Size`; primitives receive `&mut Frame`. |
| `Buffer`, `Cell`, `Style` | `Buffer` owns cells; `Cell` and `Style` are value data. None own UI lifecycle or receive events. | Rendering helpers write cells or text through `Frame`/`Buffer`; there is no separate renderable object. |
| `Rect` | Value geometry with `contains`, `inner`, and saturating edges. | The application or a parent primitive supplies it; it is not stateful. |
| `Layout`, `Constraint` | `Layout::split` deterministically derives child rectangles. `Constraint` is a declarative input value. | They produce `Rect`s before render calls and have no frame or event dependency. |

The foundation is intentionally low-level: direct cell writes, clipping, wide-cell handling, and diffing are not meaningful `Widget` implementations.

### Stateless display and graphics primitives

| Primitive | Actual render contract | State and interaction |
| --- | --- | --- |
| `Text` | `render(&self, &mut Frame, Rect)` (`src/text.rs:36`). | Immutable configured content, style, and alignment; no event state. |
| `Span` | Data rendered by `Line`; it has no standalone render call. | Immutable styled text fragment. |
| `Line` | `render(&self, &mut Frame, Rect)` (`src/rich_text.rs:60`). | Immutable spans and alignment. |
| `RichText` | `render(&self, &mut Frame, Rect)` (`src/rich_text.rs:136`). | Immutable lines and alignment. |
| `Panel` | `render(&self, &mut Frame, Rect) -> Rect` (`src/panel.rs:44`). | Stateless configuration, but its return value is its semantic output: callers use the inner `Rect` for child layout. |
| `Modal` | `render(&self, &mut Frame, parent: Rect) -> Rect` (`src/overlay.rs:86`). | Stateless content/configuration, composes `Panel` and `RichText`, and returns its positioned rectangle. |
| `Canvas` | `render(&self, &mut Frame, Rect, Style)` (`src/canvas.rs:108`). | Owns a mutable braille bitmap via explicit drawing methods; styling is intentionally supplied at render time. |
| `ProgressBar`, `Gauge`, `Sparkline` | Each has `render(&self, &mut Frame, Rect)` (`src/visualization.rs:56`, `:93`, `:118`). | Configured value/sample data only. `ProgressBar` and `Gauge` deliberately share an internal bar helper but remain separate public semantic primitives. |

There is a real common subset of `&self + &mut Frame + Rect` calls. It is not a complete common contract: `Panel` and `Modal` return a child area, `Canvas` accepts a render-time style, and `Span` has no independent visual extent. A trait covering this subset would not remove the composition code that matters.

### Interactive primitives

| Primitive | Render and state contract | Event / state ownership evidence |
| --- | --- | --- |
| `List` | `render(&self, &mut Frame, Rect, &mut ListState)`. | Selection is explicit in `ListState`; callers invoke `next`/`previous`. |
| `Table` | `render(&self, &mut Frame, Rect, &mut TableState)` (`src/table.rs:112`). | `TableState` contains selection and `ViewportState`; caller supplies row count when navigating. |
| `Tree` | `render(&self, &mut Frame, Rect, &mut TreeState)` (`src/tree.rs:198`). | Tree navigation belongs to the tree because it needs tree structure; expanded IDs, selection, and viewport are explicit `TreeState`. |
| `Viewport` | `render(&self, &mut Frame, Rect, &mut ViewportState)` (`src/viewport.rs:104`). | Scroll state is explicit and domain-independent. |
| `TextInput` | `render(&self, &mut Frame, Rect, Style) -> Option<Position>`. | The input owns grapheme cursor/text and exposes `handle_key`; the application decides focus and terminal cursor visibility. |
| `TextArea` | `render(&mut self, &mut Frame, Rect, Style) -> Option<Position>` (`src/text_area.rs:180`). | It owns content, cursor, and render-dependent scroll offsets; `handle_key` is local editing only. |
| `FocusState` | Does not render. | It is an application-level ordered focus registry, not a widget tree. `Dashboard` owns focus IDs and routes input. |
| `CommandPalette` | Renders through `Modal` and a temporary `ListState` (`src/palette.rs:87`). | It owns query/selection and handles query/navigation keys. Dashboard owns opening, Escape/Enter semantics, focus restoration, overlay priority, and command dispatch. |

The stateful render signatures are intentionally different. `List`, `Table`, `Tree`, and `Viewport` use separate reusable state because their data can be rebuilt every frame; `TextInput`, `TextArea`, and `CommandPalette` own edit/query state because it is intrinsic. Their event behavior also has different inputs and outputs: editing returns `bool`, tree navigation requires the tree model, and focus/mouse routing requires application-owned hit regions.

### Runtime and application composition

`Event` is a normalized terminal input enum, `KeyMap` maps an exact key chord to a `CommandId`, `Animation`/`Spinner` advance independently of rendering, and `Runtime` owns terminal polling, diffing, cursor writes, and redraw scheduling. None is a component.

The dashboard is the only application composition site. It:

- derives the root/body/panel rectangles with `Layout`;
- derives styles from the active theme;
- stores hit regions after panels render;
- chooses either `Table` or `Tree` for the agents area;
- owns `FocusState`, command dispatch, mouse hit testing, overlay priority, and cursor visibility; and
- renders overlays last (`Modal` before `CommandPalette`).

These operations are not copies of a reusable child-component pattern. They encode one screen's domain order and policy. `render_canvas_demo` and `render_visualization_demo` are small demo-only helpers, not repeated dashboard sections. `Modal` and `CommandPalette` already demonstrate useful direct composition without a tree or dispatcher.

The repository contains no `Widget`, `StatefulWidget`, component tree, `Box<dyn ...>`, lifecycle, reconciler, or automatic event-bubbling usage. Focused tests cover primitives directly with `Frame` and explicit state, while dashboard tests cover the application routing boundary.

### Repeated patterns and their limits

1. **Immediate frame rendering is repeated intentionally.** It provides a small, predictable call surface and static dispatch without allocation.
2. **`Rect` is supplied explicitly.** This is a necessary composition decision, not duplication: `Panel` needs to return its inner area, overlays need a parent area, and the dashboard determines layout.
3. **Several collection widgets use explicit state.** Their state shapes and navigation semantics differ materially; a generic state trait would hide the data required for correct behavior without sharing event dispatch.
4. **`TextInput` and `TextArea` share grapheme concepts, not a component contract.** A future editing-core extraction would be a focused text-editing decision, not evidence for a universal widget trait.
5. **Dashboard routing is centralized once.** There is no second screen or application with duplicated focus, mouse, overlay, or command-routing code to justify framework ownership.

## Considered Options

### A — No common widget trait

Keep explicit primitives such as `Text::render`, `Panel::render`, `Table::render`, and `Tree::render`.

- **Simplicity and ergonomics:** preserves existing readable, direct calls and return values.
- **Composability:** `Frame` and `Rect` remain the shared low-level seam; composition remains ordinary Rust control flow.
- **Performance and allocation:** static calls and caller-owned state remain the default; no trait objects or boxed tree are introduced.
- **Testability and customization:** primitive tests construct `Frame` plus exactly the state they need; callers can interleave direct cell writes, layout, and primitives.
- **Interaction:** application policy remains explicit where it belongs: focus ordering, mouse hit regions, overlays, command execution, and cursor lifecycle.
- **Cost:** no uniform heterogeneous collection of visual items. The current repository has no evidence that it needs one.

### B — Minimal immediate-mode widget trait

A trait shaped roughly like `render(&self, &mut Frame, Rect)` could cover `Text`, `Line`, `RichText`, selected graphics primitives, and perhaps `Panel` only by discarding its return value. A stateful companion trait would need associated state for collection widgets.

- **Value found:** the trait matches a subset of current calls.
- **Gap:** it does not cover `Span`, `Panel`/`Modal` return values, render-time `Style` parameters, text cursor return values, or the different stateful and event contracts.
- **Ergonomics cost:** an application would still use each concrete API for state and return values, so adding trait bounds would not simplify the dashboard.
- **Compatibility cost:** preserving all useful existing calls would require the trait to be additive, producing two equally valid APIs with no demonstrated consumer.
- **Performance:** static generic dispatch is possible, but dynamic heterogeneous storage would require the `Box<dyn Widget>` path that DragonsTUI does not need.

A minimal trait is therefore not justified now. It would be an unused marker over a partial rendering subset rather than an abstraction that removes demonstrated application duplication.

### C — Retained component model

A retained model would introduce a component tree, lifecycle, automatic event dispatch, focus propagation, and state ownership.

- **Potential benefit:** it could serve a future dynamic multi-screen agent console with independently mounted subtrees.
- **Current mismatch:** DragonsTUI has one direct dashboard composition site, explicit layout, explicit hit regions, and application-owned focus/overlay policy. There is no tree to reconcile or repeated ownership problem to solve.
- **Costs:** lifecycle and event-bubbling rules, state ownership semantics, allocation strategy, tree identity, and cursor/focus propagation would become new framework policy. This would make the current low-level path less direct and impose a much larger learning curve.

This option is not required and is explicitly out of scope for M19.

## Decision

**RESULT A — NO COMPONENT ABSTRACTION ADDED.**

DragonsTUI will continue with explicit immediate-mode primitive APIs. No `Widget`, `StatefulWidget`, component tree, dynamic dispatch requirement, lifecycle, hooks, reconciler, automatic state ownership, or automatic event bubbling is added in M19.

## Why

The repository's shared rendering seam is already concrete and sufficient: `&mut Frame` plus caller-selected `Rect`. The superficial common signature does not include the real variation that current APIs need: return areas/cursor positions, render-time styles, explicit versus intrinsic state, and domain-specific navigation.

The dashboard contains application-specific orchestration, not repeated framework boilerplate. Introducing a trait would preserve that orchestration while adding an unused API layer. A retained model would additionally replace explicit focus and mouse policies before the project has a second composition site that demonstrates the need.

This keeps DragonsTUI fast, Rust-native, predictable, low-level when required, and customizable through ordinary types and direct frame access.

## Consequences

- Existing public APIs remain unchanged.
- Existing call sites retain static dispatch and do not require heap allocation or `Box<dyn ...>`.
- State stays visibly owned either by a dedicated `*State` value or by the editing primitive that requires it.
- Applications continue to own layout composition, focus order, mouse hit regions, command policy, overlay ordering, and terminal cursor policy.
- New primitives should use an explicit render signature that exposes the state, style, and return values their semantics require. They should not implement a common trait merely because they paint cells.

## Deferred

Reassess this decision only when repository evidence shows a trait removes real duplicated application code. Concrete triggers include:

1. two or more independently implemented screens repeat the same child composition and rendering orchestration;
2. a real requirement needs a heterogeneous collection of render-only primitives, and the loss of return values/render-time parameters has a documented solution;
3. at least two stateful primitives share the same state-render-event contract and a focused prototype reduces, rather than relocates, application code; or
4. an agent-console requirement needs dynamic mounting/unmounting, cross-screen focus propagation, or scoped event routing that explicit application code demonstrably cannot express cleanly.

Any future proposal must first benchmark allocation/dispatch impact, preserve a direct low-level `Frame` path, specify focus/mouse/cursor ownership, add representative compile and application tests, and compare its code against the explicit alternative. A retained component tree is a separate architecture decision, not an automatic next step.
