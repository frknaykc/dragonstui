use dragons_tui::{Cell, ChangedCell, Color, Style, render_changed_cells};

#[test]
fn terminal_renderer_emits_cursor_rgb_attributes_and_character() {
    let style = Style::new()
        .fg(Color::Rgb {
            r: 140,
            g: 200,
            b: 255,
        })
        .bg(Color::Rgb {
            r: 20,
            g: 22,
            b: 30,
        })
        .bold()
        .italic()
        .underline();
    let changes = [ChangedCell {
        x: 1,
        y: 0,
        previous: None,
        current: Cell::new('D', style),
    }];
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changes, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("\u{1b}[1;2H"));
    assert!(output.contains("\u{1b}[38;2;140;200;255m"));
    assert!(output.contains("\u{1b}[48;2;20;22;30m"));
    assert!(output.contains("\u{1b}[1m"));
    assert!(output.contains("\u{1b}[3m"));
    assert!(output.contains("\u{1b}[4m"));
    assert!(output.contains('D'));
}

#[test]
fn terminal_renderer_resets_style_after_a_nonempty_render() {
    let changes = [ChangedCell {
        x: 0,
        y: 0,
        previous: None,
        current: Cell::new('D', Style::new().bold()),
    }];
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changes, false).unwrap();

    assert!(String::from_utf8(output).unwrap().ends_with("\u{1b}[0m"));
}

#[test]
fn terminal_renderer_does_not_emit_duplicate_full_style_resets() {
    let changes = [
        ChangedCell {
            x: 0,
            y: 0,
            previous: None,
            current: Cell::new('A', Style::new().bold()),
        },
        ChangedCell {
            x: 1,
            y: 0,
            previous: None,
            current: Cell::new('B', Style::new().underline()),
        },
    ];
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changes, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(!output.contains("\u{1b}[0m\u{1b}[0m"));
}

#[test]
fn terminal_renderer_coalesces_adjacent_changed_cells_without_losing_style_boundaries() {
    let bold = Style::new().bold();
    let changes = [
        ChangedCell {
            x: 2,
            y: 4,
            previous: None,
            current: Cell::new('A', bold),
        },
        ChangedCell {
            x: 3,
            y: 4,
            previous: None,
            current: Cell::new('B', Style::new().underline()),
        },
    ];
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changes, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("\u{1b}[5;3H"));
    assert!(!output.contains("\u{1b}[5;4H"));
    assert!(output.contains("A\u{1b}[0m\u{1b}[4mB"));
}
