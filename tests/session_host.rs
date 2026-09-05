use dragons_tui::{Cell, Frame, Rect, SessionHost, SessionHostState, Style};

#[test]
fn session_host_retains_a_bounded_line_oriented_scrollback_and_surfaces_exit() {
    let mut host = SessionHost::new(3);
    host.mark_running();
    host.push_output("one\ntwo\nthree\nfour");

    assert_eq!(host.lines(), ["two", "three", "four"]);
    host.mark_exited(Some(7));
    assert_eq!(
        host.state(),
        SessionHostState::Exited { exit_code: Some(7) }
    );

    let mut frame = Frame::new(8, 2);
    host.render(
        &mut frame,
        Rect::new(0, 0, 8, 2),
        Style::new(),
        Style::new(),
        Style::new(),
    );
    assert_eq!(
        frame.buffer().get(0, 0),
        Some(&Cell::new('t', Style::new()))
    );
    assert_eq!(
        frame.buffer().get(0, 1),
        Some(&Cell::new('t', Style::new()))
    );
}

#[test]
fn session_host_control_bytes_are_rendered_as_plain_replacements_not_terminal_escapes() {
    let mut host = SessionHost::new(2);
    host.push_output("ready\u{1b}[2J");

    assert_eq!(host.lines(), ["ready�[2J"]);
}
