use crate::{Frame, Line, Panel, Rect, RichText, Span, Style};

/// Returns a requested-size rectangle centered and clamped within `parent`.
pub fn centered_rect(parent: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(parent.width);
    let height = height.min(parent.height);
    Rect::new(
        parent
            .x
            .saturating_add(parent.width.saturating_sub(width) / 2),
        parent
            .y
            .saturating_add(parent.height.saturating_sub(height) / 2),
        width,
        height,
    )
}

/// Returns a centered rectangle sized by clamped percentages of `parent`.
pub fn centered_percent_rect(parent: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let width = u16::try_from(u32::from(parent.width) * u32::from(width_percent.min(100)) / 100)
        .unwrap_or(parent.width);
    let height = u16::try_from(u32::from(parent.height) * u32::from(height_percent.min(100)) / 100)
        .unwrap_or(parent.height);
    centered_rect(parent, width, height)
}

/// Stateless-in-use centered overlay rendered over an application-owned base frame.
///
/// Applications own whether it is open, focus isolation, and event routing; rendering returns the
/// positioned rectangle for optional composition.
pub struct Modal {
    title: String,
    lines: Vec<Line>,
    width: u16,
    height: u16,
    border_style: Style,
    title_style: Style,
    content_style: Style,
}

impl Modal {
    pub fn new(title: impl Into<String>, lines: impl IntoIterator<Item = Line>) -> Self {
        Self {
            title: title.into(),
            lines: lines.into_iter().collect(),
            width: 0,
            height: 0,
            border_style: Style::new(),
            title_style: Style::new(),
            content_style: Style::new(),
        }
    }

    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    pub fn title_style(mut self, style: Style) -> Self {
        self.title_style = style;
        self
    }

    pub fn content_style(mut self, style: Style) -> Self {
        self.content_style = style;
        self
    }

    pub fn rect(&self, parent: Rect) -> Rect {
        centered_rect(
            parent,
            if self.width == 0 {
                parent.width
            } else {
                self.width
            },
            if self.height == 0 {
                parent.height
            } else {
                self.height
            },
        )
    }

    pub fn render(&self, frame: &mut Frame, parent: Rect) -> Rect {
        let rect = self.rect(parent);
        let inner = Panel::new(&self.title)
            .border_style(self.border_style)
            .title_style(self.title_style)
            .render(frame, rect);
        let lines =
            self.lines.iter().map(|line| {
                Line::new(line.spans().iter().map(|span| {
                    Span::styled(span.content(), span.style().patch(self.content_style))
                }))
            });
        RichText::new(lines).render(frame, inner);
        rect
    }
}
