use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use dragonstui_adapter_host::{
    ActionId, AdapterId, AdapterManager, AdapterOperation, AdapterRuntimeConfig,
    AdapterSessionEvent, AdapterState, Capability, LocalAdapterRoot, ManagerError, OperationState,
    PROTOCOL_VERSION, RpcOutcome,
};
use serde_json::json;

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(name: &str) -> Self {
        let nonce = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-manager-{name}-{}-{nonce}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn adapter(&self, id: &str) {
        let adapter_dir = self.path.join(id);
        fs::create_dir_all(adapter_dir.join("bin")).unwrap();
        fs::copy(mock_executable(), adapter_dir.join("bin/mock")).unwrap();
        make_executable(&adapter_dir.join("bin/mock"));
        fs::write(
            adapter_dir.join("adapter.json"),
            format!(
                r#"{{
  "id": "{id}",
  "name": "Mock {id}",
  "version": "1.0.0",
  "protocol_version": {PROTOCOL_VERSION},
  "executable": "bin/mock"
}}"#
            ),
        )
        .unwrap();
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}

fn mock_executable() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap())
}

fn config(id: &str, mode: &str) -> AdapterRuntimeConfig {
    AdapterRuntimeConfig::new(mock_executable())
        .arg("--id")
        .arg(id)
        .arg("--mode")
        .arg(mode)
        .handshake_timeout(Duration::from_secs(2))
        .event_queue_capacity(8)
        .response_queue_capacity(8)
}

#[test]
fn manager_owns_multi_adapter_lifecycle_diagnostics_crash_isolation_and_restart() {
    let root = TempRoot::new("lifecycle");
    root.adapter("mock-a");
    root.adapter("mock-b");
    let a = AdapterId::new("mock-a").unwrap();
    let b = AdapterId::new("mock-b").unwrap();
    let echo = Capability::new("test.echo").unwrap();

    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    let discovered = manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    assert_eq!(discovered.len(), 2);

    manager
        .start_with_config(&a, config("mock-a", "crash-on-request"))
        .unwrap();
    manager
        .start_with_config(&b, config("mock-b", "normal"))
        .unwrap();
    assert_eq!(manager.providers_for(&echo), vec![a.clone(), b.clone()]);

    let doomed = manager
        .request(
            &a,
            echo.clone(),
            json!({"crash": true}),
            Duration::from_secs(2),
        )
        .unwrap();
    let healthy = manager
        .request(
            &b,
            echo.clone(),
            json!({"ok": true}),
            Duration::from_secs(2),
        )
        .unwrap();

    for _ in 0..20 {
        manager.poll(Duration::from_millis(50));
        if manager.state(&a) == Some(AdapterState::Crashed) {
            break;
        }
    }
    assert_eq!(manager.state(&a), Some(AdapterState::Crashed));
    assert_eq!(manager.state(&b), Some(AdapterState::Running));
    assert_eq!(manager.providers_for(&echo), vec![b.clone()]);

    let diagnostics = manager.diagnostics(&a).unwrap();
    assert_eq!(diagnostics.adapter_id, a);
    assert_eq!(diagnostics.protocol, Some(PROTOCOL_VERSION));
    assert_eq!(diagnostics.state, AdapterState::Crashed);
    assert!(diagnostics.last_error.is_some());
    assert_eq!(diagnostics.pending_request_count, 0);
    assert_eq!(diagnostics.stderr_dropped_line_count, 0);
    assert_eq!(diagnostics.response_queue_capacity, 8);
    assert_eq!(diagnostics.response_queue_len, 1);

    assert!(matches!(
        manager.wait_response(&a, &doomed, Duration::from_millis(20)),
        Err(ManagerError::Rpc(
            dragonstui_adapter_host::RpcError::Crashed
        ))
    ));

    let b_response = manager
        .wait_response(&b, &healthy, Duration::from_secs(2))
        .unwrap();
    assert_eq!(b_response, RpcOutcome::Response(json!({"ok": true})));

    manager
        .restart_with_config(&diagnostics.adapter_id, config("mock-a", "normal"))
        .unwrap();
    assert_eq!(
        manager.state(&diagnostics.adapter_id),
        Some(AdapterState::Running)
    );
    assert_eq!(
        manager.providers_for(&echo),
        vec![diagnostics.adapter_id.clone(), b]
    );
}

#[test]
fn unregister_stops_a_running_adapter_and_clears_its_capability_and_state() {
    let root = TempRoot::new("unregister");
    root.adapter("mock");
    let id = AdapterId::new("mock").unwrap();
    let echo = Capability::new("test.echo").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock", "normal"))
        .unwrap();
    assert_eq!(manager.providers_for(&echo), vec![id.clone()]);

    manager.unregister(&id).unwrap();

    assert_eq!(manager.state(&id), None);
    assert_eq!(manager.diagnostics(&id), None);
    assert!(manager.providers_for(&echo).is_empty());
    assert!(matches!(
        manager.stop(&id),
        Err(ManagerError::UnknownAdapter(_))
    ));
}

#[test]
fn manager_drains_generic_live_events_with_their_adapter_and_event_identity() {
    let root = TempRoot::new("live-events");
    root.adapter("mock-a");
    root.adapter("mock-b");
    let a = AdapterId::new("mock-a").unwrap();
    let b = AdapterId::new("mock-b").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 16);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&a, config("mock-a", "stress-events"))
        .unwrap();
    manager
        .start_with_config(&b, config("mock-b", "stress-events"))
        .unwrap();

    for _ in 0..12 {
        manager.poll(Duration::from_millis(20));
    }

    let live_data = manager.take_live_data();
    assert!(live_data.disconnects.is_empty());
    assert!(live_data.events.iter().any(|event| {
        event.adapter_id == a
            && event.stream == "stress"
            && event.kind == "tick"
            && event.payload["index"].is_number()
    }));
    assert!(live_data.events.iter().any(|event| {
        event.adapter_id == b
            && event.stream == "stress"
            && event.kind == "tick"
            && event.payload["index"].is_number()
    }));
}

#[test]
fn deterministic_mock_live_event_reaches_the_generic_manager_boundary() {
    let root = TempRoot::new("deterministic-live-event");
    root.adapter("mock-a");
    let id = AdapterId::new("mock-a").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock-a", "live-events"))
        .unwrap();

    for _ in 0..12 {
        manager.poll(Duration::from_millis(20));
    }

    let live_data = manager.take_live_data();
    assert_eq!(live_data.events.len(), 1);
    let event = &live_data.events[0];
    assert_eq!(event.adapter_id, id);
    assert_eq!(event.stream, "live");
    assert_eq!(event.kind, "snapshot");
    assert_eq!(event.payload, serde_json::json!({"sequence": 1}));
}

#[test]
fn manager_discovers_and_invokes_declared_actions_without_identifier_semantics() {
    let root = TempRoot::new("actions");
    root.adapter("mock");
    let id = AdapterId::new("mock").unwrap();
    let alpha = ActionId::new("fixture.action.alpha").unwrap();
    let alarming = ActionId::new("fixture.destroy.everything").unwrap();
    let confirmed = ActionId::new("fixture.inspect").unwrap();
    let delayed = ActionId::new("fixture.action.delta").unwrap();
    let unknown = ActionId::new("fixture.action.missing").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock", "actions"))
        .unwrap();

    assert_eq!(
        manager
            .actions(&id)
            .unwrap()
            .into_iter()
            .map(|action| action.id)
            .collect::<Vec<_>>(),
        vec![alpha.clone(), alarming.clone(), confirmed, delayed]
    );

    let accepted = manager
        .invoke_action(&id, &alpha, json!({}), Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        manager
            .wait_response(&id, &accepted, Duration::from_secs(2))
            .unwrap(),
        RpcOutcome::Response(json!({"outcome": "accepted"}))
    );

    let rejected = manager
        .invoke_action(&id, &alarming, json!({}), Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        manager
            .wait_response(&id, &rejected, Duration::from_secs(2))
            .unwrap(),
        RpcOutcome::AdapterError {
            code: "fixture_rejected".to_owned(),
            message: "adapter-declared rejection".to_owned(),
        }
    );
    assert!(matches!(
        manager.invoke_action(&id, &unknown, json!({}), Duration::from_secs(2)),
        Err(ManagerError::UnknownAction { .. })
    ));
}

#[test]
fn manager_tracks_action_operations_with_authoritative_typed_lifecycle() {
    let root = TempRoot::new("actions");
    root.adapter("mock");
    let id = AdapterId::new("mock").unwrap();
    let alpha = ActionId::new("fixture.action.alpha").unwrap();
    let rejected = ActionId::new("fixture.destroy.everything").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock", "actions"))
        .unwrap();

    let accepted = manager
        .start_action_operation(&id, &alpha, json!({}))
        .unwrap();
    let failed = manager
        .start_action_operation(&id, &rejected, json!({}))
        .unwrap();
    assert!(matches!(accepted.state, OperationState::Pending));
    assert!(matches!(failed.state, OperationState::Pending));

    for _ in 0..8 {
        manager.poll(Duration::from_millis(20));
    }
    let operations = manager.operations();
    assert!(matches!(
        operations.iter().find(|operation| operation.id == accepted.id),
        Some(AdapterOperation {
            state: OperationState::Succeeded { payload },
            ..
        }) if payload == &json!({"outcome": "accepted"})
    ));
    assert!(matches!(
        operations.iter().find(|operation| operation.id == failed.id),
        Some(AdapterOperation {
            state: OperationState::Failed { code, message },
            ..
        }) if code == "fixture_rejected" && message == "adapter-declared rejection"
    ));
}

#[test]
fn manager_exposes_running_before_a_delayed_action_reaches_its_typed_terminal_state() {
    let root = TempRoot::new("delayed-action");
    root.adapter("mock");
    let id = AdapterId::new("mock").unwrap();
    let delayed = ActionId::new("fixture.action.delta").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock", "actions"))
        .unwrap();

    let operation = manager
        .start_action_operation(&id, &delayed, json!({}))
        .unwrap();
    assert!(matches!(operation.state, OperationState::Pending));

    manager.poll(Duration::ZERO);
    assert!(matches!(
        manager
            .operations()
            .iter()
            .find(|current| current.id == operation.id),
        Some(AdapterOperation {
            state: OperationState::Running,
            ..
        })
    ));

    for _ in 0..20 {
        manager.poll(Duration::from_millis(25));
        if matches!(
            manager
                .operations()
                .iter()
                .find(|current| current.id == operation.id),
            Some(AdapterOperation {
                state: OperationState::Succeeded { .. },
                ..
            })
        ) {
            break;
        }
    }
    assert!(matches!(
        manager
            .operations()
            .iter()
            .find(|current| current.id == operation.id),
        Some(AdapterOperation {
            state: OperationState::Succeeded { payload },
            ..
        }) if payload == &json!({"outcome": "delayed"})
    ));
}

#[test]
fn manager_evicts_the_oldest_completed_operation_without_evicting_the_newest() {
    let root = TempRoot::new("operation-retention");
    root.adapter("mock");
    let id = AdapterId::new("mock").unwrap();
    let action = ActionId::new("fixture.action.alpha").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock", "actions"))
        .unwrap();

    let mut created = Vec::new();
    for _ in 0..17 {
        let operation = manager
            .start_action_operation(&id, &action, json!({}))
            .unwrap();
        created.push(operation.id.clone());
        for _ in 0..8 {
            manager.poll(Duration::from_millis(20));
            if manager
                .operations()
                .iter()
                .any(|current| current.id == operation.id && current.state.is_terminal())
            {
                break;
            }
        }
        assert!(
            manager
                .operations()
                .iter()
                .any(|current| current.id == operation.id && current.state.is_terminal())
        );
    }

    let retained = manager.operations();
    assert_eq!(retained.len(), 16);
    assert!(!retained.iter().any(|operation| operation.id == created[0]));
    assert!(retained.iter().any(|operation| operation.id == created[16]));
}

#[test]
fn manager_opens_only_the_provider_declared_session_capability() {
    let root = TempRoot::new("session-open");
    root.adapter("mock");
    let id = AdapterId::new("mock").unwrap();
    let capability = Capability::new("fixture.terminal").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock", "sessions"))
        .unwrap();

    let session = manager
        .open_session(&id, &capability, 24, 80, Duration::from_secs(1))
        .unwrap();

    assert_eq!(session.as_str(), "fixture-session");
    assert!(matches!(
        manager.open_session(
            &id,
            &Capability::new("test.echo").unwrap(),
            24,
            80,
            Duration::from_secs(1),
        ),
        Err(ManagerError::UnknownSessionCapability { .. })
    ));
}

#[test]
fn manager_rejects_duplicate_close_and_input_or_resize_while_close_is_pending() {
    let root = TempRoot::new("session-close-pending");
    root.adapter("mock");
    let id = AdapterId::new("mock").unwrap();
    let capability = Capability::new("fixture.terminal").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock", "sessions"))
        .unwrap();

    let session = manager
        .open_session(&id, &capability, 24, 80, Duration::from_secs(1))
        .unwrap();
    manager.close_session(&id, &session).unwrap();

    assert!(matches!(
        manager.close_session(&id, &session),
        Err(ManagerError::SessionClosing(actual)) if actual == session
    ));
    assert!(matches!(
        manager.input_session(&id, &session, "late".to_owned()),
        Err(ManagerError::SessionClosing(actual)) if actual == session
    ));
    assert!(matches!(
        manager.resize_session(&id, &session, 10, 40),
        Err(ManagerError::SessionClosing(actual)) if actual == session
    ));
}

#[test]
fn manager_scopes_provider_session_ids_by_adapter_identity() {
    let root = TempRoot::new("session-id-scope");
    root.adapter("mock-a");
    root.adapter("mock-b");
    let a = AdapterId::new("mock-a").unwrap();
    let b = AdapterId::new("mock-b").unwrap();
    let capability = Capability::new("fixture.terminal").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&a, config("mock-a", "sessions"))
        .unwrap();
    manager
        .start_with_config(&b, config("mock-b", "sessions"))
        .unwrap();

    let first = manager
        .open_session(&a, &capability, 24, 80, Duration::from_secs(1))
        .unwrap();
    let second = manager
        .open_session(&b, &capability, 24, 80, Duration::from_secs(1))
        .unwrap();

    assert_eq!(
        first, second,
        "fixture providers intentionally reuse this ID"
    );
    manager
        .input_session(&a, &first, "from-a".to_owned())
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut second_input_sent = false;
    let mut events = Vec::new();
    while events.len() < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out collecting both provider/session identities"
        );
        manager.poll(Duration::from_millis(20));
        events.extend(manager.take_session_events());
        // Force valid split-poll delivery: B cannot emit until A was consumed.
        if !second_input_sent && !events.is_empty() {
            manager
                .input_session(&b, &second, "from-b".to_owned())
                .unwrap();
            second_input_sent = true;
        }
    }
    assert_eq!(events.len(), 2);
    assert!(events.iter().any(|event| {
        matches!(event, AdapterSessionEvent::Output { adapter_id, session_id, data }
            if adapter_id == &a && session_id == &first && data == "echo:from-a")
    }));
    assert!(events.iter().any(|event| {
        matches!(event, AdapterSessionEvent::Output { adapter_id, session_id, data }
            if adapter_id == &b && session_id == &second && data == "echo:from-b")
    }));
}

#[test]
fn manager_notifies_owned_session_when_its_adapter_is_stopped() {
    let root = TempRoot::new("session-stop-notification");
    root.adapter("mock");
    let id = AdapterId::new("mock").unwrap();
    let capability = Capability::new("fixture.terminal").unwrap();
    let mut manager = AdapterManager::new(Duration::from_millis(200), 8);
    manager.discover(LocalAdapterRoot::new(&root.path)).unwrap();
    manager
        .start_with_config(&id, config("mock", "sessions"))
        .unwrap();
    let session = manager
        .open_session(&id, &capability, 24, 80, Duration::from_secs(1))
        .unwrap();

    manager.stop(&id).unwrap();

    assert!(matches!(
        manager.take_session_events().as_slice(),
        [AdapterSessionEvent::Disconnected { adapter_id, session_id, reason }]
            if adapter_id == &id && session_id == &session && reason == "adapter stopped"
    ));
}
