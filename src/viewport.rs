use crate::{Frame, Rect, Style};

/// Caller-owned offset and dimensions for [`Viewport`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ViewportState {
    offset: usize,
    content_height: usize,
    viewport_height: usize,
    initialized: bool,
}

impl ViewportState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates content and viewport dimensions, preserving the current position
    /// unless the previous position was at the bottom.
    pub fn update_dimensions(&mut self, content_height: usize, viewport_height: u16) {
        let was_at_bottom = self.initialized && self.is_at_bottom();
        self.content_height = content_height;
        self.viewport_height = usize::from(viewport_height);
        self.initialized = true;

        if was_at_bottom {
            self.offset = self.max_scroll();
        } else {
            self.offset = self.offset.min(self.max_scroll());
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn content_height(&self) -> usize {
        self.content_height
    }

    pub fn viewport_height(&self) -> usize {
        self.viewport_height
    }

    pub fn max_scroll(&self) -> usize {
        self.content_height.saturating_sub(self.viewport_height)
    }

    pub fn is_at_bottom(&self) -> bool {
        self.offset == self.max_scroll()
    }

    pub fn scroll_up(&mut self) -> bool {
        self.set_offset(self.offset.saturating_sub(1))
    }

    pub fn scroll_down(&mut self) -> bool {
        self.set_offset(self.offset.saturating_add(1))
    }

    pub fn page_up(&mut self) -> bool {
        self.set_offset(self.offset.saturating_sub(self.viewport_height))
    }

    pub fn page_down(&mut self) -> bool {
        self.set_offset(self.offset.saturating_add(self.viewport_height))
    }

    pub fn home(&mut self) -> bool {
        self.set_offset(0)
    }

    pub fn end(&mut self) -> bool {
        self.set_offset(self.max_scroll())
    }

    fn set_offset(&mut self, offset: usize) -> bool {
        let offset = offset.min(self.max_scroll());
        if self.offset == offset {
            return false;
        }

        self.offset = offset;
        true
    }
}

/// Borrowed text lines rendered through an explicit [`ViewportState`].
///
/// Rendering updates the state dimensions and writes only the visible range.
pub struct Viewport<'a> {
    lines: &'a [String],
    style: Style,
}

impl<'a> Viewport<'a> {
    pub fn new(lines: &'a [String]) -> Self {
        Self {
            lines,
            style: Style::new(),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect, state: &mut ViewportState) {
        state.update_dimensions(self.lines.len(), rect.height);
        if rect.width == 0 || rect.height == 0 {
            return;
        }

        let start = state.offset();
        let end = start
            .saturating_add(usize::from(rect.height))
            .min(self.lines.len());

        for (row, line) in self.lines[start..end].iter().enumerate() {
            frame.write_text_in(rect, 0, row as u16, line, self.style);
        }
    }
}
