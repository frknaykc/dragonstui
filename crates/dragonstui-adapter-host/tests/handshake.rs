use std::{path::PathBuf, time::Duration};

use dragonstui_adapter_host::{
    AdapterId, AdapterManifest, AdapterRuntime, AdapterRuntimeConfig, AdapterStartError,
    AdapterState, PROTOCOL_VERSION,
};

fn mock_executable() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap())
}

fn manifest(id: &str) -> AdapterManifest {
    AdapterManifest::from_json(&format!(
        r#"{{
  "id": "{id}",
  "name": "Mock",
  "version": "1.0.0",
  "protocol_version": {PROTOCOL_VERSION},
  "executable": "mock"
}}"#
    ))
    .unwrap()
}

fn config(mode: &str) -> AdapterRuntimeConfig {
    AdapterRuntimeConfig::new(mock_executable())
        .arg("--mode")
        .arg(mode)
        .handshake_timeout(Duration::from_secs(2))
}

#[test]
fn handshake_reaches_running_through_ordered_states_and_records_adapter_info() {
    let runtime = AdapterRuntime::start(manifest("mock"), config("normal")).unwrap();

    assert_eq!(
        runtime.state_history(),
        &[
            AdapterState::Discovered,
            AdapterState::Starting,
            AdapterState::Handshaking,
            AdapterState::Running
        ]
    );
    assert_eq!(runtime.state(), AdapterState::Running);
    assert_eq!(runtime.adapter_id(), &AdapterId::new("mock").unwrap());
    assert_eq!(runtime.capabilities().len(), 4);
}

#[test]
fn handshake_rejects_incompatible_failed_crashed_and_timeout_adapters() {
    let cases = [
        ("bad-protocol", "incompatible"),
        ("bad-id", "incompatible"),
        ("malformed", "failed"),
        ("crash", "crashed"),
        ("timeout", "timeout"),
        ("duplicate-capabilities", "incompatible"),
        ("empty-capabilities", "incompatible"),
    ];

    for (mode, expected) in cases {
        let error = AdapterRuntime::start(manifest("mock"), config(mode)).unwrap_err();
        match expected {
            "incompatible" => assert!(
                matches!(error, AdapterStartError::Incompatible(_)),
                "{mode}"
            ),
            "failed" => assert!(matches!(error, AdapterStartError::Failed(_)), "{mode}"),
            "crashed" => assert!(matches!(error, AdapterStartError::Crashed(_)), "{mode}"),
            "timeout" => assert!(matches!(error, AdapterStartError::Timeout), "{mode}"),
            _ => unreachable!(),
        }
    }
}
