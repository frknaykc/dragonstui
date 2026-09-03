use dragons_tui::{BorderSet, Cell, Color, Frame, Panel, Rect, Style};

#[test]
fn panel_uses_square_and_double_border_sets_without_mixing_glyphs_or_styles() {
    let square_style = Style::new().fg(Color::rgb(1, 2, 3)).bold();
    let double_style = Style::new().fg(Color::rgb(4, 5, 6)).reverse();
    let title_style = Style::new().fg(Color::rgb(7, 8, 9)).underline();
    let mut frame = Frame::new(12, 4);

    Panel::new("S")
        .border_set(BorderSet::square())
        .border_style(square_style)
        .title_style(title_style)
        .render(&mut frame, Rect::new(0, 0, 6, 4));
    Panel::untitled()
        .border_set(BorderSet::double())
        .border_style(double_style)
        .render(&mut frame, Rect::new(6, 0, 6, 4));

    assert_eq!(
        frame.buffer().get(0, 0),
        Some(&Cell::new('┌', square_style))
    );
    assert_eq!(
        frame.buffer().get(5, 3),
        Some(&Cell::new('┘', square_style))
    );
    assert_eq!(frame.buffer().get(3, 0), Some(&Cell::new('S', title_style)));
    assert_eq!(
        frame.buffer().get(6, 0),
        Some(&Cell::new('╔', double_style))
    );
    assert_eq!(
        frame.buffer().get(11, 3),
        Some(&Cell::new('╝', double_style))
    );
}
