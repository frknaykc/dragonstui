use crossterm::event::{
    Event as CrosstermEvent, KeyCode as CrosstermKeyCode, KeyEvent as CrosstermKeyEvent,
    KeyModifiers as CrosstermKeyModifiers, MouseButton as CrosstermMouseButton,
    MouseEvent as CrosstermMouseEvent, MouseEventKind as CrosstermMouseEventKind,
};
use dragons_tui::{Event, KeyCode, MouseButton, MouseKind, Size, normalize_crossterm_event};

#[test]
fn normalizes_character_and_ctrl_character_keys() {
    let character = normalize_crossterm_event(CrosstermEvent::Key(CrosstermKeyEvent::new(
        CrosstermKeyCode::Char('q'),
        CrosstermKeyModifiers::NONE,
    )))
    .unwrap();
    let ctrl_character = normalize_crossterm_event(CrosstermEvent::Key(CrosstermKeyEvent::new(
        CrosstermKeyCode::Char('c'),
        CrosstermKeyModifiers::CONTROL,
    )))
    .unwrap();

    match character {
        Event::Key(key) => {
            assert_eq!(key.code, KeyCode::Char('q'));
            assert!(!key.modifiers.ctrl);
        }
        _ => panic!("expected key event"),
    }
    match ctrl_character {
        Event::Key(key) => {
            assert_eq!(key.code, KeyCode::Char('c'));
            assert!(key.modifiers.ctrl);
        }
        _ => panic!("expected key event"),
    }
}

#[test]
fn normalizes_special_keys_and_modifiers() {
    for (source, expected) in [
        (CrosstermKeyCode::Enter, KeyCode::Enter),
        (CrosstermKeyCode::Esc, KeyCode::Escape),
        (CrosstermKeyCode::Backspace, KeyCode::Backspace),
        (CrosstermKeyCode::Tab, KeyCode::Tab),
        (CrosstermKeyCode::Left, KeyCode::Left),
        (CrosstermKeyCode::Right, KeyCode::Right),
        (CrosstermKeyCode::Up, KeyCode::Up),
        (CrosstermKeyCode::Down, KeyCode::Down),
        (CrosstermKeyCode::PageUp, KeyCode::PageUp),
        (CrosstermKeyCode::PageDown, KeyCode::PageDown),
        (CrosstermKeyCode::Home, KeyCode::Home),
        (CrosstermKeyCode::End, KeyCode::End),
    ] {
        let event = normalize_crossterm_event(CrosstermEvent::Key(CrosstermKeyEvent::new(
            source,
            CrosstermKeyModifiers::ALT | CrosstermKeyModifiers::SHIFT,
        )))
        .unwrap();

        match event {
            Event::Key(key) => {
                assert_eq!(key.code, expected);
                assert!(key.modifiers.alt);
                assert!(key.modifiers.shift);
            }
            _ => panic!("expected key event"),
        }
    }
}

#[test]
fn normalizes_resize_events() {
    assert_eq!(
        normalize_crossterm_event(CrosstermEvent::Resize(120, 40)),
        Some(Event::Resize(Size {
            width: 120,
            height: 40,
        }))
    );
}

#[test]
fn normalizes_mouse_kinds_coordinates_and_modifiers() {
    let cases = [
        (
            CrosstermMouseEventKind::Down(CrosstermMouseButton::Left),
            MouseKind::LeftDown,
        ),
        (
            CrosstermMouseEventKind::Up(CrosstermMouseButton::Left),
            MouseKind::LeftUp,
        ),
        (
            CrosstermMouseEventKind::Down(CrosstermMouseButton::Right),
            MouseKind::RightDown,
        ),
        (
            CrosstermMouseEventKind::Up(CrosstermMouseButton::Right),
            MouseKind::RightUp,
        ),
        (
            CrosstermMouseEventKind::Down(CrosstermMouseButton::Middle),
            MouseKind::MiddleDown,
        ),
        (
            CrosstermMouseEventKind::Up(CrosstermMouseButton::Middle),
            MouseKind::MiddleUp,
        ),
        (CrosstermMouseEventKind::ScrollUp, MouseKind::ScrollUp),
        (CrosstermMouseEventKind::ScrollDown, MouseKind::ScrollDown),
        (CrosstermMouseEventKind::Moved, MouseKind::Move),
        (
            CrosstermMouseEventKind::Drag(CrosstermMouseButton::Left),
            MouseKind::Drag(MouseButton::Left),
        ),
    ];

    for (kind, expected_kind) in cases {
        let event = normalize_crossterm_event(CrosstermEvent::Mouse(CrosstermMouseEvent {
            kind,
            column: 120,
            row: 40,
            modifiers: CrosstermKeyModifiers::CONTROL
                | CrosstermKeyModifiers::ALT
                | CrosstermKeyModifiers::SHIFT,
        }))
        .expect("supported mouse event should normalize");

        match event {
            Event::Mouse(mouse) => {
                assert_eq!(mouse.kind, expected_kind);
                assert_eq!((mouse.x, mouse.y), (120, 40));
                assert!(mouse.modifiers.ctrl);
                assert!(mouse.modifiers.alt);
                assert!(mouse.modifiers.shift);
            }
            _ => panic!("expected mouse event"),
        }
    }
}
