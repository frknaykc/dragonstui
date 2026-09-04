use crate::{Frame, Rect, Style, ViewportState};

/// Producer-supplied semantics for one unified-diff row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffLineKind {
    Header,
    Hunk,
    Context,
    Added,
    Deleted,
    Unknown,
}

/// A read-only diff row with optional old/new source coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    kind: DiffLineKind,
    content: String,
    old_line: Option<usize>,
    new_line: Option<usize>,
}

impl DiffLine {
    pub fn kind(&self) -> DiffLineKind {
        self.kind
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn numbering(&self) -> (Option<usize>, Option<usize>) {
        (self.old_line, self.new_line)
    }
}

/// A parsed, read-only unified diff. Parsing is conservative: malformed hunks
/// retain their text but do not receive inferred source coordinates.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiffDocument {
    lines: Vec<DiffLine>,
}

impl DiffDocument {
    pub fn parse_unified(input: &str) -> Self {
        let mut lines = Vec::new();
        let mut old_line = None;
        let mut new_line = None;

        for raw_line in input.lines() {
            let content = raw_line.trim_end_matches('\r');
            if content.starts_with("@@") {
                old_line = hunk_start(content, '-');
                new_line = hunk_start(content, '+');
                lines.push(DiffLine {
                    kind: DiffLineKind::Hunk,
                    content: content.to_owned(),
                    old_line: None,
                    new_line: None,
                });
                continue;
            }

            if content.starts_with("--- ")
                || content.starts_with("+++ ")
                || content.starts_with("diff ")
                || content.starts_with("index ")
            {
                lines.push(DiffLine {
                    kind: DiffLineKind::Header,
                    content: content.to_owned(),
                    old_line: None,
                    new_line: None,
                });
                continue;
            }

            let (kind, row_old_line, row_new_line) = match content.chars().next() {
                Some(' ') => {
                    let numbering = (old_line, new_line);
                    old_line = old_line.and_then(|line| line.checked_add(1));
                    new_line = new_line.and_then(|line| line.checked_add(1));
                    (DiffLineKind::Context, numbering.0, numbering.1)
                }
                Some('-') => {
                    let numbering = old_line;
                    old_line = old_line.and_then(|line| line.checked_add(1));
                    (DiffLineKind::Deleted, numbering, None)
                }
                Some('+') => {
                    let numbering = new_line;
                    new_line = new_line.and_then(|line| line.checked_add(1));
                    (DiffLineKind::Added, None, numbering)
                }
                _ => (DiffLineKind::Unknown, None, None),
            };
            lines.push(DiffLine {
                kind,
                content: content.to_owned(),
                old_line: row_old_line,
                new_line: row_new_line,
            });
        }

        Self { lines }
    }

    pub fn lines(&self) -> &[DiffLine] {
        &self.lines
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

fn hunk_start(header: &str, marker: char) -> Option<usize> {
    let range = header
        .split_whitespace()
        .find(|part| part.starts_with(marker))?
        .strip_prefix(marker)?;
    let start = range.split_once(',').map_or(range, |(start, _)| start);
    start.parse::<usize>().ok().filter(|line| *line > 0)
}

/// Application-supplied presentation for each explicit diff line kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffStyles {
    pub header: Style,
    pub hunk: Style,
    pub context: Style,
    pub added: Style,
    pub deleted: Style,
    pub gutter: Style,
}

impl DiffStyles {
    fn line_style(self, kind: DiffLineKind) -> Style {
        match kind {
            DiffLineKind::Header => self.header,
            DiffLineKind::Hunk => self.hunk,
            DiffLineKind::Context | DiffLineKind::Unknown => self.context,
            DiffLineKind::Added => self.added,
            DiffLineKind::Deleted => self.deleted,
        }
    }
}

/// Stateless unified-diff rendering over a caller-owned vertical viewport.
pub struct DiffViewer<'a> {
    document: &'a DiffDocument,
}

impl<'a> DiffViewer<'a> {
    pub fn new(document: &'a DiffDocument) -> Self {
        Self { document }
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        rect: Rect,
        viewport: &mut ViewportState,
        styles: DiffStyles,
    ) {
        viewport.update_dimensions(self.document.lines.len(), rect.height);
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        if self.document.lines.is_empty() {
            frame.write_text_in(rect, 0, 0, "(empty diff)", styles.context);
            return;
        }

        let number_width = self
            .document
            .lines
            .iter()
            .flat_map(|line| [line.old_line, line.new_line])
            .flatten()
            .map(decimal_width)
            .max()
            .unwrap_or(1);
        let number_width = u16::try_from(number_width).unwrap_or(u16::MAX);
        let new_number_x = number_width.saturating_add(1);
        let content_x = number_width.saturating_mul(2).saturating_add(2);
        let start = viewport.offset();
        let end = start
            .saturating_add(usize::from(rect.height))
            .min(self.document.lines.len());

        for (visible_row, line) in self.document.lines[start..end].iter().enumerate() {
            let row = u16::try_from(visible_row).unwrap_or(u16::MAX);
            if let Some(old_line) = line.old_line {
                frame.write_text_in(
                    rect,
                    0,
                    row,
                    &format!("{old_line:>width$}", width = usize::from(number_width)),
                    styles.gutter,
                );
            }
            if let Some(new_line) = line.new_line {
                frame.write_text_in(
                    rect,
                    new_number_x,
                    row,
                    &format!("{new_line:>width$}", width = usize::from(number_width)),
                    styles.gutter,
                );
            }
            frame.write_text_in(
                rect,
                content_x,
                row,
                &line.content,
                styles.line_style(line.kind),
            );
        }
    }
}

fn decimal_width(value: usize) -> usize {
    value.to_string().len()
}
