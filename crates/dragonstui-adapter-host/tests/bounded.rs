use std::{path::PathBuf, time::Duration};

use dragonstui_adapter_host::{
    AdapterManifest, AdapterRuntime, AdapterRuntimeConfig, Capability, PROTOCOL_VERSION, RpcError,
    RpcOutcome,
};
use serde_json::json;

fn mock_executable() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap())
}

fn manifest() -> AdapterManifest {
    AdapterManifest::from_json(&format!(
        r#"{{
  "id": "mock",
  "name": "Mock",
  "version": "1.0.0",
  "protocol_version": {PROTOCOL_VERSION},
  "executable": "mock"
}}"#
    ))
    .unwrap()
}

#[test]
fn bounded_event_queue_drops_oldest_and_reports_capacity_len_and_dropped_count() {
    let mut runtime = AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable())
            .arg("--mode")
            .arg("stress-events")
            .handshake_timeout(Duration::from_secs(2))
            .event_queue_capacity(5),
    )
    .unwrap();

    for _ in 0..40 {
        let _ = runtime.pump(Duration::from_secs(2));
    }

    assert_eq!(runtime.event_queue_capacity(), 5);
    assert_eq!(runtime.event_queue_len(), 5);
    assert_eq!(runtime.dropped_event_count(), 35);
    assert_eq!(runtime.pop_event().unwrap().payload, json!({"index": 35}));
}

#[test]
fn rpc_response_queue_backpressures_instead_of_silently_dropping_responses() {
    let mut runtime = AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable())
            .arg("--mode")
            .arg("normal")
            .handshake_timeout(Duration::from_secs(2))
            .response_queue_capacity(1),
    )
    .unwrap();

    let first = runtime
        .send_request(
            Capability::new("test.echo").unwrap(),
            json!({"n": 1}),
            Duration::from_secs(2),
        )
        .unwrap();
    let second = runtime
        .send_request(
            Capability::new("test.echo").unwrap(),
            json!({"n": 2}),
            Duration::from_secs(2),
        )
        .unwrap();

    runtime.pump(Duration::from_secs(2)).unwrap();
    assert_eq!(runtime.response_queue_len(), 1);
    assert!(matches!(
        runtime.pump(Duration::from_millis(20)).unwrap_err(),
        RpcError::Backpressure
    ));
    assert_eq!(
        runtime
            .wait_response(&first, Duration::from_secs(2))
            .unwrap(),
        RpcOutcome::Response(json!({"n": 1}))
    );
    assert_eq!(
        runtime
            .wait_response(&second, Duration::from_secs(2))
            .unwrap(),
        RpcOutcome::Response(json!({"n": 2}))
    );
}

#[test]
fn nonblocking_process_reader_preserves_the_limit_reason() {
    use dragonstui_adapter_host::{
        AdapterProcess, AdapterProcessConfig, Hello, ProcessError, ProtocolMessage,
    };
    let mut process = AdapterProcess::start(
        AdapterProcessConfig::new(mock_executable())
            .arg("--mode")
            .arg("stress-events")
            .event_rate_limit(0),
    )
    .unwrap();
    process
        .write_message(&ProtocolMessage::Hello(Hello {
            protocol: PROTOCOL_VERSION,
            host_version: "test".to_owned(),
        }))
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match process.try_read_stdout_message() {
            Err(ProcessError::LimitExceeded(reason)) => {
                assert!(reason.contains("event rate limit"));
                break;
            }
            Ok(_) => std::thread::yield_now(),
            Err(other) => panic!("unexpected error: {other}"),
        }
        assert!(std::time::Instant::now() < deadline);
    }
}

#[test]
fn rate_failure_is_not_hidden_by_full_response_storage() {
    let mut runtime = AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable())
            .arg("--mode")
            .arg("stress-requests")
            .event_rate_limit(0)
            .response_queue_capacity(1),
    )
    .unwrap();
    let first = runtime
        .send_request(
            Capability::new("test.echo").unwrap(),
            json!(1),
            Duration::from_secs(2),
        )
        .unwrap();
    runtime.pump(Duration::from_secs(2)).unwrap();
    assert_eq!(runtime.response_queue_len(), 1);
    runtime
        .send_request(
            Capability::new("test.stream").unwrap(),
            json!({}),
            Duration::from_secs(2),
        )
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match runtime.pump(Duration::ZERO) {
            Err(RpcError::Failed(reason)) => {
                assert!(reason.contains("event rate limit"));
                break;
            }
            Err(RpcError::Backpressure) => std::thread::yield_now(),
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert!(std::time::Instant::now() < deadline);
    }
    assert_eq!(
        runtime.wait_response(&first, Duration::ZERO).unwrap(),
        RpcOutcome::Response(json!(1))
    );
}

#[test]
fn zero_stream_budget_stops_only_the_streaming_runtime_and_preserves_diagnosis() {
    let mut limited = AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable())
            .arg("--mode")
            .arg("stress-events")
            .event_rate_limit(0),
    )
    .unwrap();
    let mut peer = AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable()).event_rate_limit(0),
    )
    .unwrap();
    let error = limited.pump(Duration::from_secs(2)).unwrap_err();
    assert!(matches!(error, RpcError::Failed(ref reason) if reason.contains("event rate limit")));
    assert_eq!(
        limited.state(),
        dragonstui_adapter_host::AdapterState::Crashed
    );
    #[cfg(unix)]
    assert!(
        !std::process::Command::new("kill")
            .arg("-0")
            .arg(limited.pid().to_string())
            .output()
            .unwrap()
            .status
            .success()
    );
    let diagnosis = limited.last_error().unwrap().to_owned();
    for _ in 0..3 {
        assert_eq!(limited.pump(Duration::ZERO), Err(RpcError::Crashed));
    }
    assert_eq!(limited.last_error(), Some(diagnosis.as_str()));
    let id = peer
        .send_request(
            Capability::new("test.echo").unwrap(),
            json!(42),
            Duration::MAX,
        )
        .unwrap();
    assert_eq!(
        peer.wait_response(&id, Duration::from_secs(2)).unwrap(),
        RpcOutcome::Response(json!(42))
    );
}
