use crate::{Alignment, Cell, Constraint, Frame, Layout, Line, Rect, Span, Style, ViewportState};

/// One table column's width constraint and cell alignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableColumn {
    constraint: Constraint,
    alignment: Alignment,
}

impl TableColumn {
    pub fn new(constraint: Constraint) -> Self {
        Self {
            constraint,
            alignment: Alignment::Left,
        }
    }

    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

/// Caller-owned selection and scrolling state for [`Table`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TableState {
    selected: usize,
    viewport: ViewportState,
}

impl TableState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_selected(&mut self, selected: usize) {
        self.selected = selected;
    }

    pub fn selected_index(&mut self, row_count: usize) -> Option<usize> {
        if row_count == 0 {
            self.selected = 0;
            return None;
        }
        self.selected = self.selected.min(row_count - 1);
        Some(self.selected)
    }

    pub fn next(&mut self, row_count: usize) {
        if let Some(selected) = self.selected_index(row_count) {
            self.selected = (selected + 1) % row_count;
        }
    }

    pub fn previous(&mut self, row_count: usize) {
        if let Some(selected) = self.selected_index(row_count) {
            self.selected = (selected + row_count - 1) % row_count;
        }
    }

    pub fn scroll_up(&mut self) -> bool {
        self.viewport.scroll_up()
    }

    pub fn scroll_down(&mut self) -> bool {
        self.viewport.scroll_down()
    }

    pub fn page_up(&mut self) -> bool {
        self.viewport.page_up()
    }

    pub fn page_down(&mut self) -> bool {
        self.viewport.page_down()
    }

    pub fn viewport(&self) -> &ViewportState {
        &self.viewport
    }
}

/// A constrained-column table rendered with explicit [`TableState`].
///
/// Rows and headers are [`Line`] values so cell content can retain rich styling.
pub struct Table {
    columns: Vec<TableColumn>,
    header: Option<Vec<Line>>,
    rows: Vec<Vec<Line>>,
    selected_style: Style,
}

impl Table {
    pub fn new(columns: impl IntoIterator<Item = TableColumn>) -> Self {
        Self {
            columns: columns.into_iter().collect(),
            header: None,
            rows: Vec::new(),
            selected_style: Style::new(),
        }
    }

    pub fn header(mut self, cells: impl IntoIterator<Item = Line>) -> Self {
        self.header = Some(cells.into_iter().collect());
        self
    }

    pub fn rows(mut self, rows: impl IntoIterator<Item = Vec<Line>>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }

    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect, state: &mut TableState) {
        if rect.width == 0 || rect.height == 0 || self.columns.is_empty() {
            return;
        }

        let column_rects = Layout::horizontal(
            self.columns
                .iter()
                .map(|column| column.constraint)
                .collect::<Vec<_>>(),
        )
        .split(rect);
        let header_height = u16::from(self.header.is_some());
        if let Some(header) = &self.header {
            self.render_row(frame, &column_rects, rect.y, header, None);
        }

        let body = Rect::new(
            rect.x,
            rect.y.saturating_add(header_height),
            rect.width,
            rect.height.saturating_sub(header_height),
        );
        state
            .viewport
            .update_dimensions(self.rows.len(), body.height);
        let selected = state.selected_index(self.rows.len());
        let start = state.viewport.offset();
        let end = start
            .saturating_add(usize::from(body.height))
            .min(self.rows.len());
        for (visible_row, row) in self.rows[start..end].iter().enumerate() {
            let Some(y) = body.y.checked_add(visible_row as u16) else {
                break;
            };
            self.render_row(
                frame,
                &column_rects,
                y,
                row,
                (selected == Some(start + visible_row)).then_some(self.selected_style),
            );
        }
    }

    fn render_row(
        &self,
        frame: &mut Frame,
        column_rects: &[Rect],
        y: u16,
        cells: &[Line],
        selected_style: Option<Style>,
    ) {
        for (index, column) in self.columns.iter().enumerate() {
            let Some(column_rect) = column_rects.get(index).copied() else {
                break;
            };
            let target = Rect::new(column_rect.x, y, column_rect.width, 1);
            if let Some(style) = selected_style {
                for x in 0..target.width {
                    if let Some(x) = target.x.checked_add(x) {
                        frame.set_cell(x, y, Cell::new(' ', style));
                    }
                }
            }
            let Some(cell) = cells.get(index) else {
                continue;
            };
            let cell = selected_style.map_or_else(
                || cell.clone().alignment(column.alignment),
                |style| {
                    Line::new(
                        cell.spans()
                            .iter()
                            .map(|span| Span::styled(span.content(), span.style().patch(style))),
                    )
                    .alignment(column.alignment)
                },
            );
            cell.render(frame, target);
        }
    }
}
