use dragons_tui::{Cell, ChangedCell, Style, render_changed_cells};

#[test]
fn renderer_emits_strikethrough_and_reverse_then_resets_before_plain_cells() {
    let changes = [
        ChangedCell {
            x: 0,
            y: 0,
            previous: None,
            current: Cell::new('X', Style::new().strikethrough().reverse()),
        },
        ChangedCell {
            x: 1,
            y: 0,
            previous: None,
            current: Cell::new('Y', Style::new()),
        },
    ];
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changes, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("\u{1b}[9m"));
    assert!(output.contains("\u{1b}[7m"));
    assert!(output.contains("X\u{1b}[0mY"));
    assert!(output.ends_with("\u{1b}[0m"));
    assert!(!output.contains("\u{1b}[0m\u{1b}[0m"));
}

#[test]
fn renderer_does_not_reset_between_consecutive_cells_with_the_same_extended_style() {
    let style = Style::new().strikethrough().reverse().underline();
    let changes = [
        ChangedCell {
            x: 0,
            y: 0,
            previous: None,
            current: Cell::new('A', style),
        },
        ChangedCell {
            x: 1,
            y: 0,
            previous: None,
            current: Cell::new('B', style),
        },
    ];
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changes, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(!output.contains("A\u{1b}[1;2H\u{1b}[0mB"));
    assert_eq!(output.matches("\u{1b}[9m").count(), 1);
    assert_eq!(output.matches("\u{1b}[7m").count(), 1);
}
