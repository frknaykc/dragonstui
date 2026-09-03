use dragons_tui::is_quit_key;

#[test]
fn only_q_requests_exit() {
    assert!(is_quit_key('q'));
    assert!(!is_quit_key('Q'));
    assert!(!is_quit_key('x'));
}
