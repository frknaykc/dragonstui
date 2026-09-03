use unicode_segmentation::UnicodeSegmentation;

use crate::{Frame, KeyCode, KeyEvent, Position, Rect, Style, display_width};

/// Multi-line editor with intrinsic grapheme-safe text, cursor, and scroll state.
///
/// Render mutates viewport bookkeeping and returns the application-owned terminal cursor position.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextArea {
    text: String,
    cursor: usize,
    top_line: usize,
    horizontal_offset: usize,
    preferred_column: Option<usize>,
    page_height: usize,
}

impl From<&str> for TextArea {
    fn from(text: &str) -> Self {
        Self::from(String::from(text))
    }
}
impl From<String> for TextArea {
    fn from(text: String) -> Self {
        let cursor = text.graphemes(true).count();
        Self {
            text,
            cursor,
            ..Self::default()
        }
    }
}

impl TextArea {
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
        if character == '\n' {
            self.enter();
            return;
        }
        let byte = self.byte_index_at(self.cursor);
        self.text.insert(byte, character);
        self.cursor = self.grapheme_index_after(byte + character.len_utf8());
        self.preferred_column = None;
    }
    pub fn enter(&mut self) -> bool {
        let byte = self.byte_index_at(self.cursor);
        self.text.insert(byte, '\n');
        self.cursor = self.grapheme_index_after(byte + 1);
        self.preferred_column = None;
        true
    }
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let start = self.byte_index_at(self.cursor - 1);
        let end = self.byte_index_at(self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor -= 1;
        self.preferred_column = None;
        true
    }
    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.grapheme_count() {
            return false;
        }
        let start = self.byte_index_at(self.cursor);
        let end = self.byte_index_at(self.cursor + 1);
        self.text.replace_range(start..end, "");
        self.preferred_column = None;
        true
    }
    pub fn left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.preferred_column = None;
        true
    }
    pub fn right(&mut self) -> bool {
        if self.cursor >= self.grapheme_count() {
            return false;
        }
        self.cursor += 1;
        self.preferred_column = None;
        true
    }
    pub fn up(&mut self) -> bool {
        let (line, column) = self.cursor_location();
        if line == 0 {
            return false;
        }
        self.move_to_line_column(line - 1, self.preferred_column.unwrap_or(column));
        true
    }
    pub fn down(&mut self) -> bool {
        let (line, column) = self.cursor_location();
        if line + 1 >= self.line_ranges().len() {
            return false;
        }
        self.move_to_line_column(line + 1, self.preferred_column.unwrap_or(column));
        true
    }
    pub fn home(&mut self) -> bool {
        let (line, _) = self.cursor_location();
        let start = self.line_ranges()[line].0;
        if self.cursor == start {
            return false;
        }
        self.cursor = start;
        self.preferred_column = None;
        true
    }
    pub fn end(&mut self) -> bool {
        let (line, _) = self.cursor_location();
        let end = self.line_ranges()[line].1;
        if self.cursor == end {
            return false;
        }
        self.cursor = end;
        self.preferred_column = None;
        true
    }
    pub fn page_up(&mut self) -> bool {
        let (line, column) = self.cursor_location();
        if line == 0 {
            return false;
        }
        self.move_to_line_column(line.saturating_sub(self.page_height.max(1)), column);
        true
    }
    pub fn page_down(&mut self) -> bool {
        let (line, column) = self.cursor_location();
        let count = self.line_ranges().len();
        if line + 1 >= count {
            return false;
        }
        self.move_to_line_column((line + self.page_height.max(1)).min(count - 1), column);
        true
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.top_line = 0;
        self.horizontal_offset = 0;
        self.preferred_column = None;
    }
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.ctrl || key.modifiers.alt {
            return false;
        }
        match key.code {
            KeyCode::Char(c) if !c.is_control() => {
                self.insert(c);
                true
            }
            KeyCode::Enter => self.enter(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Up => self.up(),
            KeyCode::Down => self.down(),
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            _ => false,
        }
    }
    pub fn render(&mut self, frame: &mut Frame, rect: Rect, style: Style) -> Option<Position> {
        if rect.width == 0 || rect.height == 0 {
            return None;
        }
        self.page_height = usize::from(rect.height);
        let ranges = self.line_ranges();
        let (line, column) = self.cursor_location_with(&ranges);
        self.top_line = self.top_line.min(ranges.len().saturating_sub(1));
        if line < self.top_line {
            self.top_line = line;
        } else if line >= self.top_line + usize::from(rect.height) {
            self.top_line = line + 1 - usize::from(rect.height);
        }
        let display = self.display_column(line, column, &ranges);
        if display < self.horizontal_offset {
            self.horizontal_offset = display;
        } else if display >= self.horizontal_offset + usize::from(rect.width) {
            self.horizontal_offset = display + 1 - usize::from(rect.width);
        }
        self.horizontal_offset = self.aligned_offset(line, self.horizontal_offset, &ranges);
        for (row, &(start, end)) in ranges
            .iter()
            .skip(self.top_line)
            .take(usize::from(rect.height))
            .enumerate()
        {
            frame.write_text_in(
                rect,
                0,
                row as u16,
                &self.visible_line(start, end, usize::from(rect.width)),
                style,
            );
        }
        let x = display.saturating_sub(self.horizontal_offset);
        let y = line.saturating_sub(self.top_line);
        if x >= usize::from(rect.width) || y >= usize::from(rect.height) {
            return None;
        }
        Some(Position {
            x: rect.x.checked_add(u16::try_from(x).ok()?)?,
            y: rect.y.checked_add(u16::try_from(y).ok()?)?,
        })
    }
    fn line_ranges(&self) -> Vec<(usize, usize)> {
        let gs: Vec<&str> = self.text.graphemes(true).collect();
        let mut ranges = Vec::new();
        let mut start = 0;
        for (i, g) in gs.iter().enumerate() {
            if *g == "\n" {
                ranges.push((start, i));
                start = i + 1;
            }
        }
        ranges.push((start, gs.len()));
        ranges
    }
    fn cursor_location(&self) -> (usize, usize) {
        let r = self.line_ranges();
        self.cursor_location_with(&r)
    }
    fn cursor_location_with(&self, r: &[(usize, usize)]) -> (usize, usize) {
        let c = self.cursor.min(self.grapheme_count());
        for (line, &(start, end)) in r.iter().enumerate() {
            if c <= end {
                return (line, c.saturating_sub(start));
            }
        }
        let (start, end) = r.last().copied().unwrap_or((0, 0));
        (r.len().saturating_sub(1), end.saturating_sub(start))
    }
    fn move_to_line_column(&mut self, line: usize, target: usize) {
        let r = self.line_ranges();
        let (start, end) = r[line];
        let gs: Vec<&str> = self.text.graphemes(true).collect();
        let mut display = 0;
        let mut cursor = start;
        for g in &gs[start..end] {
            let w = display_width(g);
            if display + w > target {
                break;
            }
            display += w;
            cursor += 1;
        }
        self.cursor = cursor;
        self.preferred_column = Some(target);
    }
    fn display_column(&self, line: usize, column: usize, r: &[(usize, usize)]) -> usize {
        let (start, end) = r[line];
        let gs: Vec<&str> = self.text.graphemes(true).collect();
        gs[start..(start + column).min(end)]
            .iter()
            .map(|g| display_width(g))
            .sum()
    }
    fn aligned_offset(&self, line: usize, offset: usize, r: &[(usize, usize)]) -> usize {
        let (start, end) = r[line];
        let gs: Vec<&str> = self.text.graphemes(true).collect();
        let mut display = 0;
        for g in &gs[start..end] {
            if display >= offset {
                return display;
            }
            display += display_width(g);
        }
        display
    }
    fn visible_line(&self, start: usize, end: usize, width: usize) -> String {
        let gs: Vec<&str> = self.text.graphemes(true).collect();
        let mut display = 0;
        let mut used = 0;
        let mut out = String::new();
        for g in &gs[start..end] {
            let w = display_width(g);
            if display + w <= self.horizontal_offset {
                display += w;
                continue;
            }
            if display < self.horizontal_offset || used + w > width {
                break;
            }
            out.push_str(g);
            display += w;
            used += w;
        }
        out
    }
    fn grapheme_count(&self) -> usize {
        self.text.graphemes(true).count()
    }
    fn grapheme_index_after(&self, byte: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .take_while(|(start, _)| *start < byte)
            .count()
    }
    fn byte_index_at(&self, index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(index)
            .map_or(self.text.len(), |(i, _)| i)
    }
}
