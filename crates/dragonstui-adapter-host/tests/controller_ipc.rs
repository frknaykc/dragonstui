use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::Duration,
};

use dragonstui_adapter_host::{
    ActionId, AdapterController, AdapterId, AdapterManagementOutcome, ControllerActionClient,
    ControllerActionOutcome, ControllerClient, ControllerIpcCommand, ControllerIpcServer,
    ControllerIpcStatus, ControllerManagementClient, ControllerManagementClientError,
    ControllerManagementRequest, ControllerManagementResponse, ControllerOperationClient,
    ObservationKind, OperationState, PROTOCOL_VERSION, local_controller_diagnostics,
};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[test]
fn typed_management_transport_round_trips_without_dispatching() {
    let id = AdapterId::new("mock").unwrap();
    let request = ControllerManagementRequest::Restart { id: id.to_string() };
    assert_eq!(
        serde_json::from_str::<ControllerManagementRequest>(
            &serde_json::to_string(&request).unwrap()
        )
        .unwrap(),
        request
    );
    let response = ControllerManagementResponse::Lifecycle {
        outcome: AdapterManagementOutcome::Restarted { id },
    };
    assert_eq!(
        serde_json::from_str::<ControllerManagementResponse>(
            &serde_json::to_string(&response).unwrap()
        )
        .unwrap(),
        response
    );
}

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let nonce = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-controller-ipc-diagnostics-{}-{nonce}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("mock/bin")).unwrap();
        let executable = path.join("mock/bin/mock");
        fs::copy(mock_executable(), &executable).unwrap();
        make_executable(&executable);
        fs::write(
            path.join("mock/adapter.json"),
            format!(
                r#"{{"id":"mock","name":"Mock","version":"1.0.0","protocol_version":{PROTOCOL_VERSION},"executable":"bin/mock"}}"#
            ),
        )
        .unwrap();
        Self { path }
    }

    fn semantic_events() -> Self {
        let root = Self::new();
        let bin = root.path.join("mock/bin");
        fs::rename(bin.join("mock"), bin.join("mock-fixture")).unwrap();
        fs::write(
            bin.join("mock"),
            "#!/bin/sh\nexec \"$(dirname \"$0\")/mock-fixture\" --mode semantic-events \"$@\"\n",
        )
        .unwrap();
        make_executable(&bin.join("mock"));
        root
    }

    fn actions() -> Self {
        let root = Self::new();
        let bin = root.path.join("mock/bin");
        fs::rename(bin.join("mock"), bin.join("mock-fixture")).unwrap();
        fs::write(
            bin.join("mock"),
            "#!/bin/sh\nexec \"$(dirname \"$0\")/mock-fixture\" --mode actions \"$@\"\n",
        )
        .unwrap();
        make_executable(&bin.join("mock"));
        root
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn mock_executable() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap())
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

fn raw_command(
    address: std::net::SocketAddr,
    token: &str,
    command: ControllerIpcCommand,
) -> Result<ControllerIpcStatus, String> {
    serde_json::from_value(raw_command_status(address, token, command)?)
        .map_err(|error| error.to_string())
}

fn raw_command_status(
    address: std::net::SocketAddr,
    token: &str,
    command: ControllerIpcCommand,
) -> Result<serde_json::Value, String> {
    let mut stream = TcpStream::connect(address).unwrap();
    serde_json::to_writer(
        &mut stream,
        &serde_json::json!({ "token": token, "command": command }),
    )
    .unwrap();
    stream.write_all(b"\n").unwrap();
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).unwrap();
    let response: serde_json::Value = serde_json::from_str(&line).unwrap();
    match response.get("status").filter(|status| !status.is_null()) {
        Some(status) => Ok(status.clone()),
        None => Err(response["error"]
            .as_str()
            .unwrap_or("missing error")
            .to_owned()),
    }
}

#[test]
fn controller_ipc_rejects_bad_tokens_and_returns_status_to_authenticated_clients() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(
        std::env::temp_dir().join("dragonstui-controller-ipc-empty"),
        Duration::from_millis(100),
        4,
    );
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_requests(2));
    let id = AdapterId::new("missing").unwrap();

    let denied = ControllerClient::new(address, "wrong-token")
        .status(&id)
        .unwrap_err();
    assert!(denied.to_string().contains("authentication failed"));

    let status = ControllerClient::new(address, "correct-token")
        .status(&id)
        .unwrap();
    assert_eq!(status, ControllerIpcStatus::Missing);

    worker.join().unwrap().unwrap();
}

#[test]
fn controller_ipc_returns_live_diagnostics_only_to_authenticated_clients() {
    let root = TempRoot::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(&root.path, Duration::from_millis(200), 8);
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_requests(4));
    let id = AdapterId::new("mock").unwrap();
    let client = ControllerClient::new(address, "correct-token");

    fs::create_dir_all(root.path.join(".controller")).unwrap();
    fs::write(
        root.path.join(".controller/endpoint.json"),
        format!(r#"{{"address":"{address}","token":"correct-token"}}"#),
    )
    .unwrap();
    client.start(&id).unwrap();
    let diagnostics = local_controller_diagnostics(&root.path, &id)
        .unwrap()
        .unwrap();
    assert_eq!(diagnostics.adapter_id, "mock");
    assert_eq!(diagnostics.version.as_deref(), Some("1.0.0"));
    assert_eq!(diagnostics.protocol, Some(PROTOCOL_VERSION));
    assert_eq!(diagnostics.state, "running");
    assert!(diagnostics.pid.is_some());
    assert!(diagnostics.uptime_millis.is_some());
    assert!(
        diagnostics
            .capabilities
            .iter()
            .any(|item| item == "test.echo")
    );
    assert_eq!(diagnostics.pending_request_count, 0);
    assert!(diagnostics.event_queue_capacity > 0);

    let live_data = client.live_data().unwrap();
    assert!(live_data.events.is_empty());
    assert!(live_data.disconnects.is_empty());

    client.stop(&id).unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn authenticated_controller_live_data_preserves_semantic_mock_observations() {
    let root = TempRoot::semantic_events();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(&root.path, Duration::from_millis(200), 8);
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_forever());
    let id = AdapterId::new("mock").unwrap();
    let client = ControllerClient::new(address, "correct-token");

    client.start(&id).unwrap();
    let mut events = Vec::new();
    for _ in 0..8 {
        thread::sleep(Duration::from_millis(20));
        let snapshot = client.live_data().unwrap();
        events.extend(snapshot.events);
    }
    assert_eq!(events.len(), 6);
    assert!(events.iter().all(|event| {
        event.adapter_id == id
            && event.stream == "observations"
            && event.kind == "fixture"
            && event.payload == serde_json::json!({"sequence": 1})
    }));
    let kinds = events
        .iter()
        .map(|event| {
            event
                .observation
                .as_ref()
                .map(|observation| observation.kind())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            None,
            Some(ObservationKind::Log),
            Some(ObservationKind::Metric),
            Some(ObservationKind::Status),
            Some(ObservationKind::Event),
            Some(ObservationKind::Error),
        ]
    );

    client.stop(&id).unwrap();
    client.shutdown().unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn typed_management_wire_shares_authoritative_state_with_legacy_ipc() {
    let root = TempRoot::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(&root.path, Duration::from_millis(200), 8);
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_requests(10));
    let id = AdapterId::new("mock").unwrap();
    let client = ControllerClient::new(address, "correct-token");

    assert_eq!(
        raw_command(
            address,
            "correct-token",
            ControllerIpcCommand::Management {
                request: ControllerManagementRequest::Start { id: id.to_string() },
            },
        )
        .unwrap(),
        ControllerIpcStatus::Management(ControllerManagementResponse::Lifecycle {
            outcome: AdapterManagementOutcome::Started { id: id.clone() },
        })
    );
    assert_eq!(client.diagnostics(&id).unwrap().unwrap().state, "running");

    assert!(matches!(
        raw_command(
            address,
            "correct-token",
            ControllerIpcCommand::Management {
                request: ControllerManagementRequest::Stop { id: id.to_string() },
            },
        )
        .unwrap(),
        ControllerIpcStatus::Management(ControllerManagementResponse::Lifecycle {
            outcome: AdapterManagementOutcome::Stopped { .. },
        })
    ));
    assert_eq!(client.diagnostics(&id).unwrap().unwrap().state, "stopped");

    client.start(&id).unwrap();
    client.stop(&id).unwrap();
    let stopped = raw_command_status(
        address,
        "correct-token",
        ControllerIpcCommand::Management {
            request: ControllerManagementRequest::Diagnostics { id: id.to_string() },
        },
    )
    .unwrap();
    assert_eq!(
        stopped["Management"]["diagnostics"]["state"], "stopped",
        "typed status: {stopped}"
    );

    assert!(matches!(
        raw_command(
            address,
            "correct-token",
            ControllerIpcCommand::Management {
                request: ControllerManagementRequest::Restart { id: id.to_string() },
            },
        )
        .unwrap(),
        ControllerIpcStatus::Management(ControllerManagementResponse::Lifecycle {
            outcome: AdapterManagementOutcome::Restarted { .. },
        })
    ));
    let restarted = raw_command_status(
        address,
        "correct-token",
        ControllerIpcCommand::Management {
            request: ControllerManagementRequest::Diagnostics { id: id.to_string() },
        },
    )
    .unwrap();
    assert_eq!(restarted["Management"]["diagnostics"]["state"], "running");
    assert!(
        restarted["Management"]["diagnostics"]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "test.echo")
    );

    let unknown = raw_command(
        address,
        "correct-token",
        ControllerIpcCommand::Management {
            request: ControllerManagementRequest::Start {
                id: "missing".into(),
            },
        },
    )
    .unwrap_err();
    assert_eq!(unknown, "unknown adapter missing");
    worker.join().unwrap().unwrap();
}

#[test]
fn malformed_typed_request_returns_a_protocol_error_without_stopping_the_daemon() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(
        std::env::temp_dir().join("dragonstui-controller-ipc-malformed"),
        Duration::from_millis(100),
        4,
    );
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_requests(2));

    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .write_all(b"{\"token\":\"correct-token\",\"command\":{\"command\":\"management\"}\n")
        .unwrap();
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).unwrap();
    assert!(line.contains("error"));

    let id = AdapterId::new("missing").unwrap();
    assert_eq!(
        ControllerClient::new(address, "correct-token")
            .status(&id)
            .unwrap(),
        ControllerIpcStatus::Missing
    );
    worker.join().unwrap().unwrap();
}

#[test]
fn typed_management_client_controls_the_same_daemon_state_as_legacy_ipc() {
    let root = TempRoot::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(&root.path, Duration::from_millis(200), 8);
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_requests(10));
    let id = AdapterId::new("mock").unwrap();
    let typed = ControllerManagementClient::new(address, "correct-token");
    let legacy = ControllerClient::new(address, "correct-token");

    assert_eq!(
        typed.start(&id).unwrap(),
        AdapterManagementOutcome::Started { id: id.clone() }
    );
    assert_eq!(legacy.diagnostics(&id).unwrap().unwrap().state, "running");
    assert_eq!(typed.diagnostics(&id).unwrap().unwrap().state, "running");

    assert_eq!(
        typed.stop(&id).unwrap(),
        AdapterManagementOutcome::Stopped { id: id.clone() }
    );
    assert_eq!(legacy.diagnostics(&id).unwrap().unwrap().state, "stopped");

    typed.start(&id).unwrap();
    legacy.stop(&id).unwrap();
    assert_eq!(typed.diagnostics(&id).unwrap().unwrap().state, "stopped");

    assert_eq!(
        typed.restart(&id).unwrap(),
        AdapterManagementOutcome::Restarted { id: id.clone() }
    );
    let diagnostics = typed.diagnostics(&id).unwrap().unwrap();
    assert_eq!(diagnostics.state, "running");
    assert!(
        diagnostics
            .capabilities
            .iter()
            .any(|item| item == "test.echo")
    );
    worker.join().unwrap().unwrap();
}

#[test]
fn typed_management_client_waits_for_a_lifecycle_timeout_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let id = AdapterId::new("mock").unwrap();
    let response = serde_json::json!({
        "status": ControllerIpcStatus::Management(ControllerManagementResponse::Lifecycle {
            outcome: AdapterManagementOutcome::Started { id: id.clone() },
        }),
        "error": null,
    });
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(&stream).read_line(&mut request).unwrap();
        thread::sleep(Duration::from_millis(2100));
        serde_json::to_writer(&mut stream, &response).unwrap();
        stream.write_all(b"\n").unwrap();
    });

    assert_eq!(
        ControllerManagementClient::new(address, "correct-token")
            .start(&id)
            .unwrap(),
        AdapterManagementOutcome::Started { id }
    );
    worker.join().unwrap();
}

#[test]
fn authenticated_action_client_discovers_invokes_and_reports_typed_outcomes() {
    let root = TempRoot::actions();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(&root.path, Duration::from_millis(200), 8);
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_forever());
    let id = AdapterId::new("mock").unwrap();
    let alpha = ActionId::new("fixture.action.alpha").unwrap();
    let alarming = ActionId::new("fixture.destroy.everything").unwrap();
    let confirmed = ActionId::new("fixture.inspect").unwrap();
    let delayed = ActionId::new("fixture.action.delta").unwrap();
    let missing = ActionId::new("fixture.action.missing").unwrap();
    let legacy = ControllerClient::new(address, "correct-token");
    let client = ControllerActionClient::new(address, "correct-token");

    legacy.start(&id).unwrap();
    assert_eq!(
        client
            .actions(&id)
            .unwrap()
            .into_iter()
            .map(|action| action.id)
            .collect::<Vec<_>>(),
        vec![alpha.clone(), alarming.clone(), confirmed, delayed]
    );
    assert!(matches!(
        client.invoke(&id, &alpha, serde_json::json!({})).unwrap().outcome,
        ControllerActionOutcome::Succeeded { payload }
            if payload == serde_json::json!({"outcome": "accepted"})
    ));
    assert!(matches!(
        client.invoke(&id, &alarming, serde_json::json!({})).unwrap().outcome,
        ControllerActionOutcome::Failed { code, message }
            if code == "fixture_rejected" && message == "adapter-declared rejection"
    ));
    assert_eq!(
        client
            .invoke(&id, &missing, serde_json::json!({}))
            .unwrap_err()
            .to_string(),
        "unknown action fixture.action.missing for adapter mock"
    );
    assert!(
        ControllerActionClient::new(address, "wrong-token")
            .actions(&id)
            .unwrap_err()
            .to_string()
            .contains("authentication failed")
    );

    legacy.shutdown().unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn authenticated_action_client_preserves_producer_confirmation_policy() {
    let root = TempRoot::actions();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(&root.path, Duration::from_millis(200), 8);
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_forever());
    let id = AdapterId::new("mock").unwrap();
    let legacy = ControllerClient::new(address, "correct-token");
    let client = ControllerActionClient::new(address, "correct-token");

    legacy.start(&id).unwrap();
    assert_eq!(
        client
            .actions(&id)
            .unwrap()
            .into_iter()
            .map(|action| (action.id.to_string(), action.confirmation_required))
            .collect::<Vec<_>>(),
        vec![
            ("fixture.action.alpha".to_owned(), false),
            ("fixture.destroy.everything".to_owned(), false),
            ("fixture.inspect".to_owned(), true),
            ("fixture.action.delta".to_owned(), false),
        ]
    );

    legacy.shutdown().unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn authenticated_operation_client_reads_controller_owned_action_lifecycle() {
    let root = TempRoot::actions();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AdapterController::new(&root.path, Duration::from_millis(200), 8);
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_forever());
    let id = AdapterId::new("mock").unwrap();
    let action_id = ActionId::new("fixture.action.alpha").unwrap();
    let legacy = ControllerClient::new(address, "correct-token");
    let client = ControllerOperationClient::new(address, "correct-token");

    legacy.start(&id).unwrap();
    let operation = client
        .start(&id, &action_id, serde_json::json!({}))
        .unwrap();
    assert!(matches!(operation.state, OperationState::Pending));
    let mut latest = operation;
    for _ in 0..8 {
        latest = client
            .operations()
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == latest.id)
            .unwrap();
        if latest.state.is_terminal() {
            break;
        }
    }
    assert!(matches!(latest.state, OperationState::Succeeded { .. }));

    legacy.shutdown().unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn typed_management_client_distinguishes_auth_unknown_connection_and_protocol_errors() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join("dragonstui-controller-management-errors");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let controller = AdapterController::new(&root, Duration::from_millis(100), 4);
    let server = ControllerIpcServer::new(listener, controller, "correct-token");
    let worker = thread::spawn(move || server.serve_requests(2));
    let missing = AdapterId::new("missing").unwrap();

    let unknown = ControllerManagementClient::new(address, "correct-token")
        .start(&missing)
        .unwrap_err();
    assert!(
        matches!(
            unknown,
            ControllerManagementClientError::AdapterNotFound(ref id) if id == "missing"
        ),
        "unexpected typed unknown error: {unknown:?}"
    );
    assert!(matches!(
        ControllerManagementClient::new(address, "wrong-token")
            .start(&missing)
            .unwrap_err(),
        ControllerManagementClientError::Authentication
    ));
    worker.join().unwrap().unwrap();

    let unavailable = TcpListener::bind("127.0.0.1:0").unwrap();
    let unavailable_address = unavailable.local_addr().unwrap();
    drop(unavailable);
    assert!(matches!(
        ControllerManagementClient::new(unavailable_address, "token")
            .start(&missing)
            .unwrap_err(),
        ControllerManagementClientError::Connection(_)
    ));

    let malformed_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let malformed_address = malformed_listener.local_addr().unwrap();
    let malformed = thread::spawn(move || {
        let (mut stream, _) = malformed_listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(&stream).read_line(&mut request).unwrap();
        stream.write_all(b"not-json\n").unwrap();
    });
    assert!(matches!(
        ControllerManagementClient::new(malformed_address, "token")
            .start(&missing)
            .unwrap_err(),
        ControllerManagementClientError::Protocol(_)
    ));
    malformed.join().unwrap();
    let _ = fs::remove_dir_all(root);
}
