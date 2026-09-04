use crate::{Frame, Line, Rect, Scrollbar, ScrollbarGeometry, Style, ViewportState};

/// Immutable, producer-styled source lines. The viewer never infers a language or token meaning.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceDocument {
    lines: Vec<Line>,
}

impl SourceDocument {
    pub fn new(lines: impl IntoIterator<Item = Line>) -> Self {
        Self {
            lines: lines.into_iter().collect(),
        }
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }
}

/// Read-only source rendering over a caller-owned shared vertical viewport.
pub struct SourceViewer<'a> {
    document: &'a SourceDocument,
}

impl<'a> SourceViewer<'a> {
    pub const fn new(document: &'a SourceDocument) -> Self {
        Self { document }
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        rect: Rect,
        viewport: &mut ViewportState,
        gutter_style: Style,
    ) {
        viewport.update_dimensions(self.document.lines.len(), rect.height);
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        if self.document.lines.is_empty() {
            frame.write_text_in(rect, 0, 0, "(empty source)", gutter_style);
            return;
        }

        let digits = self.document.lines.len().to_string().len().max(1);
        let gutter = u16::try_from(digits.saturating_add(1)).unwrap_or(u16::MAX);
        for (index, line) in self
            .document
            .lines
            .iter()
            .enumerate()
            .skip(viewport.offset())
            .take(usize::from(rect.height))
        {
            let row = u16::try_from(index.saturating_sub(viewport.offset())).unwrap_or(u16::MAX);
            let number = format!("{:>digits$} ", index.saturating_add(1), digits = digits);
            frame.write_text_in(rect, 0, row, &number, gutter_style);
            line.render(
                frame,
                Rect::new(
                    rect.x.saturating_add(gutter),
                    rect.y.saturating_add(row),
                    rect.width.saturating_sub(gutter),
                    1,
                ),
            );
        }
    }

    /// Renders source while reserving its final column for the shared scrollbar.
    pub fn render_with_scrollbar(
        &self,
        frame: &mut Frame,
        rect: Rect,
        viewport: &mut ViewportState,
        gutter_style: Style,
        track_style: Style,
        thumb_style: Style,
    ) -> Option<ScrollbarGeometry> {
        if rect.width == 0 {
            self.render(frame, rect, viewport, gutter_style);
            return None;
        }
        let content = Rect::new(rect.x, rect.y, rect.width.saturating_sub(1), rect.height);
        self.render(frame, content, viewport, gutter_style);
        Scrollbar::render(
            frame,
            viewport,
            Rect::new(
                rect.x.saturating_add(rect.width.saturating_sub(1)),
                rect.y,
                1,
                rect.height,
            ),
            track_style,
            thumb_style,
        )
    }
}
