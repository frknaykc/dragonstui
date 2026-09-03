use dragons_tui::{Canvas, Cell, Color, Frame, Rect, Style, diff};

#[test]
fn canvas_maps_all_eight_logical_braille_dots_to_their_unicode_bits() {
    let dots = [
        (0, 0, '⠁'),
        (0, 1, '⠂'),
        (0, 2, '⠄'),
        (1, 0, '⠈'),
        (1, 1, '⠐'),
        (1, 2, '⠠'),
        (0, 3, '⡀'),
        (1, 3, '⢀'),
    ];

    for (x, y, expected) in dots {
        let mut canvas = Canvas::new(1, 1);
        assert!(canvas.set_point(x, y));
        let mut frame = Frame::new(1, 1);
        canvas.render(&mut frame, Rect::new(0, 0, 1, 1), Style::new());
        assert_eq!(frame.buffer().get(0, 0).unwrap().character, expected);
    }

    let mut full = Canvas::new(1, 1);
    for y in 0..4 {
        for x in 0..2 {
            full.set_point(x, y);
        }
    }
    let mut frame = Frame::new(1, 1);
    full.render(&mut frame, Rect::new(0, 0, 1, 1), Style::new());
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '⣿');
}

#[test]
fn oversized_rectangle_clips_its_visible_edge_without_iterating_the_full_width() {
    let mut canvas = Canvas::new(2, 1);
    canvas.draw_rect(0, 0, u32::MAX, 1);

    let mut frame = Frame::new(2, 1);
    canvas.render(&mut frame, Rect::new(0, 0, 2, 1), Style::default());
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '⠉');
    assert_eq!(frame.buffer().get(1, 0).unwrap().character, '⠉');
}

#[test]
fn points_use_a_two_by_four_logical_grid_and_clear_without_reallocation() {
    let mut canvas = Canvas::new(2, 1);
    assert_eq!((canvas.logical_width(), canvas.logical_height()), (4, 4));
    assert!(canvas.set_point(0, 0));
    assert!(canvas.set_point(3, 3));
    assert!(!canvas.set_point(4, 0));
    assert!(!canvas.set_point(0, 4));

    let mut frame = Frame::new(2, 1);
    canvas.render(&mut frame, Rect::new(0, 0, 2, 1), Style::new());
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '⠁');
    assert_eq!(frame.buffer().get(1, 0).unwrap().character, '⢀');

    canvas.clear();
    canvas.render(&mut frame, Rect::new(0, 0, 2, 1), Style::new());
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, ' ');
    assert_eq!(frame.buffer().get(1, 0).unwrap().character, ' ');

    let mut zero = Canvas::new(0, 0);
    assert!(!zero.set_point(0, 0));
    zero.clear();
    zero.draw_line(-1, -1, 1, 1);
}

#[test]
fn points_combine_in_a_cell_and_remain_independent_across_cells() {
    let mut canvas = Canvas::new(2, 1);
    canvas.set_point(0, 0);
    canvas.set_point(1, 1);
    canvas.set_point(2, 0);

    let mut frame = Frame::new(2, 1);
    canvas.render(&mut frame, Rect::new(0, 0, 2, 1), Style::new());
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '⠑');
    assert_eq!(frame.buffer().get(1, 0).unwrap().character, '⠁');
}

#[test]
fn lines_cover_horizontal_vertical_diagonal_single_reverse_and_clipped_cases() {
    let mut horizontal = Canvas::new(2, 1);
    horizontal.draw_line(0, 0, 3, 0);
    let mut horizontal_frame = Frame::new(2, 1);
    horizontal.render(&mut horizontal_frame, Rect::new(0, 0, 2, 1), Style::new());
    assert_eq!(horizontal_frame.buffer().get(0, 0).unwrap().character, '⠉');
    assert_eq!(horizontal_frame.buffer().get(1, 0).unwrap().character, '⠉');

    let mut vertical = Canvas::new(1, 1);
    vertical.draw_line(0, 0, 0, 3);
    let mut vertical_frame = Frame::new(1, 1);
    vertical.render(&mut vertical_frame, Rect::new(0, 0, 1, 1), Style::new());
    assert_eq!(vertical_frame.buffer().get(0, 0).unwrap().character, '⡇');

    let mut diagonal = Canvas::new(2, 2);
    diagonal.draw_line(0, 0, 3, 7);
    let mut reversed = Canvas::new(2, 2);
    reversed.draw_line(3, 7, 0, 0);
    let mut diagonal_frame = Frame::new(2, 2);
    let mut reversed_frame = Frame::new(2, 2);
    diagonal.render(&mut diagonal_frame, Rect::new(0, 0, 2, 2), Style::new());
    reversed.render(&mut reversed_frame, Rect::new(0, 0, 2, 2), Style::new());
    assert_eq!(diagonal_frame.buffer(), reversed_frame.buffer());

    let mut point = Canvas::new(1, 1);
    point.draw_line(1, 2, 1, 2);
    let mut point_frame = Frame::new(1, 1);
    point.render(&mut point_frame, Rect::new(0, 0, 1, 1), Style::new());
    assert_eq!(point_frame.buffer().get(0, 0).unwrap().character, '⠠');

    let mut clipped = Canvas::new(2, 1);
    clipped.draw_line(-100, 2, 100, 2);
    let mut clipped_frame = Frame::new(2, 1);
    clipped.render(&mut clipped_frame, Rect::new(0, 0, 2, 1), Style::new());
    assert_eq!(clipped_frame.buffer().get(0, 0).unwrap().character, '⠤');
    assert_eq!(clipped_frame.buffer().get(1, 0).unwrap().character, '⠤');

    let mut outside = Canvas::new(1, 1);
    outside.draw_line(i32::MIN, -1, i32::MAX, -1);
    let mut outside_frame = Frame::new(1, 1);
    outside.render(&mut outside_frame, Rect::new(0, 0, 1, 1), Style::new());
    assert_eq!(outside_frame.buffer().get(0, 0).unwrap().character, ' ');
}

#[test]
fn rectangles_draw_an_outline_and_clip_zero_or_outside_edges_safely() {
    let mut normal = Canvas::new(2, 2);
    normal.draw_rect(0, 0, 4, 8);
    let mut normal_frame = Frame::new(2, 2);
    normal.render(&mut normal_frame, Rect::new(0, 0, 2, 2), Style::new());
    for y in 0..2 {
        for x in 0..2 {
            assert_ne!(normal_frame.buffer().get(x, y).unwrap().character, ' ');
        }
    }

    let mut single = Canvas::new(1, 1);
    single.draw_rect(1, 1, 1, 1);
    let mut single_frame = Frame::new(1, 1);
    single.render(&mut single_frame, Rect::new(0, 0, 1, 1), Style::new());
    assert_eq!(single_frame.buffer().get(0, 0).unwrap().character, '⠐');

    let mut zero = Canvas::new(1, 1);
    zero.draw_rect(0, 0, 0, 1);
    zero.draw_rect(0, 0, 1, 0);
    zero.draw_rect(5, 5, 1, 1);
    let mut zero_frame = Frame::new(1, 1);
    zero.render(&mut zero_frame, Rect::new(0, 0, 1, 1), Style::new());
    assert_eq!(zero_frame.buffer().get(0, 0).unwrap().character, ' ');

    let mut clipped = Canvas::new(2, 2);
    clipped.draw_rect(3, 7, 4, 4);
    let mut clipped_frame = Frame::new(2, 2);
    clipped.render(&mut clipped_frame, Rect::new(0, 0, 2, 2), Style::new());
    assert_eq!(clipped_frame.buffer().get(1, 1).unwrap().character, '⢀');
}

#[test]
fn rendering_applies_style_offset_clipping_and_blank_overwrite_diffs() {
    let style = Style::new()
        .fg(Color::rgb(1, 2, 3))
        .bg(Color::rgb(4, 5, 6))
        .bold()
        .underline();
    let mut canvas = Canvas::new(2, 1);
    canvas.set_point(0, 0);
    canvas.set_point(2, 0);

    let mut previous = Frame::new(4, 2);
    canvas.render(&mut previous, Rect::new(1, 1, 1, 1), style);
    assert_eq!(previous.buffer().get(1, 1), Some(&Cell::new('⠁', style)));
    assert_eq!(previous.buffer().get(2, 1), Some(&Cell::default()));

    canvas.clear();
    let mut current = previous.clone();
    canvas.render(&mut current, Rect::new(1, 1, 1, 1), style);
    assert_eq!(current.buffer().get(1, 1), Some(&Cell::new(' ', style)));
    let changed = diff(Some(previous.buffer()), current.buffer());
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].current, Cell::new(' ', style));

    let mut tiny = Frame::new(1, 1);
    canvas.render(&mut tiny, Rect::new(1, 1, 2, 2), style);
    assert_eq!(tiny.buffer().get(0, 0), Some(&Cell::default()));
}
