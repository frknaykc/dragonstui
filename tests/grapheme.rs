use dragons_tui::{KeyCode, KeyEvent, KeyModifiers, TextArea, TextInput};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::default(),
    }
}

#[test]
fn text_input_treats_combining_variation_zwj_and_flag_sequences_as_single_editing_units() {
    let mut input = TextInput::new();
    for character in "é❤️👨‍👩‍👧‍👦🇹🇷".chars() {
        input.insert(character);
    }
    assert_eq!(input.cursor(), 4);

    assert!(input.left());
    assert_eq!(input.cursor(), 3);
    assert!(input.handle_key(key(KeyCode::Delete)));
    assert_eq!(input.text(), "é❤️👨‍👩‍👧‍👦");
    assert!(input.backspace());
    assert_eq!(input.text(), "é❤️");
    assert!(input.text().is_char_boundary(input.text().len()));
}

#[test]
fn text_area_moves_and_deletes_by_grapheme_without_splitting_utf8() {
    let mut area = TextArea::from("é❤️👨‍👩‍👧‍👦🇹🇷");
    assert_eq!(area.cursor(), 4);
    assert!(area.handle_key(key(KeyCode::Backspace)));
    assert_eq!(area.text(), "é❤️👨‍👩‍👧‍👦");
    assert!(area.handle_key(key(KeyCode::Backspace)));
    assert_eq!(area.text(), "é❤️");
    area.insert('\n');
    for character in "İstanbul你好🚀".chars() {
        area.insert(character);
    }
    assert!(area.handle_key(key(KeyCode::Up)));
    assert_eq!(area.cursor(), 2);
    assert!(area.text().is_char_boundary(area.text().len()));
}
