use dragons_tui::{Frame, KeyCode, KeyEvent, KeyModifiers, Position, Rect, Style, TextInput};

#[test]
fn text_input_edits_ascii_turkish_cjk_and_emoji_without_breaking_utf8() {
    let mut input = TextInput::new();
    for character in ['h', 'İ', '你', '🚀'] {
        input.insert(character);
    }

    assert_eq!(input.text(), "hİ你🚀");
    assert_eq!(input.cursor(), 4);
    assert!(input.backspace());
    assert_eq!(input.text(), "hİ你");
    assert_eq!(input.cursor(), 3);

    input.left();
    input.insert('X');
    assert_eq!(input.text(), "hİX你");
    input.right();
    assert_eq!(input.cursor(), 4);
    input.clear();
    assert_eq!(input.text(), "");
    assert_eq!(input.cursor(), 0);
}

#[test]
fn text_input_handles_normalized_keys_without_handling_ctrl_c() {
    let mut input = TextInput::new();
    assert!(input.handle_key(KeyEvent::character('q')));
    assert_eq!(input.text(), "q");
    assert!(!input.handle_key(KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    }));
    assert_eq!(input.text(), "q");
    assert!(input.handle_key(KeyEvent {
        code: KeyCode::Backspace,
        modifiers: KeyModifiers::default(),
    }));
    assert_eq!(input.text(), "");
}

#[test]
fn text_input_viewport_keeps_wide_character_cursor_visible() {
    let mut input = TextInput::new();
    for character in "ab你好🚀z".chars() {
        input.insert(character);
    }

    let viewport = input.viewport(4);
    assert_eq!(viewport.text, "🚀z");
    assert_eq!(viewport.cursor_column, 3);

    let mut frame = Frame::new(6, 1);
    let cursor = input.render(&mut frame, Rect::new(0, 0, 4, 1), Style::new());
    assert_eq!(cursor, Some(Position { x: 3, y: 0 }));
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '🚀');
}

#[test]
fn text_input_reports_a_nonzero_terminal_cursor_position_after_wide_text() {
    let mut input = TextInput::new();
    for character in ['a', '你', 'b'] {
        input.insert(character);
    }
    assert!(input.left());

    let mut frame = Frame::new(10, 5);
    assert_eq!(
        input.render(&mut frame, Rect::new(4, 3, 5, 1), Style::new()),
        Some(Position { x: 7, y: 3 })
    );
    assert_eq!(frame.buffer().get(5, 3).unwrap().character, '你');
    assert_eq!(
        input.render(&mut frame, Rect::new(4, 3, 0, 1), Style::new()),
        None
    );
}
