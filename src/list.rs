use crate::{Frame, Rect, Style, Text};

/// Caller-owned selected-index state for [`List`].
///
/// Navigation clamps safely when the backing item count changes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ListState {
    selected: usize,
}

impl ListState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_selected(&mut self, selected: usize) {
        self.selected = selected;
    }

    pub fn selected_index(&mut self, item_count: usize) -> Option<usize> {
        if item_count == 0 {
            self.selected = 0;
            return None;
        }

        self.selected = self.selected.min(item_count - 1);
        Some(self.selected)
    }

    pub fn next(&mut self, item_count: usize) {
        if let Some(selected) = self.selected_index(item_count) {
            self.selected = (selected + 1) % item_count;
        }
    }

    pub fn previous(&mut self, item_count: usize) {
        if let Some(selected) = self.selected_index(item_count) {
            self.selected = (selected + item_count - 1) % item_count;
        }
    }
}

/// A stateless-in-use selectable string list with explicit [`ListState`].
///
/// The list stores its display items; applications own selection and route input to that state.
pub struct List {
    items: Vec<String>,
    normal_style: Style,
    selected_style: Style,
}

impl List {
    pub fn new(items: &[&str]) -> Self {
        Self {
            items: items.iter().map(|item| (*item).to_owned()).collect(),
            normal_style: Style::new(),
            selected_style: Style::new(),
        }
    }

    pub fn normal_style(mut self, style: Style) -> Self {
        self.normal_style = style;
        self
    }

    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect, state: &mut ListState) {
        let selected = state.selected_index(self.items.len());

        for (row, item) in self.items.iter().take(usize::from(rect.height)).enumerate() {
            let is_selected = selected == Some(row);
            let marker = if is_selected { "> " } else { "  " };
            let style = if is_selected {
                self.selected_style
            } else {
                self.normal_style
            };
            let Some(y) = rect.y.checked_add(row as u16) else {
                break;
            };

            Text::new(format!("{marker}{item}"))
                .style(style)
                .render(frame, Rect::new(rect.x, y, rect.width, 1));
        }
    }
}
