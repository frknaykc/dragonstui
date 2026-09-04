use dragons_tui::{
    Cell, Color, DiffDocument, DiffLineKind, DiffStyles, DiffViewer, Frame, Rect, Style,
    ViewportState,
};

#[test]
fn unified_diff_tracks_old_and_new_line_numbers_across_multiple_hunks() {
    let diff = DiffDocument::parse_unified(
        "--- before.txt\n+++ after.txt\n@@ -2,3 +2,4 @@\n keep\n-old\n+new\n+added\n tail\n@@ -10,2 +11,1 @@\n context\n-removed\n",
    );

    let rendered = diff.lines();
    assert_eq!(rendered[0].kind(), DiffLineKind::Header);
    assert_eq!(rendered[1].kind(), DiffLineKind::Header);
    assert_eq!(rendered[2].kind(), DiffLineKind::Hunk);
    assert_eq!(rendered[3].numbering(), (Some(2), Some(2)));
    assert_eq!(rendered[4].numbering(), (Some(3), None));
    assert_eq!(rendered[5].numbering(), (None, Some(3)));
    assert_eq!(rendered[6].numbering(), (None, Some(4)));
    assert_eq!(rendered[7].numbering(), (Some(4), Some(5)));
    assert_eq!(rendered[8].kind(), DiffLineKind::Hunk);
    assert_eq!(rendered[9].numbering(), (Some(10), Some(11)));
    assert_eq!(rendered[10].numbering(), (Some(11), None));
}

#[test]
fn diff_viewer_renders_kind_styles_and_uses_the_shared_vertical_viewport() {
    let diff = DiffDocument::parse_unified(
        "@@ -2,2 +2,3 @@\n-old\n+new\n context\n+İstanbul and a deliberately long line\n",
    );
    let styles = DiffStyles {
        header: Style::new().fg(Color::rgb(1, 2, 3)),
        hunk: Style::new().fg(Color::rgb(4, 5, 6)),
        context: Style::new().fg(Color::rgb(7, 8, 9)),
        added: Style::new().fg(Color::rgb(10, 11, 12)),
        deleted: Style::new().fg(Color::rgb(13, 14, 15)),
        gutter: Style::new().fg(Color::rgb(16, 17, 18)),
    };
    let mut viewport = ViewportState::new();
    let mut frame = Frame::new(12, 3);

    DiffViewer::new(&diff).render(&mut frame, Rect::new(0, 0, 12, 3), &mut viewport, styles);

    assert_eq!(
        frame.buffer().get(4, 1),
        Some(&Cell::new('-', styles.deleted))
    );
    assert_eq!(
        frame.buffer().get(4, 2),
        Some(&Cell::new('+', styles.added))
    );
    assert_eq!(viewport.max_scroll(), 2);

    assert!(viewport.end());
    let mut scrolled = Frame::new(8, 1);
    DiffViewer::new(&diff).render(&mut scrolled, Rect::new(0, 0, 8, 1), &mut viewport, styles);
    assert_eq!(
        scrolled.buffer().get(4, 0),
        Some(&Cell::new('+', styles.added))
    );
}

#[test]
fn empty_and_malformed_diffs_remain_unambiguous_and_render_safely() {
    let malformed = DiffDocument::parse_unified("@@ malformed\n+un-numbered\n");
    assert_eq!(malformed.lines()[0].kind(), DiffLineKind::Hunk);
    assert_eq!(malformed.lines()[1].kind(), DiffLineKind::Added);
    assert_eq!(malformed.lines()[1].numbering(), (None, None));

    let styles = DiffStyles {
        header: Style::new(),
        hunk: Style::new(),
        context: Style::new(),
        added: Style::new(),
        deleted: Style::new(),
        gutter: Style::new(),
    };
    let empty = DiffDocument::default();
    let mut viewport = ViewportState::new();
    let mut frame = Frame::new(12, 1);
    DiffViewer::new(&empty).render(&mut frame, Rect::new(0, 0, 12, 1), &mut viewport, styles);
    let rendered = (0..12)
        .map(|x| frame.buffer().get(x, 0).unwrap().character)
        .collect::<String>();
    assert!(rendered.starts_with("(empty diff)"));
}

#[test]
fn diff_viewer_reuses_the_scrollbar_for_overflowing_documents() {
    let diff = DiffDocument::parse_unified("@@ -1,3 +1,3 @@\n one\n-two\n+two\n three\n");
    let styles = DiffStyles {
        header: Style::new(),
        hunk: Style::new(),
        context: Style::new(),
        added: Style::new(),
        deleted: Style::new(),
        gutter: Style::new(),
    };
    let mut viewport = ViewportState::new();
    let mut frame = Frame::new(12, 2);

    let geometry = DiffViewer::new(&diff).render_with_scrollbar(
        &mut frame,
        Rect::new(0, 0, 12, 2),
        &mut viewport,
        styles,
        Style::new(),
        Style::new(),
    );

    assert_eq!(geometry.unwrap().track, Rect::new(11, 0, 1, 2));
}
