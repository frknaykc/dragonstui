use dragons_tui::{CommandId, KeyCode, KeyEvent, KeyMap, KeyModifiers};

#[test]
fn key_map_resolves_modifiers_and_replaces_conflicts_deterministically() {
    let mut map = KeyMap::new();
    let next = CommandId::new("focus-next");
    let previous = CommandId::new("focus-previous");
    let quit = CommandId::new("quit");

    assert_eq!(
        map.bind(KeyCode::Tab, KeyModifiers::default(), next.clone()),
        None
    );
    assert_eq!(
        map.bind(
            KeyCode::Tab,
            KeyModifiers {
                shift: true,
                ..KeyModifiers::default()
            },
            previous.clone(),
        ),
        None
    );
    assert_eq!(
        map.bind(
            KeyCode::Char('c'),
            KeyModifiers {
                ctrl: true,
                ..KeyModifiers::default()
            },
            quit.clone(),
        ),
        None
    );
    assert_eq!(map.resolve(KeyEvent::character('x')), None);
    assert_eq!(map.resolve(KeyEvent::character('\t')), None);
    assert_eq!(
        map.resolve(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::default(),
        }),
        Some(&next)
    );
    assert_eq!(
        map.bind(KeyCode::Tab, KeyModifiers::default(), quit.clone()),
        Some(next)
    );
    assert_eq!(
        map.resolve(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::default(),
        }),
        Some(&quit)
    );
}
