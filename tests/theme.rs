use dragons_tui::{Color, Frame, Line, Rect, Span, Style, Theme};

#[test]
fn theme_is_a_terminal_independent_semantic_colour_value_and_composes_with_rich_spans() {
    let theme = Theme {
        background: Color::rgb(0, 0, 0),
        primary: Color::rgb(1, 2, 3),
        secondary: Color::rgb(4, 5, 6),
        success: Color::rgb(7, 8, 9),
        warning: Color::rgb(10, 11, 12),
        error: Color::rgb(13, 14, 15),
        surface: Color::rgb(16, 17, 18),
        text: Color::rgb(19, 20, 21),
        muted: Color::rgb(22, 23, 24),
    };
    let composed = Style::new().fg(theme.success).patch(Style::new().bold());
    let mut frame = Frame::new(4, 1);

    Line::from([Span::styled("ok", composed)]).render(&mut frame, Rect::new(0, 0, 4, 1));

    assert_eq!(theme.primary, Color::rgb(1, 2, 3));
    assert_eq!(theme.background, Color::rgb(0, 0, 0));
    assert_eq!(theme.success, Color::rgb(7, 8, 9));
    assert_eq!(frame.buffer().get(0, 0).unwrap().style, composed);
    assert!(frame.buffer().get(0, 0).unwrap().style.attributes.bold);
}

#[test]
fn default_theme_exposes_distinct_semantic_colours() {
    let theme = Theme::default();

    assert_ne!(theme.primary, theme.success);
    assert_ne!(theme.warning, theme.error);
    assert_ne!(theme.text, theme.muted);
}

#[test]
fn default_theme_is_the_dragons_fire_palette() {
    let theme = Theme::default();

    assert_eq!(theme.background, Color::rgb(10, 8, 6));
    assert_eq!(theme.surface, Color::rgb(20, 14, 10));
    assert_eq!(theme.primary, Color::rgb(120, 20, 10));
    assert_eq!(theme.secondary, Color::rgb(220, 70, 10));
    assert_eq!(theme.success, Color::rgb(255, 200, 55));
    assert_eq!(theme.warning, Color::rgb(245, 130, 20));
    assert_eq!(theme.error, Color::rgb(180, 35, 10));
    assert_eq!(theme.text, Color::rgb(240, 225, 205));
    assert_eq!(theme.muted, Color::rgb(145, 105, 75));
}
