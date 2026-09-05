use std::{
    collections::{BTreeSet, HashSet},
    env,
    fs::{self, OpenOptions},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use dragonstui_adapter_host::{
    ActionId, AdapterAction, AdapterId, AdapterInfo, AdapterSession, Capability, ErrorMessage,
    Event, Observation, ObservationSeverity, ObservationStatus, PROTOCOL_VERSION, ProtocolMessage,
    Response, SessionClose, SessionExit, SessionId, SessionOpened, SessionOutput, SessionResize,
    ShutdownAck,
};
use serde_json::{Value, json};

// Reference-only lifetime admission bound, not a host/protocol-wide limit.
const REFERENCE_REQUEST_LIMIT: usize = 1024;

fn main() {
    let options = parse_options();
    let behavior = match options.mode.as_str() {
        "process" => return process_mode(),
        "normal" => MockBehavior::Normal,
        "reference" => MockBehavior::Reference,
        "bad-protocol" => MockBehavior::BadProtocol,
        "bad-id" => MockBehavior::BadId,
        "malformed" => {
            println!("{{not json}}");
            flush_stdout();
            return;
        }
        "crash" => process::exit(23),
        "timeout" => return thread::sleep(Duration::from_secs(30)),
        "hold" => {
            hold_before_handshake(&options);
            MockBehavior::Normal
        }
        "duplicate-capabilities" => MockBehavior::DuplicateCapabilities,
        "empty-capabilities" => MockBehavior::EmptyCapabilities,
        "shared-capabilities" => MockBehavior::SharedCapabilities,
        "events" => MockBehavior::Events,
        "live-events" => MockBehavior::LiveEvents,
        "semantic-events" => MockBehavior::SemanticEvents,
        "observability-events" => MockBehavior::ObservabilityEvents,
        "actions" => MockBehavior::Actions,
        "sessions" => MockBehavior::Sessions,
        "delayed-sessions" => MockBehavior::DelayedSessions,
        "stress-events" => MockBehavior::StressEvents,
        "out-of-order" => MockBehavior::OutOfOrder,
        "unknown-response" => MockBehavior::UnknownResponse,
        "crash-after-handshake" => MockBehavior::CrashAfterHandshake,
        "crash-on-request" => MockBehavior::CrashOnRequest,
        _ => usage_error(&format!("unknown mode: {}", options.mode)),
    };
    protocol_mode(
        behavior,
        &options.id,
        options.action_marker.as_deref(),
        options.session_marker.as_deref(),
        options.event_release.as_deref(),
        options.action_release.as_deref(),
    );
}

fn usage_error(message: &str) -> ! {
    eprintln!("{message}; use --help for fixture options");
    process::exit(64);
}

fn parse_options() -> MockOptions {
    let mut args = env::args().skip(1);
    let mut options = MockOptions {
        mode: "normal".to_owned(),
        id: "mock".to_owned(),
        hold_ready: None,
        hold_release: None,
        launch_marker: None,
        action_marker: None,
        session_marker: None,
        event_release: None,
        action_release: None,
    };
    let mut seen = HashSet::new();
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "dragonstui-adapter-host-mock [--mode MODE] [--id ID]\n\
Modes: normal (default), reference, process, bad-protocol, bad-id, malformed,\n\
crash, timeout, hold, duplicate-capabilities, empty-capabilities, shared-capabilities,\n\
events, live-events, semantic-events, observability-events, actions, sessions,\n\
delayed-sessions, stress-events, out-of-order, unknown-response,\n\
crash-after-handshake, crash-on-request\n\
Options: --action-marker PATH, --session-marker PATH, --event-release PATH,\n\
--action-release PATH (reference only), --hold-ready PATH, --hold-release PATH,\n\
--launch-marker PATH (hold mode)\n\
Reference combines RPC, four declared actions, fixture.terminal and observations.\n\
Startup: two synchronous observation batches; --event-release holds batch two\n\
until PATH is a file. test.stream emits both batches plus a generic event and ack.\n\
Delta: one pending action maximum; --action-release waits for a file without\n\
blocking RPC/session/shutdown; otherwise completes after 150 ms. Excess delta\n\
requests return fixture_action_busy. Markers are never consumed by the mock.\n\
Reference remembers at most 1024 unique RPC IDs per process (including failures);\n\
new IDs then receive fixture_request_limit. Duplicate IDs remain duplicate_request.\n\
Sessions and shutdown remain available after this lifetime admission limit.\n\
test.slow deliberately blocks requests for 30 seconds (timeout fixture, no reply);\n\
test.crash exits 24. Sessions echo deterministic input; no shell is executed."
            );
            process::exit(0);
        }
        if !seen.insert(arg.clone()) {
            usage_error(&format!("duplicate option: {arg}"));
        }
        if !matches!(
            arg.as_str(),
            "--mode"
                | "--id"
                | "--hold-ready"
                | "--hold-release"
                | "--launch-marker"
                | "--action-marker"
                | "--session-marker"
                | "--event-release"
                | "--action-release"
        ) {
            usage_error(&format!("unknown option: {arg}"));
        }
        let value = args
            .next()
            .filter(|value| !value.is_empty() && !value.starts_with("--"))
            .unwrap_or_else(|| usage_error(&format!("missing value for {arg}")));
        match arg.as_str() {
            "--mode" => options.mode = value,
            "--id" => options.id = value,
            "--hold-ready" => options.hold_ready = Some(value.into()),
            "--hold-release" => options.hold_release = Some(value.into()),
            "--launch-marker" => options.launch_marker = Some(value.into()),
            "--action-marker" => options.action_marker = Some(value.into()),
            "--session-marker" => options.session_marker = Some(value.into()),
            "--event-release" => options.event_release = Some(value.into()),
            "--action-release" => options.action_release = Some(value.into()),
            _ => unreachable!(),
        }
    }
    if AdapterId::new(&options.id).is_err() {
        usage_error("invalid --id");
    }
    if options.action_release.is_some() && options.mode != "reference" {
        usage_error("--action-release requires --mode reference");
    }
    if options.mode == "reference"
        && (options.hold_ready.is_some()
            || options.hold_release.is_some()
            || options.launch_marker.is_some())
    {
        usage_error("hold options are not supported by reference mode");
    }
    options
}

fn hold_before_handshake(options: &MockOptions) {
    let Some(ready) = options.hold_ready.as_deref() else {
        eprintln!("hold mode requires --hold-ready");
        process::exit(64);
    };
    let Some(release) = options.hold_release.as_deref() else {
        eprintln!("hold mode requires --hold-release");
        process::exit(64);
    };
    let Some(launch_marker) = options.launch_marker.as_deref() else {
        eprintln!("hold mode requires --launch-marker");
        process::exit(64);
    };
    if let Some(parent) = ready.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    if let Some(parent) = launch_marker.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(launch_marker)
        .unwrap()
        .write_all(format!("{}\n", options.id).as_bytes())
        .unwrap();
    fs::write(ready, "ready\n").unwrap();
    while !release.is_file() {
        thread::sleep(Duration::from_millis(5));
    }
}

fn process_mode() {
    eprintln!("diagnostic line");
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let Ok(message) = serde_json::from_str::<ProtocolMessage>(&line) else {
            continue;
        };
        match message {
            ProtocolMessage::Request(request) if request.operation.as_str() == "test.echo" => {
                emit(&ProtocolMessage::Response(Response {
                    protocol: PROTOCOL_VERSION,
                    id: request.id,
                    payload: request.payload,
                }));
            }
            ProtocolMessage::Shutdown(_) => {
                emit(&ProtocolMessage::ShutdownAck(ShutdownAck {
                    protocol: PROTOCOL_VERSION,
                }));
                break;
            }
            _ => {}
        }
    }
}

fn protocol_mode(
    behavior: MockBehavior,
    adapter_id: &str,
    action_marker: Option<&std::path::Path>,
    session_marker: Option<&std::path::Path>,
    event_release: Option<&std::path::Path>,
    action_release: Option<&std::path::Path>,
) {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let Some(Ok(line)) = lines.next() else {
        return;
    };
    let Ok(ProtocolMessage::Hello(hello)) = serde_json::from_str::<ProtocolMessage>(&line) else {
        return;
    };
    if behavior == MockBehavior::Reference && hello.protocol != PROTOCOL_VERSION {
        emit(&ProtocolMessage::Error(ErrorMessage {
            protocol: PROTOCOL_VERSION,
            id: None,
            code: "unsupported_protocol".to_owned(),
            message: "reference adapter requires protocol v1".to_owned(),
        }));
        return;
    }

    let (protocol, id, capabilities) = match behavior {
        MockBehavior::BadProtocol => (PROTOCOL_VERSION + 1, adapter_id, vec!["test.echo"]),
        MockBehavior::BadId => (PROTOCOL_VERSION, "other", vec!["test.echo"]),
        MockBehavior::DuplicateCapabilities => {
            (PROTOCOL_VERSION, adapter_id, vec!["test.echo", "test.echo"])
        }
        MockBehavior::EmptyCapabilities => (PROTOCOL_VERSION, adapter_id, Vec::new()),
        MockBehavior::SharedCapabilities => (
            PROTOCOL_VERSION,
            adapter_id,
            vec!["cap.before", "cap.shared"],
        ),
        MockBehavior::Sessions | MockBehavior::DelayedSessions => {
            (PROTOCOL_VERSION, adapter_id, vec!["fixture.terminal"])
        }
        MockBehavior::Reference => (
            PROTOCOL_VERSION,
            adapter_id,
            vec![
                "test.echo",
                "test.stream",
                "test.fail",
                "test.slow",
                "test.crash",
                "fixture.terminal",
            ],
        ),
        MockBehavior::Normal
        | MockBehavior::Events
        | MockBehavior::LiveEvents
        | MockBehavior::SemanticEvents
        | MockBehavior::ObservabilityEvents
        | MockBehavior::Actions
        | MockBehavior::StressEvents
        | MockBehavior::OutOfOrder
        | MockBehavior::UnknownResponse
        | MockBehavior::CrashAfterHandshake
        | MockBehavior::CrashOnRequest => (
            PROTOCOL_VERSION,
            adapter_id,
            vec!["test.echo", "test.stream", "test.fail", "test.slow"],
        ),
    };

    let actions = if matches!(behavior, MockBehavior::Actions | MockBehavior::Reference) {
        vec![
            AdapterAction {
                id: ActionId::new("fixture.action.alpha").unwrap(),
                label: "Alpha".to_owned(),
                description: Some("Adapter-declared success".to_owned()),
                confirmation_required: false,
                operation: Capability::new("test.echo").unwrap(),
            },
            AdapterAction {
                id: ActionId::new("fixture.destroy.everything").unwrap(),
                label: "Inspect".to_owned(),
                description: Some("Adapter-declared rejection".to_owned()),
                confirmation_required: false,
                operation: Capability::new("test.echo").unwrap(),
            },
            AdapterAction {
                id: ActionId::new("fixture.inspect").unwrap(),
                label: "Confirm inspection".to_owned(),
                description: Some("Adapter-declared confirmation requirement".to_owned()),
                confirmation_required: true,
                operation: Capability::new("test.echo").unwrap(),
            },
            AdapterAction {
                id: ActionId::new("fixture.action.delta").unwrap(),
                label: "Delta".to_owned(),
                description: Some("Adapter-declared delayed completion".to_owned()),
                confirmation_required: false,
                operation: Capability::new("test.echo").unwrap(),
            },
        ]
    } else {
        Vec::new()
    };
    let sessions = if behavior.supports_sessions() {
        vec![AdapterSession {
            capability: Capability::new("fixture.terminal").unwrap(),
            label: "Interactive fixture".to_owned(),
            description: Some("Deterministic provider-declared session".to_owned()),
        }]
    } else {
        Vec::new()
    };
    let info = AdapterInfo {
        protocol,
        id: AdapterId::new(id).unwrap(),
        version: "1.0.0".to_owned(),
        capabilities: capabilities
            .into_iter()
            .map(|capability| Capability::new(capability).unwrap())
            .collect(),
        actions,
        sessions,
    };
    emit(&ProtocolMessage::AdapterInfo(info));

    if behavior == MockBehavior::CrashAfterHandshake {
        process::exit(24);
    }
    if behavior == MockBehavior::UnknownResponse {
        emit(&ProtocolMessage::Response(Response {
            protocol: PROTOCOL_VERSION,
            id: dragonstui_adapter_host::RequestId::new("unknown:1").unwrap(),
            payload: json!({"unknown": true}),
        }));
    }
    if behavior == MockBehavior::LiveEvents {
        emit(&ProtocolMessage::Event(Event {
            protocol: PROTOCOL_VERSION,
            stream: "live".to_owned(),
            kind: "snapshot".to_owned(),
            observation: None,
            payload: json!({"sequence": 1}),
        }));
    }
    if behavior == MockBehavior::SemanticEvents {
        emit(&ProtocolMessage::Event(Event {
            protocol: PROTOCOL_VERSION,
            stream: "observations".to_owned(),
            kind: "fixture".to_owned(),
            observation: None,
            payload: json!({"sequence": 1}),
        }));
        let observations = [
            Observation::Log {
                text: "fixture log".to_owned(),
                severity: Some(ObservationSeverity::Info),
                timestamp_millis: Some(1_700_000_000_001),
            },
            Observation::Metric {
                name: "fixture.value".to_owned(),
                value: 42.into(),
                unit: Some("items".to_owned()),
                timestamp_millis: Some(1_700_000_000_002),
            },
            Observation::Status {
                entity: "fixture-entity".to_owned(),
                check: "fixture-check".to_owned(),
                status: ObservationStatus::Ok,
                timestamp_millis: Some(1_700_000_000_003),
            },
            Observation::Event {
                title: "fixture event".to_owned(),
                detail: Some("fixture detail".to_owned()),
                timestamp_millis: Some(1_700_000_000_004),
            },
            Observation::Error {
                message: "fixture error".to_owned(),
                signature: Some("fixture.error".to_owned()),
                stack: vec!["frame one".to_owned(), "frame two".to_owned()],
                timestamp_millis: Some(1_700_000_000_005),
            },
        ];
        for observation in observations {
            emit(&ProtocolMessage::Event(Event {
                protocol: PROTOCOL_VERSION,
                stream: "observations".to_owned(),
                kind: "fixture".to_owned(),
                observation: Some(observation),
                payload: json!({"sequence": 1}),
            }));
        }
    }
    let mut reference_observations = None;
    if matches!(
        behavior,
        MockBehavior::ObservabilityEvents | MockBehavior::Reference
    ) {
        let (first_batch, second_batch) = observability_fixture_batches();
        emit_observations(first_batch);
        let release = event_release.map(std::path::Path::to_path_buf);
        if behavior == MockBehavior::Reference && release.is_none() {
            // Reference startup is deterministic: no timer races with queued RPC/shutdown.
            emit_observations(second_batch);
        } else if behavior == MockBehavior::Reference {
            reference_observations =
                Some(ReferenceObservations::new(release.unwrap(), second_batch));
        } else {
            thread::spawn(move || {
                if let Some(release) = release {
                    // PTY acceptance releases batch two only after observing batch one.
                    while !release.is_file() {
                        thread::sleep(Duration::from_millis(5));
                    }
                } else {
                    thread::sleep(Duration::from_millis(1_200));
                }
                emit_observations(second_batch);
            });
        }
    }
    if behavior == MockBehavior::StressEvents {
        for index in 0..40 {
            emit(&ProtocolMessage::Event(Event {
                protocol: PROTOCOL_VERSION,
                stream: "stress".to_owned(),
                kind: "tick".to_owned(),
                observation: None,
                payload: json!({"index": index}),
            }));
        }
    }

    let mut reference_actions = (behavior == MockBehavior::Reference)
        .then(|| ReferenceActions::new(action_release.map(Path::to_path_buf)));
    let mut seen_requests = HashSet::new();
    let mut delayed = Vec::new();
    let mut provider_sessions = MockSessionRegistry::new(session_marker);
    for line in lines {
        let Ok(line) = line else {
            break;
        };
        let Ok(message) = serde_json::from_str::<ProtocolMessage>(&line) else {
            emit(&ProtocolMessage::Error(ErrorMessage {
                protocol: PROTOCOL_VERSION,
                id: None,
                code: "malformed".to_owned(),
                message: "malformed request".to_owned(),
            }));
            continue;
        };
        if behavior == MockBehavior::Reference && message.protocol() != PROTOCOL_VERSION {
            let id = match message {
                ProtocolMessage::Request(request) => Some(request.id),
                ProtocolMessage::SessionOpen(open) => Some(open.id),
                _ => None,
            };
            emit(&ProtocolMessage::Error(ErrorMessage {
                protocol: PROTOCOL_VERSION,
                id,
                code: "unsupported_protocol".to_owned(),
                message: "reference adapter requires protocol v1".to_owned(),
            }));
            continue;
        }
        match message {
            ProtocolMessage::SessionOpen(open) if behavior.supports_sessions() => {
                if behavior == MockBehavior::Reference
                    && (open.capability.as_str() != "fixture.terminal"
                        || open.rows == 0
                        || open.columns == 0)
                {
                    emit(&ProtocolMessage::Error(ErrorMessage {
                        protocol: PROTOCOL_VERSION,
                        id: Some(open.id),
                        code: "invalid_session_open".to_owned(),
                        message: "session requires fixture.terminal and nonzero dimensions"
                            .to_owned(),
                    }));
                    continue;
                }
                let session_id = SessionId::new("fixture-session").unwrap();
                match provider_sessions.record_session(session_id.clone()) {
                    Ok(true) => {}
                    Ok(false) => {
                        emit(&ProtocolMessage::Error(ErrorMessage {
                            protocol: PROTOCOL_VERSION,
                            id: Some(open.id),
                            code: "fixture_session_busy".to_owned(),
                            message: "fixture already has an active session".to_owned(),
                        }));
                        continue;
                    }
                    Err(error) => {
                        emit(&ProtocolMessage::Error(ErrorMessage {
                            protocol: PROTOCOL_VERSION,
                            id: Some(open.id),
                            code: "fixture_session_marker".to_owned(),
                            message: error.to_string(),
                        }));
                        continue;
                    }
                }
                if behavior == MockBehavior::DelayedSessions
                    && let Err(message) =
                        wait_for_delayed_session_release(session_marker, &session_id)
                {
                    let _ = provider_sessions.release_session(&session_id);
                    emit(&ProtocolMessage::Error(ErrorMessage {
                        protocol: PROTOCOL_VERSION,
                        id: Some(open.id),
                        code: "fixture_delayed_session".to_owned(),
                        message,
                    }));
                    continue;
                }
                emit(&ProtocolMessage::SessionOpened(SessionOpened {
                    protocol: PROTOCOL_VERSION,
                    id: open.id,
                    session_id,
                }));
            }
            ProtocolMessage::SessionInput(input)
                if behavior.supports_sessions()
                    && provider_sessions.contains(&input.session_id) =>
            {
                if input.data == "fixture.crash-provider" {
                    let _ = provider_sessions.clear();
                    process::exit(37);
                } else if input.data == "\u{5}" {
                    for sequence in 0..16 {
                        emit(&ProtocolMessage::SessionOutput(SessionOutput {
                            protocol: PROTOCOL_VERSION,
                            session_id: input.session_id.clone(),
                            data: format!("burst:{sequence}\n"),
                        }));
                    }
                    let _ = provider_sessions.release_session(&input.session_id);
                    emit(&ProtocolMessage::SessionExit(SessionExit {
                        protocol: PROTOCOL_VERSION,
                        session_id: input.session_id,
                        exit_code: Some(2),
                    }));
                } else if input.data == "fixture.exit-nonzero" {
                    if let Err(error) = provider_sessions.release_session(&input.session_id) {
                        emit(&ProtocolMessage::Error(ErrorMessage {
                            protocol: PROTOCOL_VERSION,
                            id: None,
                            code: "fixture_session_marker".to_owned(),
                            message: error.to_string(),
                        }));
                        continue;
                    }
                    emit(&ProtocolMessage::SessionExit(SessionExit {
                        protocol: PROTOCOL_VERSION,
                        session_id: input.session_id,
                        exit_code: Some(7),
                    }));
                } else {
                    emit(&ProtocolMessage::SessionOutput(SessionOutput {
                        protocol: PROTOCOL_VERSION,
                        session_id: input.session_id,
                        data: format!("echo:{}", input.data),
                    }));
                }
            }
            ProtocolMessage::SessionResize(SessionResize {
                session_id,
                rows,
                columns,
                ..
            }) if behavior.supports_sessions() && provider_sessions.contains(&session_id) => {
                emit(&ProtocolMessage::SessionOutput(SessionOutput {
                    protocol: PROTOCOL_VERSION,
                    session_id,
                    data: format!("resized:{rows}x{columns}"),
                }));
            }
            ProtocolMessage::SessionClose(SessionClose { session_id, .. })
                if behavior.supports_sessions() =>
            {
                match provider_sessions.release_session(&session_id) {
                    Ok(true) => emit(&ProtocolMessage::SessionExit(SessionExit {
                        protocol: PROTOCOL_VERSION,
                        session_id,
                        exit_code: None,
                    })),
                    Ok(false) => {}
                    Err(error) => emit(&ProtocolMessage::Error(ErrorMessage {
                        protocol: PROTOCOL_VERSION,
                        id: None,
                        code: "fixture_session_marker".to_owned(),
                        message: error.to_string(),
                    })),
                }
            }
            ProtocolMessage::Request(request) => {
                if behavior == MockBehavior::CrashOnRequest {
                    process::exit(25);
                }
                if behavior == MockBehavior::OutOfOrder {
                    delayed.push(request);
                    if delayed.len() == 2 {
                        while let Some(request) = delayed.pop() {
                            emit(&ProtocolMessage::Response(Response {
                                protocol: PROTOCOL_VERSION,
                                id: request.id,
                                payload: request.payload,
                            }));
                        }
                    }
                    continue;
                }
                if behavior == MockBehavior::Reference
                    && seen_requests.len() >= REFERENCE_REQUEST_LIMIT
                    && !seen_requests.contains(&request.id)
                {
                    emit(&ProtocolMessage::Error(ErrorMessage {
                        protocol: PROTOCOL_VERSION,
                        id: Some(request.id),
                        code: "fixture_request_limit".to_owned(),
                        message: "reference reached its 1024 unique RPC ID lifetime limit"
                            .to_owned(),
                    }));
                    continue;
                }
                if !seen_requests.insert(request.id.clone()) {
                    emit(&ProtocolMessage::Error(ErrorMessage {
                        protocol: PROTOCOL_VERSION,
                        id: Some(request.id),
                        code: "duplicate_request".to_owned(),
                        message: "duplicate request id".to_owned(),
                    }));
                    continue;
                }
                if behavior == MockBehavior::Reference
                    && let Some(action) = request.action.as_ref()
                {
                    let declared = matches!(
                        action.as_str(),
                        "fixture.action.alpha"
                            | "fixture.destroy.everything"
                            | "fixture.inspect"
                            | "fixture.action.delta"
                    );
                    let error = if !declared {
                        Some(("unsupported_action", "undeclared action"))
                    } else if request.operation.as_str() != "test.echo" {
                        Some((
                            "action_operation_mismatch",
                            "declared action requires test.echo",
                        ))
                    } else {
                        None
                    };
                    if let Some((code, message)) = error {
                        emit(&ProtocolMessage::Error(ErrorMessage {
                            protocol: PROTOCOL_VERSION,
                            id: Some(request.id),
                            code: code.to_owned(),
                            message: message.to_owned(),
                        }));
                        continue;
                    }
                }
                if behavior == MockBehavior::Actions
                    || (behavior == MockBehavior::Reference && request.action.is_some())
                {
                    if let (Some(marker), Some(action)) = (action_marker, request.action.as_ref()) {
                        record_action(marker, action);
                    }
                    match request.action.as_ref().map(ActionId::as_str) {
                        Some("fixture.action.alpha") => {
                            emit(&ProtocolMessage::Response(Response {
                                protocol: PROTOCOL_VERSION,
                                id: request.id,
                                payload: json!({"outcome": "accepted"}),
                            }))
                        }
                        Some("fixture.destroy.everything") => {
                            emit(&ProtocolMessage::Error(ErrorMessage {
                                protocol: PROTOCOL_VERSION,
                                id: Some(request.id),
                                code: "fixture_rejected".to_owned(),
                                message: "adapter-declared rejection".to_owned(),
                            }));
                        }
                        Some("fixture.inspect") => emit(&ProtocolMessage::Response(Response {
                            protocol: PROTOCOL_VERSION,
                            id: request.id,
                            payload: json!({"outcome": "confirmed"}),
                        })),
                        Some("fixture.action.delta") if behavior == MockBehavior::Reference => {
                            if !reference_actions
                                .as_ref()
                                .unwrap()
                                .submit(request.id.clone())
                            {
                                emit(&ProtocolMessage::Error(ErrorMessage {
                                    protocol: PROTOCOL_VERSION,
                                    id: Some(request.id),
                                    code: "fixture_action_busy".to_owned(),
                                    message: "fixture already has a pending delta action"
                                        .to_owned(),
                                }));
                            }
                        }
                        Some("fixture.action.delta") => {
                            thread::sleep(Duration::from_millis(150));
                            emit(&ProtocolMessage::Response(Response {
                                protocol: PROTOCOL_VERSION,
                                id: request.id,
                                payload: json!({"outcome": "delayed"}),
                            }));
                        }
                        _ => emit(&ProtocolMessage::Error(ErrorMessage {
                            protocol: PROTOCOL_VERSION,
                            id: Some(request.id),
                            code: "unsupported_action".to_owned(),
                            message: "undeclared action".to_owned(),
                        })),
                    }
                    continue;
                }
                match request.operation.as_str() {
                    "test.echo" => emit(&ProtocolMessage::Response(Response {
                        protocol: PROTOCOL_VERSION,
                        id: request.id,
                        payload: request.payload,
                    })),
                    "test.stream" => {
                        if behavior == MockBehavior::Reference {
                            let (first, second) = observability_fixture_batches();
                            emit_observations(first);
                            emit_observations(second);
                        }
                        emit(&ProtocolMessage::Event(Event {
                            protocol: PROTOCOL_VERSION,
                            stream: "test".to_owned(),
                            kind: "started".to_owned(),
                            observation: None,
                            payload: json!({"request": request.id.as_str()}),
                        }));
                        emit(&ProtocolMessage::Response(Response {
                            protocol: PROTOCOL_VERSION,
                            id: request.id,
                            payload: json!({"streamed": true}),
                        }));
                    }
                    "test.fail" => emit(&ProtocolMessage::Error(ErrorMessage {
                        protocol: PROTOCOL_VERSION,
                        id: Some(request.id),
                        code: "test_failed".to_owned(),
                        message: "requested failure".to_owned(),
                    })),
                    "test.slow" => thread::sleep(Duration::from_secs(30)),
                    "test.crash" => process::exit(24),
                    _ => emit(&ProtocolMessage::Error(ErrorMessage {
                        protocol: PROTOCOL_VERSION,
                        id: Some(request.id),
                        code: "unsupported_operation".to_owned(),
                        message: "unsupported operation".to_owned(),
                    })),
                }
            }
            ProtocolMessage::Shutdown(_) => {
                // Cancel a held delta before acknowledging; no late action completion.
                drop(reference_observations.take());
                drop(reference_actions.take());
                if behavior == MockBehavior::Reference {
                    let _ = provider_sessions.clear();
                }
                emit(&ProtocolMessage::ShutdownAck(ShutdownAck {
                    protocol: PROTOCOL_VERSION,
                }));
                break;
            }
            _ => {}
        }
    }
    drop(reference_observations);
    drop(reference_actions);
    if behavior == MockBehavior::Reference {
        let _ = provider_sessions.clear();
    }
}

fn emit(message: &ProtocolMessage) {
    // JSON and its terminator must be one atomic frame across all producers.
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, message).unwrap();
    stdout.write_all(b"\n").unwrap();
    stdout.flush().unwrap();
}

/// Own the reference batch producer until it exits. Joining before ShutdownAck
/// allows already-started output to finish but forbids any post-ack event frames.
struct ReferenceObservations {
    stop: mpsc::SyncSender<()>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ReferenceObservations {
    fn new(release: PathBuf, batch: Vec<Observation>) -> Self {
        let (stop, stopped) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            while matches!(
                stopped.recv_timeout(Duration::from_millis(5)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                if release.is_file() {
                    emit_observations(batch);
                    break;
                }
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for ReferenceObservations {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// One worker and one pending slot, regardless of the number of delta requests.
/// The request loop never waits for the release marker. Dropping the worker
/// cancels pending work and wakes its bounded polling wait, including on EOF.
struct ReferenceActions {
    pending: Arc<Mutex<Option<(Response, Instant)>>>,
    stop: mpsc::SyncSender<()>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ReferenceActions {
    fn new(release: Option<PathBuf>) -> Self {
        let pending: Arc<Mutex<Option<(Response, Instant)>>> = Arc::new(Mutex::new(None));
        let work = Arc::clone(&pending);
        let (stop, stopped) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            while matches!(
                stopped.recv_timeout(Duration::from_millis(5)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                let mut pending = work.lock().unwrap();
                let ready = pending.as_ref().is_some_and(|(_, deadline)| {
                    release
                        .as_ref()
                        .map_or_else(|| Instant::now() >= *deadline, |path| path.is_file())
                });
                if ready {
                    let (response, _) = pending.take().unwrap();
                    // Keep the slot locked until the completion is on the wire:
                    // an observer can immediately submit the next delta safely.
                    emit(&ProtocolMessage::Response(response));
                }
            }
        });
        Self {
            pending,
            stop,
            worker: Some(worker),
        }
    }

    fn submit(&self, id: dragonstui_adapter_host::RequestId) -> bool {
        let mut pending = self.pending.lock().unwrap();
        if pending.is_some() {
            return false;
        }
        *pending = Some((
            Response {
                protocol: PROTOCOL_VERSION,
                id,
                payload: json!({"outcome": "delayed"}),
            },
            Instant::now() + Duration::from_millis(150),
        ));
        true
    }
}

impl Drop for ReferenceActions {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn record_action(marker: &std::path::Path, action: &ActionId) {
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(marker)
        .and_then(|mut file| writeln!(file, "{action}"));
}

fn emit_observations(observations: Vec<Observation>) {
    for observation in observations {
        emit(&ProtocolMessage::Event(Event {
            protocol: PROTOCOL_VERSION,
            stream: "observations".to_owned(),
            kind: "fixture".to_owned(),
            observation: Some(observation),
            payload: json!({"sequence": 1}),
        }));
    }
}

fn observability_fixture_batches() -> (Vec<Observation>, Vec<Observation>) {
    let first = vec![
        Observation::Log {
            text: "fixture log startup".to_owned(),
            severity: Some(ObservationSeverity::Info),
            timestamp_millis: Some(1_700_000_000_101),
        },
        Observation::Log {
            text: "fixture log warning".to_owned(),
            severity: Some(ObservationSeverity::Warning),
            timestamp_millis: None,
        },
        Observation::Metric {
            name: "fixture.value".to_owned(),
            value: 10.into(),
            unit: Some("items".to_owned()),
            timestamp_millis: Some(1_700_000_000_110),
        },
        Observation::Metric {
            name: "fixture.value".to_owned(),
            value: 42.into(),
            unit: Some("items".to_owned()),
            timestamp_millis: Some(1_700_000_000_120),
        },
        Observation::Metric {
            name: "fixture.value".to_owned(),
            value: (-5).into(),
            unit: Some("items".to_owned()),
            timestamp_millis: Some(1_700_000_000_130),
        },
        Observation::Status {
            entity: "fixture-api".to_owned(),
            check: "health".to_owned(),
            status: ObservationStatus::Ok,
            timestamp_millis: Some(1_700_000_000_140),
        },
        Observation::Status {
            entity: "fixture-worker".to_owned(),
            check: "queue".to_owned(),
            status: ObservationStatus::Warning,
            timestamp_millis: Some(1_700_000_000_150),
        },
        Observation::Event {
            title: "fixture deployment".to_owned(),
            detail: Some("first timeline detail".to_owned()),
            timestamp_millis: Some(1_700_000_000_160),
        },
    ];
    let second = vec![
        Observation::Event {
            title: "fixture follow-up".to_owned(),
            detail: Some("second timeline detail".to_owned()),
            timestamp_millis: Some(1_700_000_000_170),
        },
        Observation::Event {
            title: "fixture arrival".to_owned(),
            detail: None,
            timestamp_millis: None,
        },
        Observation::Error {
            message: "fixture error".to_owned(),
            signature: Some("fixture.error".to_owned()),
            stack: vec!["frame one".to_owned(), "frame two".to_owned()],
            timestamp_millis: Some(1_700_000_000_180),
        },
        Observation::Error {
            message: "fixture error".to_owned(),
            signature: Some("fixture.error".to_owned()),
            stack: vec!["frame three".to_owned(), "frame four".to_owned()],
            timestamp_millis: Some(1_700_000_000_190),
        },
        Observation::Error {
            message: "fixture distinct error".to_owned(),
            signature: None,
            stack: vec!["distinct frame".to_owned()],
            timestamp_millis: None,
        },
        Observation::Status {
            entity: "fixture-api".to_owned(),
            check: "health".to_owned(),
            status: ObservationStatus::Error,
            timestamp_millis: Some(1_700_000_000_200),
        },
        Observation::Status {
            entity: "fixture-db".to_owned(),
            check: "latency".to_owned(),
            status: ObservationStatus::Unknown,
            timestamp_millis: Some(1_700_000_000_210),
        },
        Observation::Metric {
            name: "fixture.value".to_owned(),
            value: 70.into(),
            unit: Some("items".to_owned()),
            timestamp_millis: Some(1_700_000_000_220),
        },
    ];
    (first, second)
}

fn flush_stdout() {
    io::stdout().flush().unwrap();
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum MockBehavior {
    Normal,
    Reference,
    BadProtocol,
    BadId,
    DuplicateCapabilities,
    EmptyCapabilities,
    SharedCapabilities,
    Events,
    LiveEvents,
    SemanticEvents,
    ObservabilityEvents,
    Actions,
    Sessions,
    DelayedSessions,
    StressEvents,
    OutOfOrder,
    UnknownResponse,
    CrashAfterHandshake,
    CrashOnRequest,
}

impl MockBehavior {
    /// Declares whether this fixture mode exposes a session protocol surface.
    /// Provider-session liveness is tracked separately by `MockSessionRegistry`.
    fn supports_sessions(self) -> bool {
        matches!(
            self,
            Self::Sessions | Self::DelayedSessions | Self::Reference
        )
    }
}

/// Test-only provider session registry. The optional marker is a projection of
/// current provider-owned sessions, not an append-only lifecycle history.
struct MockSessionRegistry<'a> {
    active: BTreeSet<SessionId>,
    marker: Option<&'a Path>,
}

impl<'a> MockSessionRegistry<'a> {
    fn new(marker: Option<&'a Path>) -> Self {
        Self {
            active: BTreeSet::new(),
            marker,
        }
    }

    fn has_sessions(&self) -> bool {
        !self.active.is_empty()
    }

    fn contains(&self, session_id: &SessionId) -> bool {
        self.active.contains(session_id)
    }

    fn record_session(&mut self, session_id: SessionId) -> io::Result<bool> {
        if self.has_sessions() {
            return Ok(false);
        }
        let mut next = self.active.clone();
        next.insert(session_id);
        self.sync_marker(&next)?;
        self.active = next;
        Ok(true)
    }

    fn release_session(&mut self, session_id: &SessionId) -> io::Result<bool> {
        let mut next = self.active.clone();
        let released = next.remove(session_id);
        if released {
            self.sync_marker(&next)?;
            self.active = next;
        }
        Ok(released)
    }

    fn clear(&mut self) -> io::Result<()> {
        if self.has_sessions() {
            let next = BTreeSet::new();
            self.sync_marker(&next)?;
            self.active = next;
        }
        Ok(())
    }

    fn sync_marker(&self, active: &BTreeSet<SessionId>) -> io::Result<()> {
        let Some(marker) = self.marker else {
            return Ok(());
        };
        if let Some(parent) = marker.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut content = String::new();
        for session_id in active {
            content.push_str(session_id.as_str());
            content.push('\n');
        }
        fs::write(marker, content)
    }
}

fn delayed_session_ready_marker(marker: &Path) -> PathBuf {
    PathBuf::from(format!("{}.ready", marker.display()))
}

fn delayed_session_release_marker(marker: &Path) -> PathBuf {
    PathBuf::from(format!("{}.release", marker.display()))
}

fn wait_for_delayed_session_release(
    marker: Option<&Path>,
    session_id: &SessionId,
) -> Result<(), String> {
    let marker = marker.ok_or_else(|| "delayed-sessions requires --session-marker".to_owned())?;
    let ready = delayed_session_ready_marker(marker);
    let release = delayed_session_release_marker(marker);
    if let Some(parent) = ready.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(ready, format!("{session_id}\n")).map_err(|error| error.to_string())?;
    while !release.is_file() {
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

#[allow(dead_code)]
fn _assert_json_value_dependency(_: Value) {}

struct MockOptions {
    mode: String,
    id: String,
    hold_ready: Option<PathBuf>,
    hold_release: Option<PathBuf>,
    launch_marker: Option<PathBuf>,
    action_marker: Option<PathBuf>,
    session_marker: Option<PathBuf>,
    event_release: Option<PathBuf>,
    action_release: Option<PathBuf>,
}
