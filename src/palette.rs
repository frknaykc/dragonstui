use crate::{
    CommandId, Frame, KeyCode, KeyEvent, Line, List, ListState, Modal, Rect, Style, TextInput,
};

/// One executable [`CommandPalette`] entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteCommand {
    pub id: CommandId,
    pub title: String,
}

impl PaletteCommand {
    pub fn new(id: CommandId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
        }
    }
}

/// Query-owning command overlay composed from existing modal, input, and list primitives.
///
/// Applications open it, route keys to it, and execute the returned [`CommandId`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPalette {
    commands: Vec<PaletteCommand>,
    query: TextInput,
    selected: usize,
}

impl CommandPalette {
    pub fn new(commands: impl IntoIterator<Item = PaletteCommand>) -> Self {
        Self {
            commands: commands.into_iter().collect(),
            query: TextInput::new(),
            selected: 0,
        }
    }

    pub fn query(&self) -> &str {
        self.query.text()
    }

    pub fn filtered_titles(&self) -> Vec<String> {
        self.filtered()
            .iter()
            .map(|command| command.title.clone())
            .collect()
    }

    pub fn selected_index(&self) -> Option<usize> {
        let count = self.filtered().len();
        if count == 0 {
            None
        } else {
            Some(self.selected.min(count - 1))
        }
    }

    pub fn execute_selected(&self) -> Option<CommandId> {
        self.selected_index().and_then(|selected| {
            self.filtered()
                .get(selected)
                .map(|command| command.id.clone())
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let count = self.filtered().len();
        match key.code {
            KeyCode::Up if count > 0 => {
                let selected = self.selected.min(count - 1);
                self.selected = selected.saturating_sub(1);
                true
            }
            KeyCode::Down if count > 0 => {
                self.selected = (self.selected.min(count - 1) + 1).min(count - 1);
                true
            }
            KeyCode::Up | KeyCode::Down => false,
            _ => {
                let changed = self.query.handle_key(key);
                if changed {
                    self.selected = 0;
                }
                changed
            }
        }
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        parent: Rect,
        border_style: Style,
        content_style: Style,
        selected_style: Style,
    ) -> Rect {
        let rect = Modal::new(
            "Command Palette",
            [Line::from(format!("> {}", self.query()))],
        )
        .size(42, 12)
        .border_style(border_style)
        .title_style(border_style)
        .content_style(content_style)
        .render(frame, parent);
        let inner = rect.inner();
        let list_rect = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );
        let titles = self.filtered_titles();
        let items: Vec<&str> = titles.iter().map(String::as_str).collect();
        let mut state = ListState::new();
        if let Some(selected) = self.selected_index() {
            state.set_selected(selected);
        }
        List::new(&items)
            .normal_style(content_style)
            .selected_style(selected_style)
            .render(frame, list_rect, &mut state);
        rect
    }

    fn filtered(&self) -> Vec<&PaletteCommand> {
        let query = self.query().to_lowercase();
        self.commands
            .iter()
            .filter(|command| command.title.to_lowercase().contains(&query))
            .collect()
    }
}
