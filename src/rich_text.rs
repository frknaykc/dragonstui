use crate::{Alignment, Frame, Rect, Style, display_width};

/// One independently styled segment within a [`Line`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Span {
    content: String,
    style: Style,
}

impl Span {
    pub fn raw(content: impl Into<String>) -> Self {
        Self::styled(content, Style::new())
    }

    pub fn styled(content: impl Into<String>, style: Style) -> Self {
        Self {
            content: content.into(),
            style,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn style(&self) -> Style {
        self.style
    }
}

/// An ordered row of [`Span`] values rendered into a [`Frame`](crate::Frame).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Line {
    spans: Vec<Span>,
    alignment: Alignment,
}

impl Line {
    pub fn new(spans: impl IntoIterator<Item = Span>) -> Self {
        Self {
            spans: spans.into_iter().collect(),
            alignment: Alignment::Left,
        }
    }

    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    pub fn display_width(&self) -> usize {
        self.spans
            .iter()
            .map(|span| display_width(span.content()))
            .sum()
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect) {
        self.render_at(frame, rect, 0, self.alignment);
    }

    fn render_at(&self, frame: &mut Frame, rect: Rect, row: u16, alignment: Alignment) {
        if rect.width == 0 || row >= rect.height {
            return;
        }

        let width = u16::try_from(self.display_width()).unwrap_or(u16::MAX);
        let mut column = match alignment {
            Alignment::Left => 0,
            Alignment::Center => rect.width.saturating_sub(width) / 2,
            Alignment::Right => rect.width.saturating_sub(width),
        };

        for span in &self.spans {
            let available = rect.width.saturating_sub(column);
            if available == 0 {
                break;
            }

            let span_width = u16::try_from(display_width(span.content())).unwrap_or(u16::MAX);
            let written =
                u16::try_from(frame.write_text_in(rect, column, row, span.content(), span.style()))
                    .unwrap_or(u16::MAX);
            column = column.saturating_add(written);

            if written < span_width.min(available) {
                break;
            }
        }
    }
}

impl<const N: usize> From<[Span; N]> for Line {
    fn from(spans: [Span; N]) -> Self {
        Self::new(spans)
    }
}

impl From<&str> for Line {
    fn from(content: &str) -> Self {
        Self::new([Span::raw(content)])
    }
}

impl From<String> for Line {
    fn from(content: String) -> Self {
        Self::new([Span::raw(content)])
    }
}

/// Stateless multi-line styled display content.
///
/// Lines clip to the target rectangle. Span styles remain independent unless callers explicitly
/// patch them while composing a line.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RichText {
    lines: Vec<Line>,
    alignment: Alignment,
}

impl RichText {
    pub fn new(lines: impl IntoIterator<Item = Line>) -> Self {
        Self {
            lines: lines.into_iter().collect(),
            alignment: Alignment::Left,
        }
    }

    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect) {
        for (row, line) in self.lines.iter().take(usize::from(rect.height)).enumerate() {
            line.render_at(frame, rect, row as u16, self.alignment);
        }
    }
}

impl<const N: usize> From<[Line; N]> for RichText {
    fn from(lines: [Line; N]) -> Self {
        Self::new(lines)
    }
}
