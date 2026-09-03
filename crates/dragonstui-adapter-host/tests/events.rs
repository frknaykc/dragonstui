use std::{path::PathBuf, time::Duration};

use dragonstui_adapter_host::{
    AdapterManifest, AdapterRuntime, AdapterRuntimeConfig, Capability, PROTOCOL_VERSION, RpcOutcome,
};
use serde_json::json;

fn executable() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock")
        .unwrap()
        .into()
}

fn manifest() -> AdapterManifest {
    AdapterManifest::from_json(&format!(
        r#"{{"id":"mock","name":"Mock","version":"1.0.0","protocol_version":{PROTOCOL_VERSION},"executable":"mock"}}"#
    ))
    .unwrap()
}

fn runtime(mode: &str, event_capacity: usize) -> AdapterRuntime {
    AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(executable())
            .arg("--mode")
            .arg(mode)
            .event_queue_capacity(event_capacity),
    )
    .unwrap()
}

#[test]
fn events_interleave_with_requests_and_keep_adapter_stream_kind_routing() {
    let mut runtime = runtime("normal", 4);
    let stream = runtime
        .send_request(
            Capability::new("test.stream").unwrap(),
            json!({}),
            Duration::from_secs(1),
        )
        .unwrap();
    let echo = runtime
        .send_request(
            Capability::new("test.echo").unwrap(),
            json!({"value": 7}),
            Duration::from_secs(1),
        )
        .unwrap();

    for _ in 0..4 {
        runtime.pump(Duration::from_secs(1)).unwrap();
    }
    assert_eq!(
        runtime.take_outcome(&stream),
        Some(RpcOutcome::Response(json!({"streamed": true})))
    );
    assert_eq!(
        runtime.take_outcome(&echo),
        Some(RpcOutcome::Response(json!({"value": 7})))
    );
    let event = runtime.pop_event().unwrap();
    assert_eq!(event.adapter_id.as_str(), "mock");
    assert_eq!(event.stream, "test");
    assert_eq!(event.kind, "started");
}

#[test]
fn event_bursts_stay_bounded_drop_oldest_and_do_not_drop_response_queue() {
    let mut runtime = runtime("stress-events", 3);
    for _ in 0..40 {
        runtime.pump(Duration::from_secs(1)).unwrap();
    }
    assert_eq!(runtime.event_queue_capacity(), 3);
    assert_eq!(runtime.event_queue_len(), 3);
    assert_eq!(runtime.dropped_event_count(), 37);
    assert_eq!(runtime.pop_event().unwrap().payload, json!({"index": 37}));

    let id = runtime
        .send_request(
            Capability::new("test.echo").unwrap(),
            json!({"still": "responsive"}),
            Duration::from_secs(1),
        )
        .unwrap();
    for _ in 0..2 {
        runtime.pump(Duration::from_secs(1)).unwrap();
    }
    assert_eq!(
        runtime.take_outcome(&id),
        Some(RpcOutcome::Response(json!({"still": "responsive"})))
    );
}
