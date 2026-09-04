use dragons_tui::{
    Cell, Color, Frame, Line, Rect, SourceDocument, SourceViewer, Span, Style, ViewportState,
};

#[test]
fn source_viewer_preserves_one_based_numbers_styles_and_shared_scrollbar() {
    let keyword = Style::new().fg(Color::Rgb {
        r: 10,
        g: 20,
        b: 30,
    });
    let document = SourceDocument::new([
        Line::from("first"),
        Line::from([Span::styled("İkinci", keyword)]),
        Line::from("third long line"),
        Line::from("fourth"),
    ]);
    let mut viewport = ViewportState::new();
    let mut frame = Frame::new(12, 2);

    let scrollbar = SourceViewer::new(&document).render_with_scrollbar(
        &mut frame,
        Rect::new(0, 0, 12, 2),
        &mut viewport,
        Style::new(),
        Style::new(),
        Style::new(),
    );

    assert_eq!(
        frame.buffer().get(0, 0),
        Some(&Cell::new('1', Style::new()))
    );
    assert_eq!(frame.buffer().get(2, 1), Some(&Cell::new('İ', keyword)));
    assert_eq!(scrollbar.unwrap().track, Rect::new(11, 0, 1, 2));
    assert!(viewport.end());
    let mut scrolled = Frame::new(7, 1);
    SourceViewer::new(&document).render(
        &mut scrolled,
        Rect::new(0, 0, 7, 1),
        &mut viewport,
        Style::new(),
    );
    assert_eq!(
        scrolled.buffer().get(0, 0),
        Some(&Cell::new('4', Style::new()))
    );
    assert_eq!(
        scrolled.buffer().get(2, 0),
        Some(&Cell::new('f', Style::new()))
    );
}

#[test]
fn source_viewer_handles_empty_and_narrow_targets_without_content_inference() {
    let document = SourceDocument::default();
    let mut viewport = ViewportState::new();
    let mut frame = Frame::new(1, 1);
    SourceViewer::new(&document).render(
        &mut frame,
        Rect::new(0, 0, 1, 1),
        &mut viewport,
        Style::new(),
    );
    assert_eq!(
        frame.buffer().get(0, 0),
        Some(&Cell::new('(', Style::new()))
    );
}
