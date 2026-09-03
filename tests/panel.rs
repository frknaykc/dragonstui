use dragons_tui::{Cell, Color, Frame, Panel, Rect, Style};

#[test]
fn panel_draws_a_title_and_returns_its_inner_rect() {
    let border = Style::new().fg(Color::Rgb { r: 1, g: 2, b: 3 });
    let title = Style::new().fg(Color::Rgb { r: 4, g: 5, b: 6 }).bold();
    let mut frame = Frame::new(12, 6);

    let inner = Panel::new("Agents")
        .border_style(border)
        .title_style(title)
        .render(&mut frame, Rect::new(1, 1, 10, 4));

    assert_eq!(inner, Rect::new(2, 2, 8, 2));
    assert_eq!(frame.buffer().get(1, 1), Some(&Cell::new('╭', border)));
    assert_eq!(frame.buffer().get(4, 1), Some(&Cell::new('A', title)));
}

#[test]
fn panel_handles_tiny_partial_and_focused_styles() {
    let focused = Style::new().fg(Color::Rgb { r: 9, g: 8, b: 7 }).bold();
    let mut frame = Frame::new(3, 2);

    let tiny_inner = Panel::new("x")
        .border_style(focused)
        .render(&mut frame, Rect::new(0, 0, 1, 1));
    assert_eq!(tiny_inner, Rect::new(1, 1, 0, 0));
    assert_eq!(frame.buffer().get(0, 0), Some(&Cell::new('╭', focused)));

    Panel::untitled()
        .border_style(focused)
        .render(&mut frame, Rect::new(2, 1, 4, 3));
    assert_eq!(frame.buffer().get(2, 1), Some(&Cell::new('╭', focused)));
}

#[test]
fn zero_height_panel_does_not_render_its_title_outside_the_target_rect() {
    let mut frame = Frame::new(8, 1);

    let inner = Panel::new("title").render(&mut frame, Rect::new(0, 0, 8, 0));

    assert_eq!(inner, Rect::new(1, 0, 6, 0));
    for x in 0..8 {
        assert_eq!(frame.buffer().get(x, 0), Some(&Cell::default()));
    }
}
