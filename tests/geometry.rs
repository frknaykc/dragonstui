use dragons_tui::{Position, Rect, Size, split_horizontal, split_vertical};

#[test]
fn rect_reports_bounds_and_contains_only_its_cells() {
    let rect = Rect::new(10, 20, 8, 5);

    assert_eq!(rect.position(), Position { x: 10, y: 20 });
    assert_eq!(
        rect.size(),
        Size {
            width: 8,
            height: 5
        }
    );
    assert_eq!(rect.right(), 18);
    assert_eq!(rect.bottom(), 25);
    assert!(rect.contains(Position { x: 10, y: 20 }));
    assert!(rect.contains(Position { x: 17, y: 24 }));
    assert!(!rect.contains(Position { x: 18, y: 24 }));
    assert!(!rect.contains(Position { x: 17, y: 25 }));
}

#[test]
fn rect_inner_shrinks_each_edge_without_underflow() {
    assert_eq!(Rect::new(10, 20, 8, 5).inner(), Rect::new(11, 21, 6, 3));
    assert_eq!(Rect::new(0, 0, 1, 1).inner(), Rect::new(1, 1, 0, 0));
}

#[test]
fn fixed_splits_clamp_to_the_available_space() {
    let rect = Rect::new(0, 0, 10, 8);

    assert_eq!(
        split_horizontal(rect, 3),
        (Rect::new(0, 0, 3, 8), Rect::new(3, 0, 7, 8))
    );
    assert_eq!(
        split_vertical(rect, 2),
        (Rect::new(0, 0, 10, 2), Rect::new(0, 2, 10, 6))
    );
    assert_eq!(
        split_horizontal(rect, 99),
        (Rect::new(0, 0, 10, 8), Rect::new(10, 0, 0, 8))
    );
    assert_eq!(
        split_vertical(Rect::new(0, 0, 0, 0), 1),
        (Rect::new(0, 0, 0, 0), Rect::new(0, 0, 0, 0))
    );
}

#[test]
fn rect_bounds_and_inner_use_saturating_arithmetic_near_u16_maximum() {
    let rect = Rect::new(u16::MAX - 2, u16::MAX - 1, 10, 10);

    assert_eq!(rect.right(), u16::MAX);
    assert_eq!(rect.bottom(), u16::MAX);
    assert_eq!(rect.inner(), Rect::new(u16::MAX - 1, u16::MAX, 8, 8));
}
