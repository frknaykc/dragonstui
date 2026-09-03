use dragons_tui::{Constraint, Direction, Layout, Rect};

#[test]
fn horizontal_layout_resolves_length_percentage_fill_and_gap() {
    let areas = Layout::horizontal([
        Constraint::Length(20),
        Constraint::Percentage(30),
        Constraint::Fill(1),
        Constraint::Fill(2),
    ])
    .gap(1)
    .split(Rect::new(0, 0, 100, 10));

    assert_eq!(
        areas,
        vec![
            Rect::new(0, 0, 20, 10),
            Rect::new(21, 0, 29, 10),
            Rect::new(51, 0, 16, 10),
            Rect::new(68, 0, 32, 10),
        ]
    );
}

#[test]
fn vertical_layout_preserves_cross_axis_and_places_gaps_between_children() {
    let areas = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
        .gap(1)
        .split(Rect::new(10, 20, 8, 10));

    assert_eq!(
        areas,
        vec![Rect::new(10, 20, 8, 2), Rect::new(10, 23, 8, 7)]
    );
}

#[test]
fn length_and_percentage_constraints_clamp_in_declaration_order() {
    assert_eq!(
        Layout::horizontal([Constraint::Length(7)]).split(Rect::new(0, 0, 20, 1)),
        vec![Rect::new(0, 0, 7, 1)]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Length(3), Constraint::Length(4)])
            .split(Rect::new(0, 0, 20, 1)),
        vec![Rect::new(0, 0, 3, 1), Rect::new(3, 0, 4, 1)]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Length(15), Constraint::Length(15)])
            .split(Rect::new(0, 0, 20, 1)),
        vec![Rect::new(0, 0, 15, 1), Rect::new(15, 0, 5, 1)]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Percentage(80), Constraint::Percentage(80)])
            .split(Rect::new(0, 0, 20, 1)),
        vec![Rect::new(0, 0, 16, 1), Rect::new(16, 0, 4, 1)]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Percentage(100), Constraint::Percentage(100)])
            .split(Rect::new(0, 0, 20, 1)),
        vec![Rect::new(0, 0, 20, 1), Rect::new(20, 0, 0, 1)]
    );
    assert_eq!(
        Layout::horizontal([
            Constraint::Percentage(0),
            Constraint::Percentage(50),
            Constraint::Percentage(200),
        ])
        .split(Rect::new(0, 0, 20, 1)),
        vec![
            Rect::new(0, 0, 0, 1),
            Rect::new(0, 0, 10, 1),
            Rect::new(10, 0, 10, 1),
        ]
    );
}

#[test]
fn fill_constraints_share_remaining_space_with_deterministic_rounding() {
    assert_eq!(
        Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(Rect::new(0, 0, 5, 1)),
        vec![Rect::new(0, 0, 3, 1), Rect::new(3, 0, 2, 1)]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Fill(1), Constraint::Fill(2)]).split(Rect::new(0, 0, 5, 1)),
        vec![Rect::new(0, 0, 2, 1), Rect::new(2, 0, 3, 1)]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Fill(0), Constraint::Fill(1)]).split(Rect::new(0, 0, 5, 1)),
        vec![Rect::new(0, 0, 0, 1), Rect::new(0, 0, 5, 1)]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Length(2), Constraint::Fill(1)])
            .split(Rect::new(0, 0, 5, 1)),
        vec![Rect::new(0, 0, 2, 1), Rect::new(2, 0, 3, 1)]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Percentage(40), Constraint::Fill(1)])
            .split(Rect::new(0, 0, 5, 1)),
        vec![Rect::new(0, 0, 2, 1), Rect::new(2, 0, 3, 1)]
    );
}

#[test]
fn gap_uses_only_between_child_space_and_degrades_when_space_is_tiny() {
    assert_eq!(
        Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1)
        ])
        .gap(1)
        .split(Rect::new(0, 0, 8, 1)),
        vec![
            Rect::new(0, 0, 2, 1),
            Rect::new(3, 0, 2, 1),
            Rect::new(6, 0, 2, 1),
        ]
    );
    assert_eq!(
        Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)])
            .gap(0)
            .split(Rect::new(0, 0, 5, 1)),
        vec![Rect::new(0, 0, 1, 1), Rect::new(1, 0, 4, 1)]
    );

    let tiny = Layout::horizontal([Constraint::Length(10), Constraint::Fill(1)])
        .gap(3)
        .split(Rect::new(0, 0, 1, 1));
    assert_eq!(tiny, vec![Rect::new(0, 0, 0, 1), Rect::new(1, 0, 0, 1)]);
}

#[test]
fn nested_layouts_and_small_rects_preserve_geometry_invariants() {
    let parent = Rect::new(0, 0, 40, 15);
    let root = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .gap(1)
    .split(parent);
    let body = Layout::horizontal([
        Constraint::Length(10),
        Constraint::Fill(1),
        Constraint::Percentage(25),
    ])
    .gap(1)
    .split(root[1]);

    assert_invariants(parent, &root, Direction::Vertical);
    assert_invariants(root[1], &body, Direction::Horizontal);

    for width in [0, 1, 2, 5, 20, 40, 80, 120] {
        for height in [0, 1, 3, 8, 15, 24, 40] {
            let parent = Rect::new(0, 0, width, height);
            let horizontal = Layout::horizontal([
                Constraint::Length(10),
                Constraint::Percentage(80),
                Constraint::Fill(0),
                Constraint::Fill(2),
            ])
            .gap(2)
            .split(parent);
            let vertical = Layout::vertical([
                Constraint::Percentage(200),
                Constraint::Length(10),
                Constraint::Fill(1),
            ])
            .gap(1)
            .split(parent);

            assert_invariants(parent, &horizontal, Direction::Horizontal);
            assert_invariants(parent, &vertical, Direction::Vertical);
        }
    }
}

#[test]
fn layout_bounds_hold_for_extreme_origins_dimensions_and_gaps() {
    for x in [0, 17, u16::MAX - 2] {
        for y in [0, 23, u16::MAX - 2] {
            for (width, height) in [(0, 0), (1, 1), (5, 3), (20, 8), (120, 40)] {
                for gap in [0, 1, 3, u16::MAX] {
                    let parent = Rect::new(x, y, width, height);
                    let horizontal = Layout::horizontal([
                        Constraint::Length(u16::MAX),
                        Constraint::Percentage(200),
                        Constraint::Fill(0),
                        Constraint::Fill(u16::MAX),
                    ])
                    .gap(gap)
                    .split(parent);
                    let vertical = Layout::vertical([
                        Constraint::Percentage(200),
                        Constraint::Length(u16::MAX),
                        Constraint::Fill(1),
                    ])
                    .gap(gap)
                    .split(parent);

                    assert_invariants(parent, &horizontal, Direction::Horizontal);
                    assert_invariants(parent, &vertical, Direction::Vertical);
                }
            }
        }
    }
}

fn assert_invariants(parent: Rect, areas: &[Rect], direction: Direction) {
    for area in areas {
        assert!(area.x >= parent.x);
        assert!(area.y >= parent.y);
        assert!(area.right() <= parent.right());
        assert!(area.bottom() <= parent.bottom());
    }

    for pair in areas.windows(2) {
        match direction {
            Direction::Horizontal => assert!(pair[1].x >= pair[0].right()),
            Direction::Vertical => assert!(pair[1].y >= pair[0].bottom()),
        }
    }
}
