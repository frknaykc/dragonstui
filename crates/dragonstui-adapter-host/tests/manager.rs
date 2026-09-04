use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use dragonstui_adapter_host::{
    ActionId, AdapterId, AdapterManager, AdapterRuntimeConfig, AdapterState, Capability,
    LocalAdapterRoot, ManagerError, PROTOCOL_VERSION, RpcOutcome,
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
        vec![alpha.clone(), alarming.clone(), confirmed]
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
