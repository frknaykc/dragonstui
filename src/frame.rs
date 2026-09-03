use crate::{BorderSet, Buffer, Cell, Rect, Style};

/// Mutable, Rect-aware facade over one render [`Buffer`](crate::Buffer).
///
/// Applications construct a frame per redraw and pass it to explicit primitive `render` methods.
/// Frame writes clip to the buffer and to the supplied target rectangle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    buffer: Buffer,
}

impl Frame {
    /// Creates a blank frame with the requested terminal dimensions.
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            buffer: Buffer::new(width, height),
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn set_cell(&mut self, x: u16, y: u16, cell: Cell) -> bool {
        self.buffer.set(x, y, cell)
    }

    pub fn write_text(&mut self, x: u16, y: u16, text: &str, style: Style) -> usize {
        self.buffer.write_text(x, y, text, style)
    }

    pub fn write_text_in(&mut self, rect: Rect, x: u16, y: u16, text: &str, style: Style) -> usize {
        if x >= rect.width || y >= rect.height {
            return 0;
        }
        let (Some(x), Some(y)) = (rect.x.checked_add(x), rect.y.checked_add(y)) else {
            return 0;
        };

        self.buffer
            .write_text_clipped(x, y, text, style, rect.width - x.saturating_sub(rect.x))
    }

    pub fn draw_border(&mut self, rect: Rect, style: Style) {
        self.draw_border_with_set(rect, style, BorderSet::default());
    }

    pub fn draw_border_with_set(&mut self, rect: Rect, style: Style, border_set: BorderSet) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }

        self.set_rect_cell(rect, 0, 0, border_set.top_left, style);

        if rect.width > 1 {
            for x in 1..rect.width - 1 {
                self.set_rect_cell(rect, x, 0, border_set.horizontal, style);
            }
            self.set_rect_cell(rect, rect.width - 1, 0, border_set.top_right, style);
        }

        if rect.height == 1 {
            return;
        }

        for y in 1..rect.height - 1 {
            self.set_rect_cell(rect, 0, y, border_set.vertical, style);
            if rect.width > 1 {
                self.set_rect_cell(rect, rect.width - 1, y, border_set.vertical, style);
            }
        }

        self.set_rect_cell(rect, 0, rect.height - 1, border_set.bottom_left, style);
        if rect.width > 1 {
            for x in 1..rect.width - 1 {
                self.set_rect_cell(rect, x, rect.height - 1, border_set.horizontal, style);
            }
            self.set_rect_cell(
                rect,
                rect.width - 1,
                rect.height - 1,
                border_set.bottom_right,
                style,
            );
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn into_buffer(self) -> Buffer {
        self.buffer
    }

    fn set_rect_cell(&mut self, rect: Rect, x: u16, y: u16, character: char, style: Style) {
        if let (Some(x), Some(y)) = (rect.x.checked_add(x), rect.y.checked_add(y)) {
            self.set_cell(x, y, Cell::new(character, style));
        }
    }
}
