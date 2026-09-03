use std::{
    collections::HashSet,
    env,
    io::{self, BufRead, Write},
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
        "duplicate-capabilities" => protocol_mode(MockBehavior::DuplicateCapabilities, &options.id),
        "empty-capabilities" => protocol_mode(MockBehavior::EmptyCapabilities, &options.id),
        "events" => protocol_mode(MockBehavior::Events, &options.id),
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
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => options.mode = args.next().unwrap_or_else(|| "normal".to_owned()),
            "--id" => options.id = args.next().unwrap_or_else(|| "mock".to_owned()),
            _ => {}
        }
    }
    options
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
        MockBehavior::Normal
        | MockBehavior::Events
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
    Events,
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
}
