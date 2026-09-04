use dragons_tui::{InspectorLayout, Rect};

#[test]
fn inspector_layout_resolves_a_horizontal_master_detail_split_with_a_divider() {
    let layout = InspectorLayout::new(60, 24, 32);

    let areas = layout.split(Rect::new(10, 4, 100, 20));

    assert!(areas.is_horizontal());
    assert_eq!(areas.master, Rect::new(10, 4, 60, 20));
    assert_eq!(areas.divider, Some(Rect::new(70, 4, 1, 20)));
    assert_eq!(areas.detail, Rect::new(71, 4, 39, 20));
}

#[test]
fn inspector_layout_clamps_requested_widths_to_both_pane_minimums() {
    let layout = InspectorLayout::new(5, 24, 32);
    let areas = layout.split(Rect::new(0, 0, 80, 12));

    assert!(areas.is_horizontal());
    assert_eq!(areas.master.width, 24);
    assert_eq!(areas.divider.unwrap().width, 1);
    assert_eq!(areas.detail.width, 55);

    let detail_limited = InspectorLayout::new(95, 24, 32).split(Rect::new(0, 0, 80, 12));
    assert_eq!(detail_limited.master.width, 47);
    assert_eq!(detail_limited.detail.width, 32);
}

#[test]
fn inspector_layout_stacks_safely_when_horizontal_minimums_do_not_fit() {
    let layout = InspectorLayout::new(60, 24, 32);
    let areas = layout.split(Rect::new(3, 7, 40, 9));

    assert!(!areas.is_horizontal());
    assert_eq!(areas.divider, None);
    assert_eq!(areas.master, Rect::new(3, 7, 40, 5));
    assert_eq!(areas.detail, Rect::new(3, 12, 40, 4));
    assert_eq!(areas.master.bottom(), areas.detail.y);
}

#[test]
fn inspector_layout_recalculates_without_overflow_for_resizes_and_tiny_rectangles() {
    let layout = InspectorLayout::new(60, 24, 32);
    for (width, height) in [(0, 0), (1, 1), (56, 2), (57, 3), (80, 24), (160, 55)] {
        let parent = Rect::new(u16::MAX - width, u16::MAX - height, width, height);
        let areas = layout.split(parent);
        assert!(areas.master.x >= parent.x && areas.master.right() <= parent.right());
        assert!(areas.detail.x >= parent.x && areas.detail.right() <= parent.right());
        assert!(areas.master.y >= parent.y && areas.master.bottom() <= parent.bottom());
        assert!(areas.detail.y >= parent.y && areas.detail.bottom() <= parent.bottom());
        if let Some(divider) = areas.divider {
            assert_eq!(areas.master.right(), divider.x);
            assert_eq!(divider.right(), areas.detail.x);
        } else {
            assert_eq!(areas.master.bottom(), areas.detail.y);
        }
    }
}
