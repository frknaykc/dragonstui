use dragons_tui::{Cell, Color, Style};

#[test]
fn cell_preserves_character_rgb_colours_and_attributes() {
    let style = Style::new()
        .fg(Color::Rgb {
            r: 140,
            g: 200,
            b: 255,
        })
        .bg(Color::Rgb {
            r: 20,
            g: 22,
            b: 30,
        })
        .bold()
        .dim()
        .italic()
        .underline();
    let cell = Cell::new('D', style);

    assert_eq!(cell.character, 'D');
    assert_eq!(
        cell.style.fg,
        Some(Color::Rgb {
            r: 140,
            g: 200,
            b: 255
        })
    );
    assert_eq!(
        cell.style.bg,
        Some(Color::Rgb {
            r: 20,
            g: 22,
            b: 30
        })
    );
    assert!(cell.style.attributes.bold);
    assert!(cell.style.attributes.dim);
    assert!(cell.style.attributes.italic);
    assert!(cell.style.attributes.underline);
}
