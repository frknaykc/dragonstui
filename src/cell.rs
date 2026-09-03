use crate::Style;

/// Storage role of a terminal cell.
///
/// [`CellKind::WideContinuation`] is the second column of a wide glyph and is never emitted as
/// an independent terminal character.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CellKind {
    #[default]
    Normal,
    Wide,
    WideContinuation,
}

/// A styled terminal character with wide-glyph metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub character: char,
    pub style: Style,
    pub kind: CellKind,
}

impl Cell {
    pub fn new(character: char, style: Style) -> Self {
        Self {
            character,
            style,
            kind: CellKind::Normal,
        }
    }

    pub(crate) fn wide(character: char, style: Style) -> Self {
        Self {
            character,
            style,
            kind: CellKind::Wide,
        }
    }

    pub(crate) fn continuation(style: Style) -> Self {
        Self {
            character: ' ',
            style,
            kind: CellKind::WideContinuation,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::new(' ', Style::default())
    }
}
