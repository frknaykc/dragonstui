use std::{
    io::{self, Write},
    time::Duration,
};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self as crossterm_event, Event as CrosstermEvent, KeyCode as CrosstermKeyCode,
        KeyEvent as CrosstermKeyEvent, KeyEventKind, KeyModifiers as CrosstermKeyModifiers,
        MouseButton as CrosstermMouseButton, MouseEvent as CrosstermMouseEvent,
        MouseEventKind as CrosstermMouseEventKind,
    },
    queue,
    style::{
        Attribute, Color as CrosstermColor, Print, SetAttribute, SetBackgroundColor,
        SetForegroundColor,
    },
    terminal::{Clear, ClearType},
};

use crate::{
    CellKind, ChangedCell, Color, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent,
    MouseKind, Position, Size, Style,
};

pub fn terminal_size() -> io::Result<Size> {
    let (width, height) = crossterm::terminal::size()?;
    Ok(Size::new(width, height))
}

pub fn normalize_crossterm_event(event: CrosstermEvent) -> Option<Event> {
    match event {
        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
            normalize_key_event(key).map(Event::Key)
        }
        CrosstermEvent::Mouse(mouse) => normalize_mouse_event(mouse).map(Event::Mouse),
        CrosstermEvent::Resize(width, height) => Some(Event::Resize(Size { width, height })),
        _ => None,
    }
}

pub(crate) fn poll_event(timeout: Duration) -> io::Result<Option<Event>> {
    if !crossterm_event::poll(timeout)? {
        return Ok(None);
    }

    loop {
        if let Some(event) = normalize_crossterm_event(crossterm_event::read()?) {
            return Ok(Some(event));
        }
        if !crossterm_event::poll(Duration::ZERO)? {
            return Ok(None);
        }
    }
}

pub fn render_changed_cells(
    output: &mut impl Write,
    changed: &[ChangedCell],
    clear_screen: bool,
) -> io::Result<()> {
    if clear_screen {
        queue!(output, Clear(ClearType::All))?;
    }

    let mut active_style = None;
    let mut next_position = None;

    for change in changed {
        if change.current.kind == CellKind::WideContinuation {
            continue;
        }

        if next_position != Some((change.x, change.y)) {
            queue!(output, MoveTo(change.x, change.y))?;
        }

        if active_style != Some(change.current.style) {
            apply_style(output, change.current.style)?;
            active_style = Some(change.current.style);
        }

        queue!(output, Print(change.current.character))?;
        let width = match change.current.kind {
            CellKind::Wide => 2,
            CellKind::Normal | CellKind::WideContinuation => 1,
        };
        next_position = change.x.checked_add(width).map(|next_x| (next_x, change.y));
    }

    if active_style.is_some() {
        queue!(output, SetAttribute(Attribute::Reset))?;
    }

    output.flush()
}

pub fn set_cursor(output: &mut impl Write, position: Option<Position>) -> io::Result<()> {
    match position {
        Some(position) => queue!(output, MoveTo(position.x, position.y), Show)?,
        None => queue!(output, Hide)?,
    }
    output.flush()
}

fn normalize_key_event(event: CrosstermKeyEvent) -> Option<KeyEvent> {
    let code = match event.code {
        CrosstermKeyCode::Char(character) => KeyCode::Char(character),
        CrosstermKeyCode::Enter => KeyCode::Enter,
        CrosstermKeyCode::Esc => KeyCode::Escape,
        CrosstermKeyCode::Backspace => KeyCode::Backspace,
        CrosstermKeyCode::Delete => KeyCode::Delete,
        CrosstermKeyCode::Tab => KeyCode::Tab,
        CrosstermKeyCode::Left => KeyCode::Left,
        CrosstermKeyCode::Right => KeyCode::Right,
        CrosstermKeyCode::Up => KeyCode::Up,
        CrosstermKeyCode::Down => KeyCode::Down,
        CrosstermKeyCode::PageUp => KeyCode::PageUp,
        CrosstermKeyCode::PageDown => KeyCode::PageDown,
        CrosstermKeyCode::Home => KeyCode::Home,
        CrosstermKeyCode::End => KeyCode::End,
        _ => return None,
    };

    Some(KeyEvent {
        code,
        modifiers: normalize_modifiers(event.modifiers),
    })
}

fn normalize_mouse_event(event: CrosstermMouseEvent) -> Option<MouseEvent> {
    let kind = match event.kind {
        CrosstermMouseEventKind::Down(CrosstermMouseButton::Left) => MouseKind::LeftDown,
        CrosstermMouseEventKind::Up(CrosstermMouseButton::Left) => MouseKind::LeftUp,
        CrosstermMouseEventKind::Down(CrosstermMouseButton::Right) => MouseKind::RightDown,
        CrosstermMouseEventKind::Up(CrosstermMouseButton::Right) => MouseKind::RightUp,
        CrosstermMouseEventKind::Down(CrosstermMouseButton::Middle) => MouseKind::MiddleDown,
        CrosstermMouseEventKind::Up(CrosstermMouseButton::Middle) => MouseKind::MiddleUp,
        CrosstermMouseEventKind::ScrollUp => MouseKind::ScrollUp,
        CrosstermMouseEventKind::ScrollDown => MouseKind::ScrollDown,
        CrosstermMouseEventKind::Moved => MouseKind::Move,
        CrosstermMouseEventKind::Drag(button) => MouseKind::Drag(normalize_mouse_button(button)?),
        CrosstermMouseEventKind::ScrollLeft | CrosstermMouseEventKind::ScrollRight => return None,
    };

    Some(MouseEvent {
        x: event.column,
        y: event.row,
        kind,
        modifiers: normalize_modifiers(event.modifiers),
    })
}

fn normalize_mouse_button(button: CrosstermMouseButton) -> Option<MouseButton> {
    match button {
        CrosstermMouseButton::Left => Some(MouseButton::Left),
        CrosstermMouseButton::Right => Some(MouseButton::Right),
        CrosstermMouseButton::Middle => Some(MouseButton::Middle),
    }
}

fn normalize_modifiers(modifiers: CrosstermKeyModifiers) -> KeyModifiers {
    KeyModifiers {
        ctrl: modifiers.contains(CrosstermKeyModifiers::CONTROL),
        alt: modifiers.contains(CrosstermKeyModifiers::ALT),
        shift: modifiers.contains(CrosstermKeyModifiers::SHIFT),
    }
}

fn apply_style(output: &mut impl Write, style: Style) -> io::Result<()> {
    queue!(output, SetAttribute(Attribute::Reset))?;

    if let Some(color) = style.fg {
        queue!(output, SetForegroundColor(to_crossterm_color(color)))?;
    }

    if let Some(color) = style.bg {
        queue!(output, SetBackgroundColor(to_crossterm_color(color)))?;
    }

    if style.attributes.bold {
        queue!(output, SetAttribute(Attribute::Bold))?;
    }
    if style.attributes.dim {
        queue!(output, SetAttribute(Attribute::Dim))?;
    }
    if style.attributes.italic {
        queue!(output, SetAttribute(Attribute::Italic))?;
    }
    if style.attributes.underline {
        queue!(output, SetAttribute(Attribute::Underlined))?;
    }
    if style.attributes.strikethrough {
        queue!(output, SetAttribute(Attribute::CrossedOut))?;
    }
    if style.attributes.reverse {
        queue!(output, SetAttribute(Attribute::Reverse))?;
    }

    Ok(())
}

fn to_crossterm_color(color: Color) -> CrosstermColor {
    match color {
        Color::Rgb { r, g, b } => CrosstermColor::Rgb { r, g, b },
    }
}
