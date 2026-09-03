use dragons_tui::{CommandId, CommandPalette, KeyCode, KeyEvent, PaletteCommand};

#[test]
fn palette_filters_case_insensitively_navigates_and_executes_the_selected_command() {
    let mut palette = CommandPalette::new([
        PaletteCommand::new(CommandId::new("focus-agents"), "Focus Agents"),
        PaletteCommand::new(CommandId::new("focus-output"), "Focus Output"),
        PaletteCommand::new(CommandId::new("quit"), "Quit"),
    ]);

    assert_eq!(
        palette.filtered_titles(),
        vec!["Focus Agents", "Focus Output", "Quit"]
    );
    assert!(palette.handle_key(KeyEvent::character('o')));
    assert_eq!(palette.query(), "o");
    assert_eq!(
        palette.filtered_titles(),
        vec!["Focus Agents", "Focus Output"]
    );

    assert!(palette.handle_key(KeyEvent {
        code: KeyCode::Down,
        modifiers: Default::default(),
    }));
    assert_eq!(palette.selected_index(), Some(1));
    assert_eq!(
        palette.execute_selected(),
        Some(CommandId::new("focus-output"))
    );
}

#[test]
fn palette_keeps_empty_results_safe_and_ignores_command_modifiers_in_its_query() {
    let mut palette = CommandPalette::new([PaletteCommand::new(
        CommandId::new("focus-input"),
        "Focus Input",
    )]);

    assert!(palette.handle_key(KeyEvent::character('İ')));
    assert_eq!(palette.filtered_titles(), Vec::<String>::new());
    assert_eq!(palette.selected_index(), None);
    assert_eq!(palette.execute_selected(), None);
    assert!(!palette.handle_key(KeyEvent {
        code: KeyCode::Char('p'),
        modifiers: dragons_tui::KeyModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    }));
}
