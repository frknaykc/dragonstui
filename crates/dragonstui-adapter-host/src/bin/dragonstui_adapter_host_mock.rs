use std::{
    collections::HashSet,
    env,
    fs::{self, OpenOptions},
    io::{self, BufRead, Write},
    path::PathBuf,
    process, thread,
    time::Duration,
};

use dragonstui_adapter_host::{
    AdapterId, AdapterInfo, Capability, ErrorMessage, Event, PROTOCOL_VERSION, ProtocolMessage,
    Response, ShutdownAck,
};
use serde_json::{Value, json};

fn main() {
    let options = parse_options();
    match options.mode.as_str() {
        "process" => process_mode(),
        "normal" => protocol_mode(MockBehavior::Normal, &options.id),
        "bad-protocol" => protocol_mode(MockBehavior::BadProtocol, &options.id),
        "bad-id" => protocol_mode(MockBehavior::BadId, &options.id),
        "malformed" => {
            println!("{{not json}}");
            flush_stdout();
        }
        "crash" => process::exit(23),
        "timeout" => thread::sleep(Duration::from_secs(30)),
        "hold" => {
            hold_before_handshake(&options);
            protocol_mode(MockBehavior::Normal, &options.id);
        }
        "duplicate-capabilities" => protocol_mode(MockBehavior::DuplicateCapabilities, &options.id),
        "empty-capabilities" => protocol_mode(MockBehavior::EmptyCapabilities, &options.id),
        "shared-capabilities" => protocol_mode(MockBehavior::SharedCapabilities, &options.id),
        "events" => protocol_mode(MockBehavior::Events, &options.id),
        "live-events" => protocol_mode(MockBehavior::LiveEvents, &options.id),
        "stress-events" => protocol_mode(MockBehavior::StressEvents, &options.id),
        "out-of-order" => protocol_mode(MockBehavior::OutOfOrder, &options.id),
        "unknown-response" => protocol_mode(MockBehavior::UnknownResponse, &options.id),
        "crash-after-handshake" => protocol_mode(MockBehavior::CrashAfterHandshake, &options.id),
        "crash-on-request" => protocol_mode(MockBehavior::CrashOnRequest, &options.id),
        _ => protocol_mode(MockBehavior::Normal, &options.id),
    }
}

fn parse_options() -> MockOptions {
    let mut args = env::args().skip(1);
    let mut options = MockOptions {
        mode: "normal".to_owned(),
        id: "mock".to_owned(),
        hold_ready: None,
        hold_release: None,
        launch_marker: None,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => options.mode = args.next().unwrap_or_else(|| "normal".to_owned()),
            "--id" => options.id = args.next().unwrap_or_else(|| "mock".to_owned()),
            "--hold-ready" => options.hold_ready = args.next().map(PathBuf::from),
            "--hold-release" => options.hold_release = args.next().map(PathBuf::from),
            "--launch-marker" => options.launch_marker = args.next().map(PathBuf::from),
            _ => {}
        }
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

fn protocol_mode(behavior: MockBehavior, adapter_id: &str) {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let Some(Ok(line)) = lines.next() else {
        return;
    };
    let Ok(ProtocolMessage::Hello(_)) = serde_json::from_str::<ProtocolMessage>(&line) else {
        return;
    };

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
        MockBehavior::Normal
        | MockBehavior::Events
        | MockBehavior::LiveEvents
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

    let info = AdapterInfo {
        protocol,
        id: AdapterId::new(id).unwrap(),
        version: "1.0.0".to_owned(),
        capabilities: capabilities
            .into_iter()
            .map(|capability| Capability::new(capability).unwrap())
            .collect(),
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
            payload: json!({"sequence": 1}),
        }));
    }
    if behavior == MockBehavior::StressEvents {
        for index in 0..40 {
            emit(&ProtocolMessage::Event(Event {
                protocol: PROTOCOL_VERSION,
                stream: "stress".to_owned(),
                kind: "tick".to_owned(),
                payload: json!({"index": index}),
            }));
        }
    }

    let mut seen_requests = HashSet::new();
    let mut delayed = Vec::new();
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
        match message {
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
                if !seen_requests.insert(request.id.clone()) {
                    emit(&ProtocolMessage::Error(ErrorMessage {
                        protocol: PROTOCOL_VERSION,
                        id: Some(request.id),
                        code: "duplicate_request".to_owned(),
                        message: "duplicate request id".to_owned(),
                    }));
                    continue;
                }
                match request.operation.as_str() {
                    "test.echo" => emit(&ProtocolMessage::Response(Response {
                        protocol: PROTOCOL_VERSION,
                        id: request.id,
                        payload: request.payload,
                    })),
                    "test.stream" => {
                        emit(&ProtocolMessage::Event(Event {
                            protocol: PROTOCOL_VERSION,
                            stream: "test".to_owned(),
                            kind: "started".to_owned(),
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
                emit(&ProtocolMessage::ShutdownAck(ShutdownAck {
                    protocol: PROTOCOL_VERSION,
                }));
                break;
            }
            _ => {}
        }
    }
}

fn emit(message: &ProtocolMessage) {
    serde_json::to_writer(io::stdout(), message).unwrap();
    println!();
    flush_stdout();
}

fn flush_stdout() {
    io::stdout().flush().unwrap();
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum MockBehavior {
    Normal,
    BadProtocol,
    BadId,
    DuplicateCapabilities,
    EmptyCapabilities,
    SharedCapabilities,
    Events,
    LiveEvents,
    StressEvents,
    OutOfOrder,
    UnknownResponse,
    CrashAfterHandshake,
    CrashOnRequest,
}

#[allow(dead_code)]
fn _assert_json_value_dependency(_: Value) {}

struct MockOptions {
    mode: String,
    id: String,
    hold_ready: Option<PathBuf>,
    hold_release: Option<PathBuf>,
    launch_marker: Option<PathBuf>,
}
