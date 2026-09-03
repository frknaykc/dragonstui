use std::time::Instant;

use crate::Size;

/// Modifier flags normalized from terminal input.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Keyboard keys supported by the terminal-independent event model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// A normalized key press and its modifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyEvent {
    pub const fn character(character: char) -> Self {
        Self {
            code: KeyCode::Char(character),
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
        }
    }
}

/// Mouse buttons represented by [`MouseEvent`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Supported normalized mouse actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseKind {
    LeftDown,
    LeftUp,
    RightDown,
    RightUp,
    MiddleDown,
    MiddleUp,
    ScrollUp,
    ScrollDown,
    Move,
    Drag(MouseButton),
}

/// A zero-based terminal mouse event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MouseEvent {
    pub x: u16,
    pub y: u16,
    pub kind: MouseKind,
    pub modifiers: KeyModifiers,
}

/// Input, resize, or application-tick event; applications own routing policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(Size),
    Tick(Instant),
}
