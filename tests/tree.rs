use dragons_tui::{Cell, Color, Frame, Line, Rect, Style, Tree, TreeNode, TreeState};

#[test]
fn tree_renders_expanded_rich_nodes_and_selected_child() {
    let normal = Style::new().fg(Color::rgb(1, 2, 3));
    let selected = Style::new().fg(Color::rgb(4, 5, 6)).bg(Color::rgb(7, 8, 9));
    let tree = Tree::new([TreeNode::new(1, Line::from("src")).children([
        TreeNode::new(2, Line::new([dragons_tui::Span::styled("main.rs", normal)])),
        TreeNode::new(3, Line::from("runtime.rs")),
    ])]);
    let mut state = TreeState::new();
    state.expand(1);
    state.set_selected(2);
    let mut frame = Frame::new(20, 3);

    tree.selected_style(selected)
        .render(&mut frame, Rect::new(0, 0, 20, 3), &mut state);

    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '▼');
    assert_eq!(frame.buffer().get(0, 1).unwrap().character, ' ');
    assert_eq!(frame.buffer().get(2, 1).unwrap().character, '•');
    assert_eq!(
        frame.buffer().get(4, 1),
        Some(&Cell::new('m', normal.patch(selected)))
    );
}

#[test]
fn tree_navigation_normalizes_visibility_and_clips_tiny_or_extreme_rects() {
    let tree =
        Tree::new([TreeNode::new(1, "root")
            .children([TreeNode::new(2, "child"), TreeNode::new(3, "sibling")])]);
    let mut state = TreeState::new();

    assert!(tree.move_right(&mut state));
    assert_eq!(state.selected(), Some(1));
    assert!(tree.move_right(&mut state));
    assert_eq!(state.selected(), Some(2));
    assert!(tree.move_down(&mut state));
    assert_eq!(state.selected(), Some(3));
    assert!(tree.move_left(&mut state));
    assert_eq!(state.selected(), Some(1));
    assert!(tree.move_left(&mut state));
    assert!(!state.is_expanded(1));
    assert!(tree.toggle(&mut state));
    assert!(state.is_expanded(1));

    let mut tiny = Frame::new(1, 1);
    tree.render(
        &mut tiny,
        Rect::new(u16::MAX, u16::MAX - 1, 1, 3),
        &mut state,
    );
    assert_eq!(tiny.buffer().get(0, 0), Some(&Cell::default()));

    let replacement = Tree::new([TreeNode::new(9, "replacement")]);
    let mut replacement_frame = Frame::new(16, 1);
    replacement.render(&mut replacement_frame, Rect::new(0, 0, 16, 1), &mut state);
    assert_eq!(state.selected(), Some(9));
}
