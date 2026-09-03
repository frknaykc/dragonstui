//! Editors own their local text state; the application owns event routing and cursor lifecycle.

use std::io;

use dragons_tui::{
    Color, Frame, KeyCode, KeyEvent, KeyModifiers, Rect, Style, Text, TextArea, TextInput, diff,
    render_changed_cells,
};

fn main() -> io::Result<()> {
    let mut input = TextInput::new();
    for character in "İstanbul 🚀".chars() {
        input.insert(character);
    }
    input.handle_key(KeyEvent::character('!'));

    let mut area = TextArea::from("é ❤️ 👨‍👩‍👧‍👦 🇹🇷\n你好");
    area.handle_key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::default(),
    });
    area.handle_key(KeyEvent::character('o'));
    area.handle_key(KeyEvent::character('k'));
    area.handle_key(KeyEvent::character('!'));

    let mut frame = Frame::new(52, 8);
    let style = Style::new().fg(Color::rgb(240, 225, 205));
    Text::new("TextInput:").render(&mut frame, Rect::new(0, 0, 52, 1));
    let _cursor = input.render(&mut frame, Rect::new(0, 1, 52, 1), style);
    Text::new("TextArea:").render(&mut frame, Rect::new(0, 3, 52, 1));
    let _cursor = area.render(&mut frame, Rect::new(0, 4, 52, 4), style);

    render_changed_cells(&mut io::stdout(), &diff(None, frame.buffer()), true)
}
