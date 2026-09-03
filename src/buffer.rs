use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{Cell, CellKind, Style};

/// Terminal-sized cell storage used by [`Frame`](crate::Frame) and [`Runtime`](crate::Runtime).
///
/// A buffer preserves wide-character lead/continuation cells and clips writes to its bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Buffer {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

/// One changed cell in deterministic row-major diff order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangedCell {
    pub x: u16,
    pub y: u16,
    pub previous: Option<Cell>,
    pub current: Cell,
}

impl Buffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); usize::from(width) * usize::from(height)],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        self.index(x, y).and_then(|index| self.cells.get(index))
    }

    /// Replaces one in-bounds cell and returns whether the coordinate existed.
    ///
    /// This is a scalar-cell write: it normalizes `cell.kind` to [`CellKind::Normal`] and repairs
    /// any wide-cell partner at the target. Use [`Buffer::write_text`] for width-aware text.
    pub fn set(&mut self, x: u16, y: u16, cell: Cell) -> bool {
        let Some(index) = self.index(x, y) else {
            return false;
        };

        self.clear_cell_and_partner(x, y);
        self.cells[index] = Cell::new(cell.character, cell.style);
        true
    }

    /// Writes by Unicode scalar display width; combining scalars are skipped and
    /// multi-scalar grapheme clusters (such as ZWJ emoji sequences) are not combined.
    pub fn write_text(&mut self, x: u16, y: u16, text: &str, style: Style) -> usize {
        self.write_text_clipped(x, y, text, style, self.width.saturating_sub(x))
    }

    pub(crate) fn write_text_clipped(
        &mut self,
        x: u16,
        y: u16,
        text: &str,
        style: Style,
        max_width: u16,
    ) -> usize {
        if y >= self.height || x >= self.width {
            return 0;
        }

        let available = self.width.saturating_sub(x).min(max_width);
        let mut cursor = x;
        let mut written = 0;

        for character in text.chars() {
            let width = UnicodeWidthChar::width(character).unwrap_or(0);

            if width == 0 {
                continue;
            }

            let width = u16::try_from(width).unwrap_or(u16::MAX).min(2);
            if width > available.saturating_sub(written) {
                break;
            }

            if width == 1 {
                self.set(cursor, y, Cell::new(character, style));
            } else if !self.write_wide(cursor, y, character, style) {
                break;
            }

            cursor += width;
            written += width;
        }

        written as usize
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    fn write_wide(&mut self, x: u16, y: u16, character: char, style: Style) -> bool {
        let Some(continuation_x) = x.checked_add(1) else {
            return false;
        };
        let (Some(index), Some(continuation_index)) =
            (self.index(x, y), self.index(continuation_x, y))
        else {
            return false;
        };

        self.clear_cell_and_partner(x, y);
        self.clear_cell_and_partner(continuation_x, y);
        self.cells[index] = Cell::wide(character, style);
        self.cells[continuation_index] = Cell::continuation(style);
        true
    }

    fn clear_cell_and_partner(&mut self, x: u16, y: u16) {
        let Some(index) = self.index(x, y) else {
            return;
        };

        match self.cells[index].kind {
            CellKind::Normal => self.cells[index] = Cell::default(),
            CellKind::Wide => {
                self.cells[index] = Cell::default();
                if let Some(continuation_index) =
                    x.checked_add(1).and_then(|next_x| self.index(next_x, y))
                {
                    self.cells[continuation_index] = Cell::default();
                }
            }
            CellKind::WideContinuation => {
                self.cells[index] = Cell::default();
                match x
                    .checked_sub(1)
                    .and_then(|previous_x| self.index(previous_x, y))
                {
                    Some(wide_index) if self.cells[wide_index].kind == CellKind::Wide => {
                        self.cells[wide_index] = Cell::default();
                    }
                    _ => {}
                }
            }
        }
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        (x < self.width && y < self.height)
            .then(|| usize::from(y) * usize::from(self.width) + usize::from(x))
    }
}

/// Returns terminal column width from `unicode-width`.
///
/// This is scalar-based: combining marks and complex grapheme clusters are not
/// stored as a single renderable unit in this milestone.
/// Returns terminal column width calculated by `unicode-width`.
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Returns cells that changed from `previous` to `current` in row-major order.
///
/// Passing `None` treats every current cell as changed. A dimension change likewise produces a
/// full current-buffer redraw.
pub fn diff(previous: Option<&Buffer>, current: &Buffer) -> Vec<ChangedCell> {
    let resized = previous
        .is_none_or(|buffer| buffer.width != current.width || buffer.height != current.height);
    let mut changed = Vec::new();

    for y in 0..current.height {
        for x in 0..current.width {
            let current_cell = *current
                .get(x, y)
                .expect("buffer coordinates are valid during diff");
            let previous_cell = previous.and_then(|buffer| buffer.get(x, y)).copied();

            if resized || previous_cell != Some(current_cell) {
                changed.push(ChangedCell {
                    x,
                    y,
                    previous: previous_cell,
                    current: current_cell,
                });
            }
        }
    }

    changed
}
