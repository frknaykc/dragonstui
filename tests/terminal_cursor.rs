use dragons_tui::{Position, set_cursor};

#[test]
fn terminal_cursor_is_visible_at_input_position_or_hidden_elsewhere() {
    let mut output = Vec::new();
    set_cursor(&mut output, Some(Position { x: 2, y: 3 })).unwrap();
    let visible = String::from_utf8(output).unwrap();
    assert!(visible.contains("\u{1b}[4;3H"));
    assert!(visible.contains("\u{1b}[?25h"));

    let mut output = Vec::new();
    set_cursor(&mut output, None).unwrap();
    assert!(String::from_utf8(output).unwrap().contains("\u{1b}[?25l"));
}
