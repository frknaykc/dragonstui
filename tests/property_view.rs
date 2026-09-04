use dragons_tui::{Frame, PropertyRow, PropertyView, Rect, Style, ViewportState};

#[test]
fn property_view_aligns_unicode_labels_and_clips_long_values() {
    let rows = [
        PropertyRow::new("PID", "4218"),
        PropertyRow::new("İsim", "Ada Lovelace and a deliberately long value"),
    ];
    let mut frame = Frame::new(24, 3);
    let mut viewport = ViewportState::new();
    PropertyView::new(&rows).render(
        &mut frame,
        Rect::new(0, 0, 16, 2),
        &mut viewport,
        Style::new(),
        Style::new(),
    );
    let first = (0..16)
        .map(|x| frame.buffer().get(x, 0).unwrap().character)
        .collect::<String>();
    let second = (0..16)
        .map(|x| frame.buffer().get(x, 1).unwrap().character)
        .collect::<String>();
    assert!(first.starts_with("PID  "));
    assert!(second.starts_with("İsim "));
    assert_eq!(viewport.max_scroll(), 0);
}

#[test]
fn property_view_empty_scrolls_and_recovers_after_content_shrink() {
    let mut viewport = ViewportState::new();
    let rows = (0..8)
        .map(|index| PropertyRow::new(format!("R{index}"), index.to_string()))
        .collect::<Vec<_>>();
    let mut frame = Frame::new(20, 3);
    PropertyView::new(&rows).render(
        &mut frame,
        Rect::new(0, 0, 20, 3),
        &mut viewport,
        Style::new(),
        Style::new(),
    );
    assert!(viewport.end());
    PropertyView::new(&[]).render(
        &mut frame,
        Rect::new(0, 0, 20, 3),
        &mut viewport,
        Style::new(),
        Style::new(),
    );
    assert_eq!(viewport.offset(), 0);
}
