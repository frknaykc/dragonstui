use crate::{BorderSet, Frame, Rect, Style, Text};

/// Bordered display container that returns its inner rectangle for caller composition.
///
/// A panel is not a component tree node: callers render child primitives into the returned area.
pub struct Panel {
    title: Option<String>,
    border_style: Style,
    border_set: BorderSet,
    title_style: Style,
}

impl Panel {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            border_style: Style::new(),
            border_set: BorderSet::default(),
            title_style: Style::new(),
        }
    }

    pub fn untitled() -> Self {
        Self {
            title: None,
            border_style: Style::new(),
            border_set: BorderSet::default(),
            title_style: Style::new(),
        }
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    pub fn border_set(mut self, border_set: BorderSet) -> Self {
        self.border_set = border_set;
        self
    }

    pub fn title_style(mut self, style: Style) -> Self {
        self.title_style = style;
        self
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect) -> Rect {
        if rect.width == 0 || rect.height == 0 {
            return rect.inner();
        }

        frame.draw_border_with_set(rect, self.border_style, self.border_set);

        if let Some(title) = &self.title {
            Text::new(format!(" {title}"))
                .style(self.title_style)
                .render(
                    frame,
                    Rect::new(
                        rect.x.saturating_add(2),
                        rect.y,
                        rect.width.saturating_sub(3),
                        1,
                    ),
                );
        }

        rect.inner()
    }
}
