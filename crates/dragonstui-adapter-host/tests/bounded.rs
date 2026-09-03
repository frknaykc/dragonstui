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
