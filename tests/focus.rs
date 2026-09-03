use dragons_tui::{FocusId, FocusState};

#[test]
fn focus_state_starts_at_the_first_item_and_wraps_in_both_directions() {
    let agents = FocusId::new(1);
    let output = FocusId::new(2);
    let input = FocusId::new(3);
    let mut focus = FocusState::new([agents, output, input]);

    assert_eq!(focus.current(), Some(agents));
    assert!(focus.focus_next());
    assert_eq!(focus.current(), Some(output));
    assert!(focus.focus_next());
    assert_eq!(focus.current(), Some(input));
    assert!(focus.focus_next());
    assert_eq!(focus.current(), Some(agents));
    assert!(focus.focus_previous());
    assert_eq!(focus.current(), Some(input));
}

#[test]
fn focus_state_handles_empty_single_and_unknown_focus_ids_without_panicking() {
    let agents = FocusId::new(1);
    let unknown = FocusId::new(99);
    let mut empty = FocusState::new([]);

    assert_eq!(empty.current(), None);
    assert!(!empty.focus_next());
    assert!(!empty.focus_previous());
    assert!(!empty.set_focus(agents));

    let mut single = FocusState::new([agents]);
    assert_eq!(single.current(), Some(agents));
    assert!(!single.focus_next());
    assert!(!single.focus_previous());
    assert!(!single.set_focus(agents));
    assert!(!single.set_focus(unknown));
    assert_eq!(single.current(), Some(agents));
}

#[test]
fn focus_state_replaces_order_deduplicates_ids_and_normalizes_removed_focus() {
    let agents = FocusId::new(1);
    let output = FocusId::new(2);
    let input = FocusId::new(3);
    let mut focus = FocusState::new([agents, output, agents, output]);

    assert_eq!(focus.current(), Some(agents));
    assert!(focus.focus_next());
    assert_eq!(focus.current(), Some(output));
    assert!(focus.focus_next());
    assert_eq!(focus.current(), Some(agents));
    assert!(focus.set_focus(output));

    assert!(focus.replace_order([agents, input]));
    assert_eq!(focus.current(), Some(agents));
    assert!(!focus.replace_order([input, agents]));
    assert_eq!(focus.current(), Some(agents));
    assert!(focus.replace_order([]));
    assert_eq!(focus.current(), None);
    assert!(!focus.focus_next());
}
