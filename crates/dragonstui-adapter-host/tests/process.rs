use std::{path::PathBuf, time::Duration};

use dragonstui_adapter_host::{
    AdapterProcess, AdapterProcessConfig, Capability, PROTOCOL_VERSION, ProcessStatus,
    ProtocolMessage, Request, RequestId,
};
use serde_json::json;

fn mock_executable() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap())
}

#[cfg(unix)]
#[test]
fn provider_environment_excludes_reserved_controller_bootstrap_key() {
    let mut process = AdapterProcess::start(
        AdapterProcessConfig::new("/bin/sh")
            .arg("-c")
            .arg("test -z \"${DRAGONSTUI_CONTROLLER_TOKEN+x}\"")
            .env("DRAGONSTUI_CONTROLLER_TOKEN", "fixture-only"),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let ProcessStatus::Exited { success, .. } = process.status() {
            assert!(
                success,
                "reserved bootstrap key reached provider environment"
            );
            break;
        }
        assert!(std::time::Instant::now() < deadline, "fixture did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
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
