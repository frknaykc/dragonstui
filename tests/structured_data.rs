use dragons_tui::{
    Frame, Position, Rect, Scrollbar, ScrollbarState, StructuredData, Style, TreeState,
};
use serde_json::json;

#[test]
fn structured_data_maps_nested_objects_to_stable_paths_and_expands_them() {
    let value = json!({
        "user": {
            "name": "Ada",
            "active": true,
        }
    });
    let inspector = StructuredData::new(&value);
    let user_id = inspector.id_for_path("root.user").unwrap();
    assert_eq!(
        user_id,
        StructuredData::new(&value)
            .id_for_path("root.user")
            .unwrap()
    );

    let mut state = TreeState::new();
    state.set_selected(inspector.id_for_path("root").unwrap());
    assert!(inspector.move_right(&mut state));
    assert_eq!(state.selected(), inspector.id_for_path("root"));
    assert!(inspector.move_right(&mut state));
    assert_eq!(state.selected(), Some(user_id));
    assert!(inspector.move_right(&mut state));

    let mut frame = Frame::new(40, 8);
    inspector.render(&mut frame, Rect::new(0, 0, 40, 8), &mut state, Style::new());
    let rendered = (0..8)
        .map(|y| {
            (0..40)
                .filter_map(|x| frame.buffer().get(x, y).map(|cell| cell.character))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("user"));
    assert!(rendered.contains("name: \"Ada\""));
    assert!(rendered.contains("active: true"));
}

#[test]
fn structured_data_reuses_tree_viewport_for_scrollbar_interaction() {
    let value = json!({"items": [0, 1, 2, 3, 4, 5]});
    let inspector = StructuredData::new(&value);
    let mut state = TreeState::new();
    state.set_selected(inspector.id_for_path("root").unwrap());
    assert!(inspector.move_right(&mut state));
    state.set_selected(inspector.id_for_path("root.items").unwrap());
    assert!(inspector.move_right(&mut state));

    let mut frame = Frame::new(24, 3);
    inspector.render(&mut frame, Rect::new(0, 0, 23, 3), &mut state, Style::new());
    let track = Rect::new(23, 0, 1, 3);
    assert!(Scrollbar::geometry(state.viewport(), track).is_some());
    let mut scrollbar = ScrollbarState::new();
    assert!(scrollbar.track_click(state.viewport_mut(), track, Position { x: 23, y: 2 }));
    assert!(state.viewport().is_at_bottom());
}

#[test]
fn structured_data_renders_arrays_scalars_empty_containers_and_unicode_without_panicking() {
    let value = json!({
        "empty_array": [],
        "empty_object": {},
        "items": ["Ada", 42, true, null, {"şehir": "İstanbul"}],
    });
    let inspector = StructuredData::new(&value);
    assert!(inspector.id_for_path("root.items[4].şehir").is_some());

    let mut state = TreeState::new();
    state.set_selected(inspector.id_for_path("root").unwrap());
    assert!(inspector.move_right(&mut state));
    state.set_selected(inspector.id_for_path("root.items").unwrap());
    assert!(inspector.move_right(&mut state));
    for _ in 0..5 {
        assert!(inspector.move_down(&mut state));
    }
    assert!(inspector.move_right(&mut state));
    assert!(inspector.move_right(&mut state));

    let mut frame = Frame::new(34, 12);
    inspector.render(
        &mut frame,
        Rect::new(0, 0, 34, 12),
        &mut state,
        Style::new(),
    );
    let rendered = (0..12)
        .map(|y| {
            (0..34)
                .filter_map(|x| frame.buffer().get(x, y).map(|cell| cell.character))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("şehir"));

    for scalar in [json!(null), json!(true), json!(42), json!("Ada")] {
        let mut scalar_frame = Frame::new(20, 2);
        StructuredData::new(&scalar).render(
            &mut scalar_frame,
            Rect::new(0, 0, 20, 2),
            &mut TreeState::new(),
            Style::new(),
        );
    }
}

#[test]
fn structured_data_recovers_selection_when_a_refreshed_payload_removes_its_path() {
    let original = json!({"user": {"name": "Ada"}});
    let original_inspector = StructuredData::new(&original);
    let mut state = TreeState::new();
    state.set_selected(original_inspector.id_for_path("root.user.name").unwrap());

    let refreshed = json!({"status": "ready"});
    let mut frame = Frame::new(28, 5);
    StructuredData::new(&refreshed).render(
        &mut frame,
        Rect::new(0, 0, 28, 5),
        &mut state,
        Style::new(),
    );

    assert_eq!(
        state.selected(),
        StructuredData::new(&refreshed).id_for_path("root")
    );
}
