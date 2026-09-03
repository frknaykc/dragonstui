use unicode_segmentation::UnicodeSegmentation;

use crate::{Frame, KeyCode, KeyEvent, Position, Rect, Style, display_width};

/// The clipped text and cursor column returned by [`TextInput::viewport`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputViewport {
    pub text: String,
    pub cursor_column: u16,
}

/// Single-line editor with intrinsic grapheme-safe editing state.
///
/// Edits extended grapheme clusters; terminal emoji widths remain terminal-dependent. Rendering
/// returns the application-owned terminal cursor position rather than managing it automatically.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextInput {
    text: String,
    cursor: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Cursor is an extended grapheme-cluster index, never a UTF-8 byte offset.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn insert(&mut self, character: char) {
        let byte_index = self.byte_index_at(self.cursor);
        self.text.insert(byte_index, character);
        self.cursor = self.grapheme_index_after(byte_index + character.len_utf8());
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let start = self.byte_index_at(self.cursor - 1);
        let end = self.byte_index_at(self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor -= 1;
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.grapheme_count() {
            return false;
        }
        let start = self.byte_index_at(self.cursor);
        let end = self.byte_index_at(self.cursor + 1);
        self.text.replace_range(start..end, "");
        true
    }

    pub fn left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        true
    }

    pub fn right(&mut self) -> bool {
        if self.cursor >= self.grapheme_count() {
            return false;
        }
        self.cursor += 1;
        true
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.ctrl || key.modifiers.alt {
            return false;
        }
        match key.code {
            KeyCode::Char(character) if !character.is_control() => {
                self.insert(character);
                true
            }
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            _ => false,
        }
    }

    pub fn viewport(&self, width: u16) -> InputViewport {
        if width == 0 {
            return InputViewport {
                text: String::new(),
                cursor_column: 0,
            };
        }
        let graphemes: Vec<&str> = self.text.graphemes(true).collect();
        let cursor = self.cursor.min(graphemes.len());
        let cursor_width: usize = graphemes[..cursor]
            .iter()
            .map(|item| display_width(item))
            .sum();
        let mut start = 0;
        let mut start_width = 0;
        let max_cursor_column = usize::from(width - 1);
        while start < cursor && cursor_width.saturating_sub(start_width) > max_cursor_column {
            start_width += display_width(graphemes[start]);
            start += 1;
        }
        let mut text = String::new();
        let mut used_width = 0;
        for grapheme in &graphemes[start..] {
            let grapheme_width = display_width(grapheme);
            if used_width + grapheme_width > usize::from(width) {
                break;
            }
            text.push_str(grapheme);
            used_width += grapheme_width;
        }
        InputViewport {
            text,
            cursor_column: u16::try_from(cursor_width.saturating_sub(start_width))
                .unwrap_or(width - 1),
        }
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect, style: Style) -> Option<Position> {
        if rect.width == 0 || rect.height == 0 {
            return None;
        }
        let viewport = self.viewport(rect.width);
        frame.write_text_in(rect, 0, 0, &viewport.text, style);
        let x = rect.x.checked_add(viewport.cursor_column)?;
        Some(Position { x, y: rect.y })
    }

    fn grapheme_count(&self) -> usize {
        self.text.graphemes(true).count()
    }

    fn grapheme_index_after(&self, byte_index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .take_while(|(start, _)| *start < byte_index)
            .count()
    }

    fn byte_index_at(&self, grapheme_index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(grapheme_index)
            .map_or(self.text.len(), |(index, _)| index)
    }
}
