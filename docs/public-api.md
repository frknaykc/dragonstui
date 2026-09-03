# Public API Guide

## Scope and Stability

DragonsTUI is at package version `0.1.0`. This document records the current, exercised public surface; it is not a SemVer 1.0 stability promise. M22 intentionally does not redesign, rename, hide, or add broad compatibility shims for APIs that are already used by tests, examples, and the dashboard.

The API is re-exported from the crate root (`dragons_tui::*`). Internal modules remain private so consumers use one discoverable namespace.

## Rendering Model

```text
application state
    ↓
Layout derives Rect values
    ↓
explicit primitive render calls
    ↓
Frame
    ↓
Buffer
    ↓
diff(previous, current)
    ↓
render_changed_cells / Runtime
    ↓
terminal
```

This is explicit immediate-mode Rust. There is no `Widget` trait, retained component tree, virtual DOM, automatic lifecycle, automatic event bubbling, or framework-owned application state. The M19 rationale is in [`architecture/component-model.md`](architecture/component-model.md).

## API Categories

| Category | Public API | State and rendering convention |
| --- | --- | --- |
| Foundation | `Buffer`, `ChangedCell`, `diff`, `display_width`, `Cell`, `CellKind`, `Frame`, `Color`, `Attributes`, `Style` | Construct a `Frame` each redraw, render into it, then consume it through `Runtime` or diff/terminal functions. `Buffer` preserves wide-cell invariants. |
| Geometry and layout | `Position`, `Size`, `Rect`, `split_horizontal`, `split_vertical`, `Direction`, `Constraint`, `Layout` | Geometry values are plain copied data. `Layout::split` derives child rectangles; callers decide composition. |
| Display | `Alignment`, `Text`, `Span`, `Line`, `RichText`, `BorderSet`, `Panel`, `Theme` | Stateless value renderers use `render(&self, &mut Frame, Rect)`. `Panel::render` returns its inner `Rect` so callers render children explicitly. |
| Collections | `List`, `ListState`, `Table`, `TableColumn`, `TableState`, `Tree`, `TreeNode`, `TreeState`, `Viewport`, `ViewportState` | Collection data is stored in the primitive. Selection, expansion, and scrolling are caller-owned state passed as `&mut` to `render`. Tree IDs must be stable and unique within a tree. |
| Editors | `TextInput`, `InputViewport`, `TextArea` | Editors own their local text/cursor/scroll state. `handle_key` accepts normalized `KeyEvent`; `render` returns an optional cursor position for the application to apply. |
| Overlay and commands | `Modal`, `centered_rect`, `centered_percent_rect`, `CommandPalette`, `PaletteCommand`, `CommandId`, `KeyBinding`, `KeyMap`, `FocusId`, `FocusState` | Overlay open/close, focus isolation, mouse hit testing, and command execution remain application policy. `CommandPalette` composes the existing modal/input/list APIs. |
| Graphics and visualization | `Canvas`, `ProgressBar`, `Gauge`, `Sparkline` | `Canvas` owns Braille dots. The visualization types are rendered values. All write explicitly to a frame rectangle. |
| Runtime and terminal | `Event`, `KeyCode`, `KeyEvent`, `KeyModifiers`, `MouseButton`, `MouseEvent`, `MouseKind`, `Animation`, `Spinner`, `Runtime`, `tick_due`, `terminal_size`, `normalize_crossterm_event`, `render_changed_cells`, `set_cursor`, `is_quit_key` | `Runtime` retains only the previous framebuffer for diffing. Applications own event routing, redraw policy beyond `request_redraw`, and terminal guard/lifecycle policy. |

## Construction and Naming Conventions

- `new(...)` constructs a value with required data (`Frame`, `Text`, `Table`, `Tree`, `Canvas`, `Animation`, and others).
- `Default`/`new()` initializes empty caller-owned state types (`ListState`, `TableState`, `TreeState`, `ViewportState`, `TextInput`, `TextArea`, `KeyMap`).
- Value builders consume and return `Self` for style and optional configuration (`style`, `alignment`, `selected_style`, `label`, `gap`, `frame_duration`).
- `render` writes into a supplied `Frame` and target `Rect`; it does not allocate a UI node or mutate global framework state.
- `handle_key` returns whether a primitive consumed the normalized key. Applications decide which primitive receives a key.
- State getters normalize/clamp against current data where required (`selected_index`, collection viewport state, tree normalization).

The signatures intentionally differ where semantics differ: `Panel` and `Modal` return positioned areas; editors return cursor positions; collection primitives take heterogeneous explicit state; `Canvas::render` takes a render-time style. M22 preserves these meaningful distinctions instead of forcing a common trait.

## Prelude Decision

No `dragonstui::prelude::*` is added. The six representative examples import small, concept-specific sets of root exports. A blanket prelude would make state ownership and rendering dependencies less visible without removing meaningful boilerplate. Revisit only if multiple real consumer applications exhibit repeated, stable import groups.

## Public API Changes in M22

- Added crate-level documentation describing the immediate-mode pipeline and M19 boundary.
- Added focused rustdoc to core frame/buffer/cell/style/geometry/layout/display, interaction, overlay, graphics, event, runtime, focus, command, and theme types.
- Added six runnable examples under `examples/`.
- Added no new framework runtime behavior, public traits, error hierarchy, dependency, or breaking rename.

## Errors

Most rendering primitives are intentionally infallible and clip safely. Fallible terminal-facing APIs return `std::io::Result` directly:

- `terminal_size`
- `render_changed_cells`
- `set_cursor`
- `Runtime::next_event`
- `Runtime::render` / `Runtime::render_with_cursor`

There is no framework-wide `Error` enum because no current fallible path needs cross-domain error translation.

## Library and Binary Boundary

`src/lib.rs` exports framework primitives. The existing `dragons_tui` binary remains an application/dashboard: its agent/process mock data, terminal guard, layout, focus ordering, hit regions, overlay priority, and command routing are not framework APIs. The library examples consume only root exports and normal Rust dependencies; they do not require Hermes, another AI agent, network access, API keys, or an external process.

## Cargo Metadata Audit

`Cargo.toml` declares the package name, version, edition, description, MIT license, canonical repository URL, keywords, categories, dependencies, and `default-run = "dragons_tui"`. The default run setting preserves the pre-M20 `cargo run` dashboard behavior after adding the measurement binary.

## Deferred Stabilization

- SemVer 1.0 compatibility guarantees.
- Terminal capability/fallback API: documented as unnecessary without a consumer in [`terminal-compatibility.md`](terminal-compatibility.md).
- OSC 8 hyperlinks and non-RGB color fallback.
- A prelude, only if real consumer import patterns justify it.
- New component/widget abstractions, unless future repository evidence invalidates M19.
