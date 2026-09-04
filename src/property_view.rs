use crate::{Frame, Rect, Style, ViewportState, display_width};

/// One generic, read-only property row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyRow {
    pub label: String,
    pub value: String,
}

impl PropertyRow {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// A borrowed key/value detail view with caller-owned vertical scroll state.
pub struct PropertyView<'a> {
    rows: &'a [PropertyRow],
}

impl<'a> PropertyView<'a> {
    pub fn new(rows: &'a [PropertyRow]) -> Self {
        Self { rows }
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        rect: Rect,
        viewport: &mut ViewportState,
        label_style: Style,
        value_style: Style,
    ) {
        viewport.update_dimensions(self.rows.len(), rect.height);
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let label_width = self
            .rows
            .iter()
            .map(|row| display_width(&row.label))
            .max()
            .unwrap_or(0)
            .min(usize::from(rect.width.saturating_sub(1)));
        for (visible_row, property) in self.rows[viewport.offset()..]
            .iter()
            .take(usize::from(rect.height))
            .enumerate()
        {
            let y = u16::try_from(visible_row).unwrap_or(u16::MAX);
            let value_x = u16::try_from(label_width.saturating_add(1)).unwrap_or(rect.width);
            frame.write_text_in(rect, 0, y, &property.label, label_style);
            frame.write_text_in(rect, value_x, y, &property.value, value_style);
        }
    }
}
