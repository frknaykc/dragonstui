use dragons_tui::{Buffer, Cell, Color, Style, diff};

#[test]
fn style_patch_merges_colours_and_attributes_without_mutating_the_base_style() {
    let base = Style::new()
        .fg(Color::rgb(1, 2, 3))
        .bg(Color::rgb(4, 5, 6))
        .bold()
        .underline();
    let overlay = Style::new()
        .fg(Color::rgb(7, 8, 9))
        .dim()
        .italic()
        .strikethrough()
        .reverse();

    let patched = base.patch(overlay);

    assert_eq!(base.fg, Some(Color::rgb(1, 2, 3)));
    assert_eq!(base.bg, Some(Color::rgb(4, 5, 6)));
    assert!(base.attributes.bold);
    assert!(base.attributes.underline);
    assert_eq!(patched.fg, Some(Color::rgb(7, 8, 9)));
    assert_eq!(patched.bg, Some(Color::rgb(4, 5, 6)));
    assert!(patched.attributes.bold);
    assert!(patched.attributes.dim);
    assert!(patched.attributes.italic);
    assert!(patched.attributes.underline);
    assert!(patched.attributes.strikethrough);
    assert!(patched.attributes.reverse);

    let mut before = Buffer::new(1, 1);
    let mut after = Buffer::new(1, 1);
    before.set(0, 0, Cell::new('X', base));
    after.set(0, 0, Cell::new('X', patched));
    assert_eq!(diff(Some(&before), &after).len(), 1);
}

#[test]
fn repeated_style_patches_are_deterministic_and_leave_unspecified_values_intact() {
    let base = Style::new().fg(Color::rgb(1, 2, 3));
    let first = base.patch(Style::new().bg(Color::rgb(4, 5, 6)).bold());
    let second = first.patch(Style::new().italic().reverse());

    assert_eq!(second.fg, Some(Color::rgb(1, 2, 3)));
    assert_eq!(second.bg, Some(Color::rgb(4, 5, 6)));
    assert!(second.attributes.bold);
    assert!(second.attributes.italic);
    assert!(second.attributes.reverse);
    assert_eq!(
        second,
        base.patch(Style::new().bg(Color::rgb(4, 5, 6)).bold())
            .patch(Style::new().italic().reverse())
    );
}
