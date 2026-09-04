use std::{path::PathBuf, time::Duration};

use dragonstui_adapter_host::{
    AdapterProcess, AdapterProcessConfig, Capability, PROTOCOL_VERSION, ProcessStatus,
    ProtocolMessage, Request, RequestId,
};
use serde_json::json;

fn mock_executable() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap())
}

#[test]
fn process_uses_piped_protocol_stdout_and_keeps_stderr_as_diagnostics() {
    let config = AdapterProcessConfig::new(mock_executable())
        .arg("--mode")
        .arg("process");
    let mut process = AdapterProcess::start(config).unwrap();

    process
        .write_message(&ProtocolMessage::Request(Request {
            protocol: PROTOCOL_VERSION,
            id: RequestId::new("req-process").unwrap(),
            operation: Capability::new("test.echo").unwrap(),
            action: None,
            payload: json!({"value": 7}),
        }))
        .unwrap();

    let response = process.read_stdout_message(Duration::from_secs(2)).unwrap();

    assert_eq!(
        response,
        ProtocolMessage::Response(dragonstui_adapter_host::Response {
            protocol: PROTOCOL_VERSION,
            id: RequestId::new("req-process").unwrap(),
            payload: json!({"value": 7}),
        })
    );
    assert!(process.stderr_tail().contains("diagnostic line"));
    assert!(!process.stderr_tail().contains("req-process"));
    assert!(matches!(process.status(), ProcessStatus::Running { .. }));

    let status = process
        .stop(Duration::from_millis(300), Duration::from_millis(300))
        .unwrap();
    assert!(status.success());
    assert!(matches!(process.status(), ProcessStatus::Exited { .. }));
}
