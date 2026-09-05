//! M66 combined reference-fixture acceptance through the real authenticated IPC.
//! This is not a provider conformance suite. Confirmation remains UI policy:
//! the public controller API has no `confirmed` argument or refusal outcome.
#![cfg(unix)]

use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use dragonstui_adapter_host::{
    ActionId, AdapterController, AdapterEvent, AdapterId, AdapterManagementOutcome,
    AdapterOperation, AdapterSessionEvent, AdapterState, Capability, ControllerActionClient,
    ControllerActionOutcome, ControllerClient, ControllerIpcServer, ControllerManagementClient,
    ControllerOperationClient, ControllerSessionClient, Observation, ObservationKind,
    ObservationSeverity, ObservationStatus, OperationId, OperationState, PROTOCOL_VERSION,
    SessionId,
};
use serde_json::json;

static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);
const WAIT: Duration = Duration::from_millis(1_500);
const TOKEN: &str = "isolated-reference-test-token";

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(id: &AdapterId) -> Self {
        let nonce = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-reference-controller-{}-{nonce}",
            std::process::id()
        ));
        // Acquire cleanup ownership only after creating our own directory.
        fs::create_dir(&path).unwrap();
        let root = Self(path);
        let bin = root.0.join(id.as_str()).join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::copy(
            std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap(),
            bin.join("mock-fixture"),
        )
        .unwrap();
        fs::set_permissions(bin.join("mock-fixture"), fs::Permissions::from_mode(0o755)).unwrap();
        // All marker paths are derived from the isolated wrapper location, not
        // interpolated shell paths or the user's installed adapter root.
        fs::write(
            bin.join("reference"),
            format!(
                "#!/bin/sh\nbase=\"$(dirname \"$0\")/../..\"\nexec \"$(dirname \"$0\")/mock-fixture\" --mode reference --id {id} --action-marker \"$base/actions\" --session-marker \"$base/sessions\" --event-release \"$base/events.release\" --action-release \"$base/actions.release\"\n"
            ),
        )
        .unwrap();
        fs::set_permissions(bin.join("reference"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            root.0.join(id.as_str()).join("adapter.json"),
            serde_json::to_vec(&json!({
                "id": id,
                "name": "Combined reference fixture",
                "version": "1.0.0",
                "protocol_version": PROTOCOL_VERSION,
                "executable": "bin/reference"
            }))
            .unwrap(),
        )
        .unwrap();
        root
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Harness {
    root: TempRoot,
    id: AdapterId,
    management: ControllerManagementClient,
    actions: ControllerActionClient,
    operations: ControllerOperationClient,
    sessions: ControllerSessionClient,
    control: ControllerClient,
    address: std::net::SocketAddr,
    finished: Receiver<Result<(), String>>,
    worker: Option<JoinHandle<()>>,
}

impl Harness {
    fn new() -> Self {
        let id = AdapterId::new("reference-controller").unwrap();
        let root = TempRoot::new(&id);
        let mut controller = AdapterController::new(&root.0, Duration::from_millis(200), 128);
        controller.discover().unwrap();
        assert_eq!(controller.state(&id), Some(AdapterState::Discovered));
        let discovered = controller.diagnostics(&id).unwrap();
        assert!(
            discovered.pid.is_none(),
            "discovery must not launch the fixture"
        );
        assert!(discovered.capabilities.is_empty());
        assert!(!root.0.join("actions").exists());
        assert!(!root.0.join("sessions").exists());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = ControllerIpcServer::new(listener, controller, TOKEN);
        let (sender, finished) = mpsc::channel();
        let worker = thread::spawn(move || {
            // serve_forever owns and drops the controller (and its children)
            // before this completion notification is sent.
            let result = server.serve_forever().map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        Self {
            root,
            id,
            management: ControllerManagementClient::new(address, TOKEN),
            actions: ControllerActionClient::new(address, TOKEN),
            operations: ControllerOperationClient::new(address, TOKEN),
            sessions: ControllerSessionClient::new(address, TOKEN),
            control: ControllerClient::new(address, TOKEN),
            address,
            finished,
            worker: Some(worker),
        }
    }

    fn start(&self) {
        assert_eq!(
            self.management.start(&self.id).unwrap(),
            AdapterManagementOutcome::Started {
                id: self.id.clone()
            }
        );
        let diagnostics = self.management.diagnostics(&self.id).unwrap().unwrap();
        assert_eq!(diagnostics.state, "running");
        assert_eq!(diagnostics.adapter_id, self.id.as_str());
        assert_eq!(diagnostics.protocol, Some(PROTOCOL_VERSION));
        assert_eq!(diagnostics.version.as_deref(), Some("1.0.0"));
        assert!(diagnostics.pid.is_some());
        let mut capabilities = diagnostics.capabilities;
        capabilities.sort();
        assert_eq!(
            capabilities,
            [
                "fixture.terminal",
                "test.crash",
                "test.echo",
                "test.fail",
                "test.slow",
                "test.stream"
            ]
        );
    }

    fn release(&self, name: &str) {
        fs::write(self.root.0.join(name), "released\n").unwrap();
    }

    fn operation(&self, id: &OperationId) -> AdapterOperation {
        self.operations
            .operations()
            .unwrap()
            .into_iter()
            .find(|operation| &operation.id == id)
            .expect("controller must retain this operation")
    }

    fn wait_operation(
        &self,
        id: &OperationId,
        state: fn(&OperationState) -> bool,
    ) -> AdapterOperation {
        until("expected operation state", || {
            let operation = self.operation(id);
            state(&operation.state).then_some(operation)
        })
    }

    fn session_event(&self) -> AdapterSessionEvent {
        until("provider session event", || {
            let events = self.sessions.events().unwrap();
            if events.is_empty() {
                None
            } else {
                assert_eq!(events.len(), 1, "unexpected session events: {events:?}");
                events.into_iter().next()
            }
        })
    }

    fn assert_provider_sessions(&self, expected: &str) {
        until("provider-owned session registry", || {
            fs::read_to_string(self.root.0.join("sessions"))
                .ok()
                .filter(|contents| contents == expected)
        });
    }

    fn assert_inactive(&self, session: &SessionId) {
        until("controller-owned session cleanup", || {
            (!self.sessions.active(&self.id, session).unwrap()).then_some(())
        });
        self.assert_provider_sessions("");
    }

    fn shutdown(&mut self) -> Result<(), String> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        let request = self.control.shutdown().map_err(|error| error.to_string());
        // Client calls have transport deadlines; never join an unbounded server
        // blindly, including when an assertion unwinds the fixture.
        let result = self
            .finished
            .recv_timeout(Duration::from_secs(5))
            .map_err(|error| format!("reference controller cleanup deadline: {error}"))?;
        worker
            .join()
            .map_err(|_| "reference controller thread panicked".to_owned())?;
        result?;
        request.map(|_| ())
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("reference fixture cleanup failed: {error}");
        }
    }
}

fn until<T>(label: &str, mut sample: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(value) = sample() {
            return value;
        }
        assert!(Instant::now() < deadline, "deadline waiting for {label}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn action(value: &str) -> ActionId {
    ActionId::new(value).unwrap()
}
fn terminal() -> Capability {
    Capability::new("fixture.terminal").unwrap()
}

#[test]
fn reference_ipc_preserves_observations_declarations_and_explicit_action_dispatch() {
    let mut host = Harness::new();
    host.start();
    let declarations = host.actions.actions(&host.id).unwrap();
    assert_eq!(
        declarations
            .iter()
            .map(|action| (action.id.as_str(), action.confirmation_required))
            .collect::<Vec<_>>(),
        [
            ("fixture.action.alpha", false),
            ("fixture.destroy.everything", false),
            ("fixture.inspect", true),
            ("fixture.action.delta", false)
        ]
    );
    assert!(
        declarations
            .iter()
            .all(|action| action.operation.as_str() == "test.echo")
    );
    let sessions = host.sessions.sessions(&host.id).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].capability, terminal());
    assert_eq!(sessions[0].label, "Interactive fixture");

    let mut events: Vec<AdapterEvent> = Vec::new();
    until("first observability batch", || {
        let batch = host.management.live_data().unwrap();
        assert!(batch.disconnects.is_empty());
        events.extend(batch.events);
        (events.len() >= 8).then_some(())
    });
    assert_eq!(events.len(), 8);
    // Read-only discovery/metadata/event operations never dispatch an action.
    assert!(!host.root.0.join("actions").exists());
    assert!(!host.root.0.join("events.release").exists());
    assert!(host.management.live_data().unwrap().events.is_empty());
    host.release("events.release");
    until("second observability batch", || {
        let batch = host.management.live_data().unwrap();
        assert!(batch.disconnects.is_empty());
        events.extend(batch.events);
        (events.len() >= 16).then_some(())
    });
    assert_eq!(events.len(), 16);
    assert!(events.iter().all(|event| event.adapter_id == host.id
        && event.stream == "observations"
        && event.kind == "fixture"
        && event.payload == json!({"sequence": 1})));
    let kinds = events
        .iter()
        .map(|event| event.observation.as_ref().unwrap().kind())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            ObservationKind::Log,
            ObservationKind::Log,
            ObservationKind::Metric,
            ObservationKind::Metric,
            ObservationKind::Metric,
            ObservationKind::Status,
            ObservationKind::Status,
            ObservationKind::Event,
            ObservationKind::Event,
            ObservationKind::Event,
            ObservationKind::Error,
            ObservationKind::Error,
            ObservationKind::Error,
            ObservationKind::Status,
            ObservationKind::Status,
            ObservationKind::Metric
        ]
    );
    assert_eq!(
        events[0].observation,
        Some(Observation::Log {
            text: "fixture log startup".into(),
            severity: Some(ObservationSeverity::Info),
            timestamp_millis: Some(1_700_000_000_101)
        })
    );
    assert_eq!(
        events[4].observation,
        Some(Observation::Metric {
            name: "fixture.value".into(),
            value: (-5).into(),
            unit: Some("items".into()),
            timestamp_millis: Some(1_700_000_000_130)
        })
    );
    assert_eq!(
        events[9].observation,
        Some(Observation::Event {
            title: "fixture arrival".into(),
            detail: None,
            timestamp_millis: None
        })
    );
    assert_eq!(
        events[10].observation,
        Some(Observation::Error {
            message: "fixture error".into(),
            signature: Some("fixture.error".into()),
            stack: vec!["frame one".into(), "frame two".into()],
            timestamp_millis: Some(1_700_000_000_180)
        })
    );
    assert_eq!(
        events[14].observation,
        Some(Observation::Status {
            entity: "fixture-db".into(),
            check: "latency".into(),
            status: ObservationStatus::Unknown,
            timestamp_millis: Some(1_700_000_000_210)
        })
    );

    let inspect = action("fixture.inspect");
    // This is authentication refusal, not invented confirmation enforcement.
    // The caller owns the separate confirmation interaction before invoke().
    let denied = ControllerActionClient::new(host.address, "wrong-token")
        .invoke(&host.id, &inspect, json!({"opaque": "kept by caller"}))
        .unwrap_err();
    assert!(denied.to_string().contains("authentication failed"));
    assert!(!host.root.0.join("actions").exists());
    let response = host
        .actions
        .invoke(&host.id, &inspect, json!({"opaque": "kept by caller"}))
        .unwrap();
    assert_eq!(response.adapter_id, host.id);
    assert_eq!(response.action_id, inspect);
    assert_eq!(
        response.outcome,
        ControllerActionOutcome::Succeeded {
            payload: json!({"outcome": "confirmed"})
        }
    );
    assert_eq!(
        fs::read_to_string(host.root.0.join("actions")).unwrap(),
        "fixture.inspect\n"
    );
    assert_eq!(
        host.actions
            .invoke(&host.id, &action("fixture.action.alpha"), json!({}))
            .unwrap()
            .outcome,
        ControllerActionOutcome::Succeeded {
            payload: json!({"outcome": "accepted"})
        }
    );
    assert_eq!(
        host.actions
            .invoke(&host.id, &action("fixture.destroy.everything"), json!({}))
            .unwrap()
            .outcome,
        ControllerActionOutcome::Failed {
            code: "fixture_rejected".into(),
            message: "adapter-declared rejection".into()
        }
    );
    assert_eq!(
        fs::read_to_string(host.root.0.join("actions")).unwrap(),
        "fixture.inspect\nfixture.action.alpha\nfixture.destroy.everything\n"
    );
    host.management.stop(&host.id).unwrap();
    assert_eq!(
        host.management
            .diagnostics(&host.id)
            .unwrap()
            .unwrap()
            .state,
        "stopped"
    );
    host.shutdown().unwrap();
}

#[test]
fn reference_ipc_held_operation_keeps_sessions_responsive_and_cleans_both_registries() {
    let mut host = Harness::new();
    host.start();
    let pending = host
        .operations
        .start(&host.id, &action("fixture.action.delta"), json!({}))
        .unwrap();
    assert_eq!(pending.state, OperationState::Pending);
    assert_eq!(pending.adapter_id, host.id);
    assert_eq!(pending.action_id, action("fixture.action.delta"));
    host.wait_operation(&pending.id, |state| {
        matches!(state, OperationState::Running)
    });
    assert!(!host.root.0.join("actions.release").exists());

    let session = host.sessions.open(&host.id, &terminal(), 24, 80).unwrap();
    host.assert_provider_sessions(&format!("{session}\n"));
    assert!(host.sessions.active(&host.id, &session).unwrap());
    host.sessions
        .input(&host.id, &session, "alpha\u{3}")
        .unwrap();
    assert_eq!(
        host.session_event(),
        AdapterSessionEvent::Output {
            adapter_id: host.id.clone(),
            session_id: session.clone(),
            data: "echo:alpha\u{3}".into()
        }
    );
    host.sessions.resize(&host.id, &session, 10, 40).unwrap();
    assert_eq!(
        host.session_event(),
        AdapterSessionEvent::Output {
            adapter_id: host.id.clone(),
            session_id: session.clone(),
            data: "resized:10x40".into()
        }
    );
    assert_eq!(
        host.actions
            .invoke(&host.id, &action("fixture.action.alpha"), json!({}))
            .unwrap()
            .outcome,
        ControllerActionOutcome::Succeeded {
            payload: json!({"outcome": "accepted"})
        }
    );
    assert_eq!(host.operation(&pending.id).state, OperationState::Running);
    host.release("actions.release");
    assert_eq!(
        host.wait_operation(&pending.id, OperationState::is_terminal)
            .state,
        OperationState::Succeeded {
            payload: json!({"outcome": "delayed"})
        }
    );

    let rejected = host
        .operations
        .start(&host.id, &action("fixture.destroy.everything"), json!({}))
        .unwrap();
    assert_eq!(rejected.state, OperationState::Pending);
    assert_eq!(
        host.wait_operation(&rejected.id, OperationState::is_terminal)
            .state,
        OperationState::Failed {
            code: "fixture_rejected".into(),
            message: "adapter-declared rejection".into()
        }
    );
    host.sessions.close(&host.id, &session).unwrap();
    assert_eq!(
        host.session_event(),
        AdapterSessionEvent::Exited {
            adapter_id: host.id.clone(),
            session_id: session.clone(),
            exit_code: None
        }
    );
    host.assert_inactive(&session);
    assert!(
        host.sessions
            .input(&host.id, &session, "after-close")
            .is_err()
    );
    assert!(host.sessions.resize(&host.id, &session, 20, 60).is_err());

    let next = host.sessions.open(&host.id, &terminal(), 20, 60).unwrap();
    host.assert_provider_sessions(&format!("{next}\n"));
    host.sessions
        .input(&host.id, &next, "fixture.exit-nonzero")
        .unwrap();
    assert_eq!(
        host.session_event(),
        AdapterSessionEvent::Exited {
            adapter_id: host.id.clone(),
            session_id: next.clone(),
            exit_code: Some(7)
        }
    );
    host.assert_inactive(&next);
    // A third open proves terminal cleanup released admission, not only output.
    let stopped_session = host.sessions.open(&host.id, &terminal(), 12, 30).unwrap();
    host.assert_provider_sessions(&format!("{stopped_session}\n"));
    host.management.stop(&host.id).unwrap();
    host.assert_inactive(&stopped_session);
    let stopped = host.management.diagnostics(&host.id).unwrap().unwrap();
    assert_eq!(stopped.state, "stopped");
    assert!(stopped.pid.is_none());
    assert!(stopped.capabilities.is_empty());
    host.shutdown().unwrap();
}

#[test]
fn reference_ipc_crash_fails_running_operation_disconnects_session_and_restarts_cleanly() {
    let mut host = Harness::new();
    host.start();
    let session = host.sessions.open(&host.id, &terminal(), 24, 80).unwrap();
    host.assert_provider_sessions(&format!("{session}\n"));
    let pending = host
        .operations
        .start(&host.id, &action("fixture.action.delta"), json!({}))
        .unwrap();
    host.wait_operation(&pending.id, |state| {
        matches!(state, OperationState::Running)
    });
    host.sessions
        .input(&host.id, &session, "fixture.crash-provider")
        .unwrap();
    assert_eq!(
        host.session_event(),
        AdapterSessionEvent::Disconnected {
            adapter_id: host.id.clone(),
            session_id: session.clone(),
            reason: "adapter process crashed".into()
        }
    );
    host.assert_inactive(&session);
    let failed = host.wait_operation(&pending.id, OperationState::is_terminal);
    assert!(
        matches!(failed.state, OperationState::Failed { ref code, .. } if code == "adapter_crashed"),
        "{failed:?}"
    );
    let crashed = host.management.diagnostics(&host.id).unwrap().unwrap();
    assert_eq!(crashed.state, "crashed");
    assert!(crashed.last_error.is_some());
    let disconnects = until("typed provider stream disconnect", || {
        let data = host.management.live_data().unwrap();
        (!data.disconnects.is_empty()).then_some(data.disconnects)
    });
    assert_eq!(disconnects.len(), 1);
    assert_eq!(disconnects[0].adapter_id, host.id);
    assert!(!disconnects[0].reason.is_empty());
    assert!(host.management.live_data().unwrap().disconnects.is_empty());
    assert!(
        host.actions
            .invoke(&host.id, &action("fixture.action.alpha"), json!({}))
            .is_err()
    );

    assert_eq!(
        host.management.restart(&host.id).unwrap(),
        AdapterManagementOutcome::Restarted {
            id: host.id.clone()
        }
    );
    let restarted = host.management.diagnostics(&host.id).unwrap().unwrap();
    assert_eq!(restarted.state, "running");
    assert!(restarted.pid.is_some());
    assert!(
        restarted
            .capabilities
            .iter()
            .any(|capability| capability == "fixture.terminal")
    );
    assert_eq!(
        host.operation(&pending.id),
        failed,
        "restart must not rewrite terminal operation history"
    );
    assert!(!host.sessions.active(&host.id, &session).unwrap());
    let reopened = host.sessions.open(&host.id, &terminal(), 18, 64).unwrap();
    host.assert_provider_sessions(&format!("{reopened}\n"));
    host.sessions
        .input(&host.id, &reopened, "after-restart")
        .unwrap();
    assert_eq!(
        host.session_event(),
        AdapterSessionEvent::Output {
            adapter_id: host.id.clone(),
            session_id: reopened.clone(),
            data: "echo:after-restart".into()
        }
    );
    host.sessions.close(&host.id, &reopened).unwrap();
    assert_eq!(
        host.session_event(),
        AdapterSessionEvent::Exited {
            adapter_id: host.id.clone(),
            session_id: reopened.clone(),
            exit_code: None
        }
    );
    host.assert_inactive(&reopened);
    host.management.stop(&host.id).unwrap();
    let stopped = host.management.diagnostics(&host.id).unwrap().unwrap();
    assert_eq!(stopped.state, "stopped");
    assert!(stopped.capabilities.is_empty());
    assert!(stopped.pid.is_none());
    host.shutdown().unwrap();
}
