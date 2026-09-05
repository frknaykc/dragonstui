use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use dragonstui_adapter_host::{
    ActionId, AdapterInfo, Capability, Hello, PROTOCOL_VERSION, ProtocolMessage, Request,
    RequestId, SessionClose, SessionId, SessionInput, SessionOpen, SessionResize, Shutdown,
};
use serde_json::{Value, json};

const DEADLINE: Duration = Duration::from_secs(5);

struct Fixture {
    child: Child,
    input: Option<mpsc::SyncSender<ProtocolMessage>>,
    written: Receiver<Result<(), String>>,
    frames: Receiver<Result<ProtocolMessage, String>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
}

impl Fixture {
    fn spawn(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_dragonstui-adapter-host-mock"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let (input, requests) = mpsc::sync_channel::<ProtocolMessage>(1);
        let (ack, written) = mpsc::channel();
        let writer = thread::spawn(move || {
            for message in requests {
                let result = serde_json::to_writer(&mut stdin, &message)
                    .map_err(|e| e.to_string())
                    .and_then(|()| stdin.write_all(b"\n").map_err(|e| e.to_string()))
                    .and_then(|()| stdin.flush().map_err(|e| e.to_string()));
                if ack.send(result).is_err() {
                    break;
                }
            }
        });
        let output = child.stdout.take().unwrap();
        let (tx, frames) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let frame = line.map_err(|e| e.to_string()).and_then(|line| {
                    serde_json::from_str(&line)
                        .map_err(|e| format!("non-atomic frame: {e}: {line}"))
                });
                if tx.send(frame).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input: Some(input),
            written,
            frames,
            reader: Some(reader),
            writer: Some(writer),
        }
    }

    fn send(&mut self, message: ProtocolMessage) {
        // A provider that stops reading must fail the test, not hang a pipe write.
        self.input.as_ref().unwrap().try_send(message).unwrap();
        self.written
            .recv_timeout(DEADLINE)
            .expect("child write deadline")
            .unwrap();
    }

    fn next(&self) -> ProtocolMessage {
        self.frames
            .recv_timeout(DEADLINE)
            .expect("child frame deadline")
            .unwrap()
    }

    fn quiet(&self) {
        assert!(
            matches!(
                self.frames.recv_timeout(Duration::from_millis(80)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "unexpected frame or child exit"
        );
    }

    fn hello(&mut self) -> AdapterInfo {
        self.send(ProtocolMessage::Hello(Hello {
            protocol: PROTOCOL_VERSION,
            host_version: "reference-test".into(),
        }));
        let ProtocolMessage::AdapterInfo(info) = self.next() else {
            panic!("expected adapter_info")
        };
        info
    }

    fn request(&mut self, id: &str, operation: &str, action: Option<&str>, payload: Value) {
        self.send(ProtocolMessage::Request(Request {
            protocol: PROTOCOL_VERSION,
            id: RequestId::new(id).unwrap(),
            operation: Capability::new(operation).unwrap(),
            action: action.map(|v| ActionId::new(v).unwrap()),
            payload,
        }));
    }

    fn response(&self, id: &str, payload: Value) {
        let ProtocolMessage::Response(response) = self.next() else {
            panic!("expected response for {id}")
        };
        assert_eq!(response.id.as_str(), id);
        assert_eq!(response.payload, payload);
    }

    fn error(&self, id: &str, code: &str) {
        let ProtocolMessage::Error(error) = self.next() else {
            panic!("expected error for {id}")
        };
        assert_eq!(error.id.unwrap().as_str(), id);
        assert_eq!(error.code, code);
    }

    fn observations(&self, count: usize) -> Vec<ProtocolMessage> {
        (0..count)
            .map(|_| {
                let message = self.next();
                assert!(
                    matches!(&message, ProtocolMessage::Event(event) if event.observation.is_some())
                );
                message
            })
            .collect()
    }

    fn open(&mut self, id: &str, capability: &str, rows: u16, columns: u16) {
        self.send(ProtocolMessage::SessionOpen(SessionOpen {
            protocol: PROTOCOL_VERSION,
            id: RequestId::new(id).unwrap(),
            capability: Capability::new(capability).unwrap(),
            rows,
            columns,
        }));
    }

    fn opened(&self, id: &str) {
        let ProtocolMessage::SessionOpened(open) = self.next() else {
            panic!("expected session_opened")
        };
        assert_eq!(open.id.as_str(), id);
        assert_eq!(open.session_id, session_id());
    }

    fn input(&mut self, data: &str) {
        self.send(ProtocolMessage::SessionInput(SessionInput {
            protocol: PROTOCOL_VERSION,
            session_id: session_id(),
            data: data.into(),
        }));
    }

    fn output(&self, data: &str) {
        let ProtocolMessage::SessionOutput(output) = self.next() else {
            panic!("expected session_output")
        };
        assert_eq!(output.session_id, session_id());
        assert_eq!(output.data, data);
    }

    fn session_exit(&self, code: Option<i32>) {
        let ProtocolMessage::SessionExit(exit) = self.next() else {
            panic!("expected session_exit")
        };
        assert_eq!(exit.session_id, session_id());
        assert_eq!(exit.exit_code, code);
    }

    fn exited(&mut self) -> ExitStatus {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "child exit deadline");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn shutdown(&mut self) {
        self.send(ProtocolMessage::Shutdown(Shutdown {
            protocol: PROTOCOL_VERSION,
        }));
        assert!(matches!(self.next(), ProtocolMessage::ShutdownAck(_)));
        assert!(self.exited().success());
        assert!(
            matches!(
                self.frames.recv_timeout(DEADLINE),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ),
            "frame after shutdown acknowledgement"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        drop(self.input.take());
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-reference-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn session_id() -> SessionId {
    SessionId::new("fixture-session").unwrap()
}

#[test]
fn reference_combines_rpc_actions_sessions_and_complete_deterministic_observations() {
    let mut fixture = Fixture::spawn(&["--mode", "reference", "--id", "reference-fixture"]);
    let info = fixture.hello();
    assert_eq!(info.id.as_str(), "reference-fixture");
    assert_eq!(
        info.capabilities
            .iter()
            .map(Capability::as_str)
            .collect::<Vec<_>>(),
        [
            "test.echo",
            "test.stream",
            "test.fail",
            "test.slow",
            "test.crash",
            "fixture.terminal"
        ]
    );
    let mut actions = Fixture::spawn(&["--mode", "actions"]);
    assert_eq!(info.actions, actions.hello().actions);
    actions.shutdown();
    let mut sessions = Fixture::spawn(&["--mode", "sessions"]);
    assert_eq!(info.sessions, sessions.hello().sessions);
    sessions.shutdown();

    // Sending RPC immediately makes startup ordering (both batches before RPC) observable.
    fixture.request("echo", "test.echo", None, json!({"nested": [1, "λ", null]}));
    let startup = fixture.observations(16);
    let dir = TempDir::new();
    let release = dir.path("legacy-events-release");
    fs::write(&release, "release").unwrap();
    let mut observations = Fixture::spawn(&[
        "--mode",
        "observability-events",
        "--event-release",
        release.to_str().unwrap(),
    ]);
    observations.hello();
    assert_eq!(startup, observations.observations(16));
    observations.shutdown();
    let mut kinds = BTreeMap::new();
    for message in &startup {
        let ProtocolMessage::Event(event) = message else {
            unreachable!()
        };
        *kinds
            .entry(event.observation.as_ref().unwrap().kind().as_str())
            .or_insert(0) += 1;
    }
    assert_eq!(
        kinds,
        BTreeMap::from([
            ("log", 2),
            ("metric", 4),
            ("status", 4),
            ("event", 3),
            ("error", 3)
        ])
    );
    fixture.response("echo", json!({"nested": [1, "λ", null]}));
    fixture.open("open", "fixture.terminal", 24, 80);
    fixture.opened("open");
    fixture.input("$(not-a-shell)\nλ");
    fixture.output("echo:$(not-a-shell)\nλ");
    fixture.send(ProtocolMessage::SessionResize(SessionResize {
        protocol: PROTOCOL_VERSION,
        session_id: session_id(),
        rows: 31,
        columns: 101,
    }));
    fixture.output("resized:31x101");

    for (id, action, outcome) in [
        ("alpha", "fixture.action.alpha", "accepted"),
        ("inspect", "fixture.inspect", "confirmed"),
        ("delta", "fixture.action.delta", "delayed"),
    ] {
        fixture.request(id, "test.echo", Some(action), json!({}));
        fixture.response(id, json!({"outcome": outcome}));
    }
    fixture.request("stream", "test.stream", None, json!({}));
    assert_eq!(fixture.observations(16), startup);
    let ProtocolMessage::Event(event) = fixture.next() else {
        panic!("generic stream event")
    };
    assert!(event.observation.is_none());
    assert_eq!(
        (event.stream.as_str(), event.kind.as_str()),
        ("test", "started")
    );
    assert_eq!(event.payload, json!({"request": "stream"}));
    fixture.response("stream", json!({"streamed": true}));
    fixture.quiet();
    fixture.send(ProtocolMessage::SessionClose(SessionClose {
        protocol: PROTOCOL_VERSION,
        session_id: session_id(),
    }));
    fixture.session_exit(None);
    fixture.shutdown();
}

#[test]
fn held_delta_is_bounded_and_keeps_rpc_sessions_and_shutdown_responsive() {
    let dir = TempDir::new();
    let release = dir.path("action-release");
    let marker = dir.path("actions");
    let session_marker = dir.path("sessions");
    let mut fixture = Fixture::spawn(&[
        "--mode",
        "reference",
        "--action-release",
        release.to_str().unwrap(),
        "--action-marker",
        marker.to_str().unwrap(),
        "--session-marker",
        session_marker.to_str().unwrap(),
    ]);
    fixture.hello();
    fixture.observations(16);
    fixture.request("held", "test.echo", Some("fixture.action.delta"), json!({}));
    fixture.request("barrier", "test.echo", None, json!("responsive"));
    fixture.response("barrier", json!("responsive"));
    assert_eq!(
        fs::read_to_string(&marker).unwrap(),
        "fixture.action.delta\n"
    );
    for index in 0..20 {
        let id = format!("busy-{index}");
        fixture.request(&id, "test.echo", Some("fixture.action.delta"), json!({}));
        fixture.error(&id, "fixture_action_busy");
    }
    fixture.open("open", "fixture.terminal", 24, 80);
    fixture.opened("open");
    fixture.input("while held");
    fixture.output("echo:while held");
    fixture.quiet();
    fs::write(&release, "release").unwrap();
    fixture.response("held", json!({"outcome": "delayed"}));
    fs::remove_file(&release).unwrap();
    fixture.request(
        "held-again",
        "test.echo",
        Some("fixture.action.delta"),
        json!({}),
    );
    fixture.request("barrier-again", "test.echo", None, json!(true));
    fixture.response("barrier-again", json!(true));
    fixture.shutdown();
    assert_eq!(fs::read_to_string(session_marker).unwrap(), "");
}

#[test]
fn invalid_actions_and_session_opens_do_not_reserve_or_execute_and_failures_recover() {
    let dir = TempDir::new();
    let marker = dir.path("sessions");
    let actions = dir.path("actions");
    let mut fixture = Fixture::spawn(&[
        "--mode",
        "reference",
        "--session-marker",
        marker.to_str().unwrap(),
        "--action-marker",
        actions.to_str().unwrap(),
    ]);
    fixture.hello();
    fixture.observations(16);
    for (id, op, action, error) in [
        (
            "undeclared",
            "test.echo",
            "fixture.unknown",
            "unsupported_action",
        ),
        (
            "mismatch",
            "test.crash",
            "fixture.action.alpha",
            "action_operation_mismatch",
        ),
    ] {
        fixture.request(id, op, Some(action), json!({}));
        fixture.error(id, error);
    }
    assert!(!actions.exists());
    for (id, capability, rows, columns) in [
        ("wrong-cap", "test.echo", 24, 80),
        ("zero-rows", "fixture.terminal", 0, 80),
        ("zero-cols", "fixture.terminal", 24, 0),
    ] {
        fixture.open(id, capability, rows, columns);
        fixture.error(id, "invalid_session_open");
        assert!(!marker.exists());
    }
    fixture.open("valid", "fixture.terminal", 24, 80);
    fixture.opened("valid");
    assert_eq!(fs::read_to_string(&marker).unwrap(), "fixture-session\n");
    fixture.open("busy", "fixture.terminal", 24, 80);
    fixture.error("busy", "fixture_session_busy");
    fixture.request(
        "rejected",
        "test.echo",
        Some("fixture.destroy.everything"),
        json!({}),
    );
    fixture.error("rejected", "fixture_rejected");
    fixture.request("fail", "test.fail", None, json!({}));
    fixture.error("fail", "test_failed");
    fixture.request("unsupported", "test.unknown", None, json!({}));
    fixture.error("unsupported", "unsupported_operation");
    fixture.request("fail", "test.echo", None, json!({}));
    fixture.error("fail", "duplicate_request");
    fixture.input("fixture.exit-nonzero");
    fixture.session_exit(Some(7));
    assert_eq!(fs::read_to_string(&marker).unwrap(), "");
    fixture.open("reopen", "fixture.terminal", 24, 80);
    fixture.opened("reopen");
    fixture.input("\u{5}");
    for sequence in 0..16 {
        fixture.output(&format!("burst:{sequence}\n"));
    }
    fixture.session_exit(Some(2));
    fixture.request("recovered", "test.echo", None, json!(true));
    fixture.response("recovered", json!(true));
    fixture.shutdown();
}

#[test]
fn event_release_and_concurrent_responses_preserve_atomic_json_lines() {
    let dir = TempDir::new();
    let release = dir.path("events");
    let mut fixture = Fixture::spawn(&[
        "--mode",
        "reference",
        "--event-release",
        release.to_str().unwrap(),
    ]);
    fixture.hello();
    fixture.observations(8);
    fixture.request("barrier", "test.echo", None, json!(true));
    fixture.response("barrier", json!(true));
    fixture.quiet();
    // The reader parses every physical line, while the two producer threads compete.
    let payload = json!({"large": "λ\nquoted\"".repeat(4096)});
    fixture.request("traffic-0", "test.echo", None, payload.clone());
    fs::write(release, "release").unwrap();
    for index in 1..64 {
        fixture.request(
            &format!("traffic-{index}"),
            "test.echo",
            None,
            payload.clone(),
        );
    }
    let mut responses = BTreeMap::new();
    let mut events = 0;
    for _ in 0..72 {
        match fixture.next() {
            ProtocolMessage::Response(response) => {
                assert_eq!(response.payload, payload);
                assert!(responses.insert(response.id, ()).is_none());
            }
            ProtocolMessage::Event(event) => {
                assert!(event.observation.is_some());
                events += 1;
            }
            other => panic!("unexpected traffic: {other:?}"),
        }
    }
    assert_eq!(events, 8);
    for index in 0..64 {
        assert!(responses.contains_key(&RequestId::new(format!("traffic-{index}")).unwrap()));
    }
    fixture.quiet();
    fixture.shutdown();
}

#[test]
fn crash_and_slow_are_distinct_controlled_faults_and_session_crash_clears_marker() {
    let mut crash = Fixture::spawn(&["--mode", "reference"]);
    crash.hello();
    crash.observations(16);
    crash.request("crash", "test.crash", None, json!({}));
    assert_eq!(crash.exited().code(), Some(24));
    let mut slow = Fixture::spawn(&["--mode", "reference"]);
    slow.hello();
    slow.observations(16);
    slow.request("slow", "test.slow", None, json!({}));
    slow.request("blocked", "test.echo", None, json!(true));
    assert!(matches!(
        slow.frames.recv_timeout(Duration::from_millis(200)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert!(slow.child.try_wait().unwrap().is_none());
    drop(slow); // Deliberate 30-second timeout fixture is killed/reaped, never awaited.
    let dir = TempDir::new();
    let marker = dir.path("sessions");
    let mut crash = Fixture::spawn(&[
        "--mode",
        "reference",
        "--session-marker",
        marker.to_str().unwrap(),
    ]);
    crash.hello();
    crash.observations(16);
    crash.open("open", "fixture.terminal", 24, 80);
    crash.opened("open");
    crash.input("fixture.crash-provider");
    assert_eq!(crash.exited().code(), Some(37));
    assert_eq!(fs::read_to_string(marker).unwrap(), "");
}

#[test]
fn eof_cancels_held_work_and_clears_provider_session_state() {
    let dir = TempDir::new();
    let action_release = dir.path("action-release");
    let event_release = dir.path("event-release");
    let marker = dir.path("sessions");
    let mut fixture = Fixture::spawn(&[
        "--mode",
        "reference",
        "--action-release",
        action_release.to_str().unwrap(),
        "--event-release",
        event_release.to_str().unwrap(),
        "--session-marker",
        marker.to_str().unwrap(),
    ]);
    fixture.hello();
    fixture.observations(8);
    fixture.request("held", "test.echo", Some("fixture.action.delta"), json!({}));
    fixture.open("open", "fixture.terminal", 24, 80);
    fixture.opened("open");
    drop(fixture.input.take());
    assert!(fixture.exited().success());
    assert_eq!(fs::read_to_string(marker).unwrap(), "");
    assert!(matches!(
        fixture.frames.recv_timeout(DEADLINE),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn legacy_normal_and_action_only_routing_remain_unchanged() {
    let mut normal = Fixture::spawn(&[]);
    let info = normal.hello();
    assert!(info.actions.is_empty() && info.sessions.is_empty());
    assert_eq!(
        info.capabilities
            .iter()
            .map(Capability::as_str)
            .collect::<Vec<_>>(),
        ["test.echo", "test.stream", "test.fail", "test.slow"]
    );
    normal.quiet();
    normal.request("echo", "test.echo", None, json!("legacy"));
    normal.response("echo", json!("legacy"));
    normal.request("stream", "test.stream", None, json!({}));
    assert!(matches!(normal.next(), ProtocolMessage::Event(event) if event.observation.is_none()));
    normal.response("stream", json!({"streamed": true}));
    normal.shutdown();
    let mut actions = Fixture::spawn(&["--mode", "actions"]);
    actions.hello();
    actions.request("plain", "test.echo", None, json!({}));
    actions.error("plain", "unsupported_action");
    // Legacy action mode deliberately ignores operation; reference alone validates it.
    actions.request(
        "legacy-action",
        "test.fail",
        Some("fixture.action.alpha"),
        json!({}),
    );
    actions.response("legacy-action", json!({"outcome": "accepted"}));
    actions.shutdown();
}

#[test]
fn reference_rpc_id_admission_is_bounded_without_blocking_sessions_or_shutdown() {
    let mut fixture = Fixture::spawn(&["--mode", "reference"]);
    fixture.hello();
    fixture.observations(16);
    for index in 0..1024 {
        let id = format!("admitted-{index}");
        // Failed requests also consume the lifetime bookkeeping allowance.
        fixture.request(&id, "test.fail", None, json!({}));
        fixture.error(&id, "test_failed");
    }
    fixture.request("overflow", "test.echo", None, json!({}));
    fixture.error("overflow", "fixture_request_limit");
    fixture.request("overflow-again", "test.crash", None, json!({}));
    fixture.error("overflow-again", "fixture_request_limit");
    fixture.request("admitted-0", "test.echo", None, json!({}));
    fixture.error("admitted-0", "duplicate_request");
    fixture.open("open", "fixture.terminal", 24, 80);
    fixture.opened("open");
    fixture.input("after admission limit");
    fixture.output("echo:after admission limit");
    fixture.shutdown();
}

#[test]
fn reference_shutdown_orders_any_released_batch_before_ack() {
    for release_before_shutdown in [false, true] {
        let dir = TempDir::new();
        let release = dir.path("event-release");
        let mut fixture = Fixture::spawn(&[
            "--mode",
            "reference",
            "--event-release",
            release.to_str().unwrap(),
        ]);
        fixture.hello();
        fixture.observations(8);
        if release_before_shutdown {
            fs::write(&release, "release").unwrap();
        }
        fixture.send(ProtocolMessage::Shutdown(Shutdown {
            protocol: PROTOCOL_VERSION,
        }));
        if !release_before_shutdown {
            fs::write(&release, "release").unwrap();
        }
        let mut acknowledged = false;
        let mut events = 0;
        for _ in 0..9 {
            match fixture.next() {
                ProtocolMessage::Event(_) => events += 1,
                ProtocolMessage::ShutdownAck(_) => {
                    acknowledged = true;
                    break;
                }
                other => panic!("unexpected shutdown frame: {other:?}"),
            }
        }
        assert!(acknowledged);
        assert!(
            events == 0 || events == 8,
            "batch either cancelled or completed before ack"
        );
        assert!(fixture.exited().success());
        assert!(matches!(
            fixture.frames.recv_timeout(DEADLINE),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}

#[test]
fn reference_rejects_incompatible_hello_and_request_versions() {
    let mut incompatible = Fixture::spawn(&["--mode", "reference"]);
    incompatible.send(ProtocolMessage::Hello(Hello {
        protocol: PROTOCOL_VERSION + 1,
        host_version: "incompatible".into(),
    }));
    assert!(matches!(incompatible.next(), ProtocolMessage::Error(error)
        if error.id.is_none() && error.code == "unsupported_protocol"));
    assert!(incompatible.exited().success());

    let mut fixture = Fixture::spawn(&["--mode", "reference"]);
    fixture.hello();
    fixture.observations(16);
    fixture.send(ProtocolMessage::Request(Request {
        protocol: PROTOCOL_VERSION + 1,
        id: RequestId::new("wrong-version").unwrap(),
        operation: Capability::new("test.crash").unwrap(),
        action: None,
        payload: json!({}),
    }));
    fixture.error("wrong-version", "unsupported_protocol");
    fixture.request("still-live", "test.echo", None, json!("live"));
    fixture.response("still-live", json!("live"));
    fixture.shutdown();
}

#[test]
fn cli_help_and_invalid_reference_arguments_terminate_without_handshake() {
    for args in [
        vec!["--mode", "typo"],
        vec!["--mode", "reference", "--unknown"],
        vec!["--mode", "reference", "--action-release"],
        vec!["--mode", "reference", "--id", "INVALID"],
        vec!["--mode", "reference", "--hold-ready", "unused"],
        vec!["--mode", "reference", "--mode", "normal"],
    ] {
        let mut fixture = Fixture::spawn(&args);
        assert_eq!(fixture.exited().code(), Some(64), "{args:?}");
    }
    // Help is plain text rather than a protocol stream, so capture it separately.
    let mut child = Command::new(env!("CARGO_BIN_EXE_dragonstui-adapter-host-mock"))
        .arg("--help")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + DEADLINE;
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("help deadline");
        }
        thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "reference",
        "--action-release",
        "--event-release",
        "test.slow",
        "30",
        "test.crash",
        "24",
    ] {
        assert!(help.contains(expected), "missing {expected}");
    }
}
