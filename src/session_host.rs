use std::collections::VecDeque;

use crate::{Frame, Rect, Scrollbar, ScrollbarGeometry, Style, ViewportState};

/// Presentation-only state for one generic, controller-owned session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionHostState {
    Opening,
    Running,
    Exited { exit_code: Option<i32> },
    Failed { message: String },
    Closed,
}

/// Bounded, line-oriented output surface for a generic interactive session.
///
/// This is deliberately not a terminal emulator: C0 control bytes are rendered
/// as replacement characters, preventing provider output from changing outer
/// terminal state. Session execution and I/O authority stay outside this view.
#[derive(Clone, Debug)]
pub struct SessionHost {
    lines: VecDeque<String>,
    scrollback_capacity: usize,
    state: SessionHostState,
    viewport: ViewportState,
}

impl SessionHost {
    pub fn new(scrollback_capacity: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            scrollback_capacity: scrollback_capacity.max(1),
            state: SessionHostState::Opening,
            viewport: ViewportState::new(),
        }
    }

    pub fn state(&self) -> SessionHostState {
        self.state.clone()
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }

    pub const fn scrollback_capacity(&self) -> usize {
        self.scrollback_capacity
    }

    pub fn mark_running(&mut self) {
        self.state = SessionHostState::Running;
    }

    pub fn mark_exited(&mut self, exit_code: Option<i32>) {
        self.state = SessionHostState::Exited { exit_code };
    }

    pub fn mark_failed(&mut self, message: impl Into<String>) {
        self.state = SessionHostState::Failed {
            message: message.into(),
        };
    }

    pub fn mark_closed(&mut self) {
        self.state = SessionHostState::Closed;
    }

    /// Appends opaque provider output after making control bytes inert. The
    /// oldest complete lines are evicted before this returns.
    pub fn push_output(&mut self, output: &str) {
        let safe = output
            .chars()
            .map(|character| {
                if character == '\n' || !character.is_control() {
                    character
                } else {
                    '\u{fffd}'
                }
            })
            .collect::<String>();
        for line in safe.split('\n') {
            self.lines.push_back(line.to_owned());
            while self.lines.len() > self.scrollback_capacity {
                self.lines.pop_front();
            }
        }
    }

    pub fn scroll_up(&mut self, lines: usize) -> bool {
        let mut changed = false;
        for _ in 0..lines {
            changed |= self.viewport.scroll_up();
        }
        changed
    }

    pub fn scroll_down(&mut self, lines: usize) -> bool {
        let mut changed = false;
        for _ in 0..lines {
            changed |= self.viewport.scroll_down();
        }
        changed
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        rect: Rect,
        output_style: Style,
        track_style: Style,
        thumb_style: Style,
    ) -> Option<ScrollbarGeometry> {
        self.viewport
            .update_dimensions(self.lines.len(), rect.height);
        if rect.width == 0 || rect.height == 0 {
            return None;
        }
        let content = Rect::new(rect.x, rect.y, rect.width.saturating_sub(1), rect.height);
        if self.lines.is_empty() {
            frame.write_text_in(content, 0, 0, "(session output pending)", output_style);
        } else {
            for (row, line) in self
                .lines
                .iter()
                .skip(self.viewport.offset())
                .take(usize::from(content.height))
                .enumerate()
            {
                frame.write_text_in(content, 0, row as u16, line, output_style);
            }
        }
        Scrollbar::render(
            frame,
            &self.viewport,
            Rect::new(
                rect.x.saturating_add(rect.width.saturating_sub(1)),
                rect.y,
                1,
                rect.height,
            ),
            track_style,
            thumb_style,
        )
    }
}
