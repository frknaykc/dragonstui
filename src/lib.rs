//! DragonsTUI is an explicit immediate-mode terminal UI library.
//!
//! Applications own state, derive [`Rect`] values with [`Layout`], and render primitives
//! directly into a [`Frame`]. [`Runtime`] diffs the resulting [`Buffer`] and encodes changed
//! cells for a terminal writer.
//!
//! ```text
//! application state → layout → explicit primitives → Frame → Buffer → diff → terminal
//! ```
//!
//! There is intentionally no component tree, virtual DOM, automatic state ownership, or
//! automatic event bubbling. Stateless values such as [`Text`] render directly; collection
//! primitives receive caller-owned state explicitly; editors own local editing state and return
//! cursor positions when rendered. This preserves direct control over focus, mouse hit testing,
//! and composition. See `docs/architecture/component-model.md` in the repository for M19.

mod animation;
mod border;
mod buffer;
mod canvas;
mod cell;
mod event;
mod focus;
mod frame;
mod geometry;
mod inspector;
mod keymap;
mod layout;
mod list;
mod overlay;
mod palette;
mod panel;
mod rich_text;
mod runtime;
mod scrollbar;
mod shutdown;
mod spinner;
mod style;
mod table;
mod terminal;
mod text;
mod text_area;
mod text_input;
mod theme;
mod tree;
mod viewport;
mod visualization;

pub use animation::Animation;
pub use border::BorderSet;
pub use buffer::{Buffer, ChangedCell, diff, display_width};
pub use canvas::Canvas;
pub use cell::{Cell, CellKind};
pub use event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseKind};
pub use focus::{FocusId, FocusState};
pub use frame::Frame;
pub use geometry::{Position, Rect, Size, split_horizontal, split_vertical};
pub use inspector::{InspectorAreas, InspectorLayout, InspectorSplitState};
pub use keymap::{CommandId, KeyBinding, KeyMap};
pub use layout::{Constraint, Direction, Layout};
pub use list::{List, ListState};
pub use overlay::{Modal, centered_percent_rect, centered_rect};
pub use palette::{CommandPalette, PaletteCommand};
pub use panel::Panel;
pub use rich_text::{Line, RichText, Span};
pub use runtime::{Runtime, tick_due};
pub use scrollbar::{Scrollbar, ScrollbarGeometry, ScrollbarState};
pub use shutdown::ShutdownSignal;
pub use spinner::Spinner;
pub use style::{Attributes, Color, Style};
pub use table::{Table, TableColumn, TableState};
pub use terminal::{normalize_crossterm_event, render_changed_cells, set_cursor, terminal_size};
pub use text::{Alignment, Text};
pub use text_area::TextArea;
pub use text_input::{InputViewport, TextInput};
pub use theme::Theme;
pub use tree::{Tree, TreeNode, TreeState};
pub use viewport::{Viewport, ViewportState};
pub use visualization::{Gauge, ProgressBar, Sparkline};

/// Returns whether `character` is the conventional application-level quit shortcut.
///
/// Applications own event routing and may choose a different quit policy.
pub fn is_quit_key(character: char) -> bool {
    character == 'q'
}
