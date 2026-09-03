use dragons_tui::{Frame, KeyCode, KeyEvent, KeyModifiers, Position, Rect, Style, TextArea};

#[test]
fn text_area_edits_lines_and_moves_the_cursor_without_invalid_utf8() {
    let mut area = TextArea::new();
    for character in "ab".chars() {
        area.insert(character);
    }
    assert!(area.enter());
    for character in "İ你🚀".chars() {
        area.insert(character);
    }
    assert_eq!(area.text(), "ab\nİ你🚀");

    assert!(area.left());
    assert!(area.backspace());
    assert_eq!(area.text(), "ab\nİ🚀");
    assert!(area.up());
    assert!(area.end());
    assert!(area.delete());
    assert_eq!(area.text(), "abİ🚀");
    assert!(area.text().is_char_boundary(area.text().len()));
}

#[test]
fn text_area_handles_navigation_scroll_and_clips_to_its_rect() {
    let mut area = TextArea::from("one\ntwo\nthree\nfour\n你好abcdef");
    assert!(area.handle_key(KeyEvent {
        code: KeyCode::Up,
        modifiers: KeyModifiers::default(),
    }));
    assert!(area.handle_key(KeyEvent {
        code: KeyCode::PageDown,
        modifiers: KeyModifiers::default(),
    }));

    let mut frame = Frame::new(8, 3);
    assert_eq!(
        area.render(&mut frame, Rect::new(0, 0, 5, 2), Style::new()),
        Some(Position { x: 4, y: 1 })
    );
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, 'f');
    assert_eq!(frame.buffer().get(0, 1).unwrap().character, '你');
    assert_eq!(
        area.render(&mut frame, Rect::new(0, 0, 0, 0), Style::new()),
        None
    );
}

#[test]
fn text_area_normalizes_delete_home_and_tiny_rects() {
    let mut area = TextArea::from("first\nsecond");
    assert!(area.handle_key(KeyEvent {
        code: KeyCode::Home,
        modifiers: KeyModifiers::default(),
    }));
    assert!(area.handle_key(KeyEvent {
        code: KeyCode::Up,
        modifiers: KeyModifiers::default(),
    }));
    assert!(area.handle_key(KeyEvent {
        code: KeyCode::Delete,
        modifiers: KeyModifiers::default(),
    }));
    assert_eq!(area.text(), "irst\nsecond");
    assert_eq!(area.cursor(), 0);
}

#[test]
fn text_area_uses_terminal_display_width_for_cjk_horizontal_clipping() {
    let mut area = TextArea::from("你好a");
    assert!(area.home());
    let mut frame = Frame::new(5, 1);

    area.render(&mut frame, Rect::new(0, 0, 5, 1), Style::new());

    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '你');
    assert_eq!(frame.buffer().get(2, 0).unwrap().character, '好');
    assert_eq!(frame.buffer().get(4, 0).unwrap().character, 'a');
}
