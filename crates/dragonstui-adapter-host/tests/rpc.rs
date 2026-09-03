use std::{path::PathBuf, time::Duration};

use dragonstui_adapter_host::{
    AdapterManifest, AdapterRuntime, AdapterRuntimeConfig, AdapterState, Capability,
    PROTOCOL_VERSION, RpcError, RpcOutcome,
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

fn start_runtime(mode: &str) -> AdapterRuntime {
    AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable())
            .arg("--mode")
            .arg(mode)
            .handshake_timeout(Duration::from_secs(2)),
    )
    .unwrap()
}

#[test]
fn rpc_generates_unique_ids_correlates_responses_errors_and_timeouts() {
    let mut runtime = start_runtime("normal");
    let echo = Capability::new("test.echo").unwrap();
    let fail = Capability::new("test.fail").unwrap();
    let slow = Capability::new("test.slow").unwrap();

    let echo_id = runtime
        .send_request(echo, json!({"message": "hello"}), Duration::from_secs(2))
        .unwrap();
    let fail_id = runtime
        .send_request(fail, json!({}), Duration::from_secs(2))
        .unwrap();

    assert_ne!(echo_id, fail_id);
    assert_eq!(runtime.pending_count(), 2);
    assert_eq!(
        runtime
            .wait_response(&echo_id, Duration::from_secs(2))
            .unwrap(),
        RpcOutcome::Response(json!({"message": "hello"}))
    );
    assert_eq!(
        runtime
            .wait_response(&fail_id, Duration::from_secs(2))
            .unwrap(),
        RpcOutcome::AdapterError {
            code: "test_failed".to_owned(),
            message: "requested failure".to_owned()
        }
    );

    let slow_id = runtime
        .send_request(slow, json!({}), Duration::from_millis(100))
        .unwrap();
    let error = runtime
        .wait_response(&slow_id, Duration::from_millis(150))
        .unwrap_err();
    assert!(matches!(error, RpcError::Timeout));
    assert_eq!(runtime.pending_count(), 0);
}

#[test]
fn rpc_accepts_out_of_order_responses_records_unknown_responses_and_cleans_pending_on_crash() {
    let mut runtime = start_runtime("out-of-order");
    let echo = Capability::new("test.echo").unwrap();
    let first = runtime
        .send_request(echo.clone(), json!({"n": 1}), Duration::from_secs(2))
        .unwrap();
    let second = runtime
        .send_request(echo, json!({"n": 2}), Duration::from_secs(2))
        .unwrap();

    assert_eq!(
        runtime
            .wait_response(&second, Duration::from_secs(2))
            .unwrap(),
        RpcOutcome::Response(json!({"n": 2}))
    );
    assert_eq!(
        runtime
            .wait_response(&first, Duration::from_secs(2))
            .unwrap(),
        RpcOutcome::Response(json!({"n": 1}))
    );

    let mut runtime = start_runtime("unknown-response");
    runtime.pump(Duration::from_secs(2)).unwrap();
    assert_eq!(runtime.unknown_response_count(), 1);

    let mut runtime = start_runtime("crash-after-handshake");
    let id = runtime
        .send_request(
            Capability::new("test.echo").unwrap(),
            json!({}),
            Duration::from_secs(2),
        )
        .unwrap();
    let error = runtime
        .wait_response(&id, Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(error, RpcError::Crashed));
    assert_eq!(runtime.state(), AdapterState::Crashed);
    assert_eq!(runtime.pending_count(), 0);
}
