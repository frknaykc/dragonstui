use crate::{Frame, Rect, Style, display_width};

/// Horizontal alignment for text and rich text within a target rectangle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Stateless plain text rendered directly into a [`Frame`](crate::Frame).
///
/// Newlines occupy rows in the target rectangle; content clips instead of wrapping.
pub struct Text {
    content: String,
    style: Style,
    alignment: Alignment,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            style: Style::new(),
            alignment: Alignment::Left,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect) {
        for (row, line) in self
            .content
            .lines()
            .take(usize::from(rect.height))
            .enumerate()
        {
            let width = u16::try_from(display_width(line)).unwrap_or(u16::MAX);
            let offset = match self.alignment {
                Alignment::Left => 0,
                Alignment::Center => rect.width.saturating_sub(width) / 2,
                Alignment::Right => rect.width.saturating_sub(width),
            };

            frame.write_text_in(rect, offset, row as u16, line, self.style);
        }
    }
}
