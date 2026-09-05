use std::{path::PathBuf, thread, time::Duration};

use dragonstui_adapter_host::{
    AdapterManifest, AdapterRuntime, AdapterRuntimeConfig, Capability, Observation,
    PROTOCOL_VERSION, RpcOutcome,
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

#[test]
fn semantic_mock_events_survive_handshake_and_runtime_with_provenance_and_payload() {
    let mut runtime = runtime("semantic-events", 8);

    for _ in 0..8 {
        runtime.pump(Duration::from_secs(1)).unwrap();
    }

    let events = std::iter::from_fn(|| runtime.pop_event()).collect::<Vec<_>>();
    assert_eq!(events.len(), 6);
    assert!(events.iter().all(|event| {
        event.adapter_id.as_str() == "mock"
            && event.stream == "observations"
            && event.kind == "fixture"
            && event.payload == json!({"sequence": 1})
    }));
    assert!(events.iter().any(|event| event.observation.is_none()));
    assert!(
        events
            .iter()
            .any(|event| matches!(event.observation, Some(Observation::Log { .. })))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event.observation, Some(Observation::Metric { .. })))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event.observation, Some(Observation::Status { .. })))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event.observation, Some(Observation::Event { .. })))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event.observation, Some(Observation::Error { .. })))
    );
}

#[test]
fn observability_fixture_release_gate_prevents_batch_coalescing() {
    struct Control(PathBuf);
    impl Drop for Control {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let control = Control(std::env::temp_dir().join(format!(
        "dragonstui-event-gate-{}-{nonce}",
        std::process::id()
    )));
    std::fs::create_dir(&control.0).unwrap();
    let release = control.0.join("release");
    let mut runtime = AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(executable())
            .arg("--mode")
            .arg("observability-events")
            .arg("--event-release")
            .arg(release.to_string_lossy().into_owned())
            .event_queue_capacity(8),
    )
    .unwrap();
    for _ in 0..8 {
        assert!(runtime.pump(Duration::from_secs(1)).unwrap());
    }
    // This exceeds the old fixed batch delay, without allowing the second batch.
    assert!(!runtime.pump(Duration::from_millis(1_400)).unwrap());
    assert_eq!(std::iter::from_fn(|| runtime.pop_event()).count(), 8);
    assert_eq!(runtime.dropped_event_count(), 0);
    std::fs::write(release, "release\n").unwrap();
    for _ in 0..8 {
        assert!(runtime.pump(Duration::from_secs(1)).unwrap());
    }
    assert_eq!(std::iter::from_fn(|| runtime.pop_event()).count(), 8);
    assert_eq!(runtime.dropped_event_count(), 0);
    runtime.stop(Duration::from_millis(200)).unwrap();
}

#[test]
fn observability_fixture_emits_two_bounded_batches_with_all_surface_samples() {
    let mut runtime = runtime("observability-events", 16);

    thread::sleep(Duration::from_millis(1_300));
    for _ in 0..16 {
        runtime.pump(Duration::from_millis(100)).unwrap();
    }

    let events = std::iter::from_fn(|| runtime.pop_event()).collect::<Vec<_>>();
    assert_eq!(events.len(), 16);
    assert!(events.iter().all(|event| {
        event.adapter_id.as_str() == "mock"
            && event.stream == "observations"
            && event.kind == "fixture"
            && event.payload == json!({"sequence": 1})
    }));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.observation, Some(Observation::Log { .. })))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.observation, Some(Observation::Metric { .. })))
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.observation, Some(Observation::Status { .. })))
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.observation, Some(Observation::Event { .. })))
            .count(),
        3
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.observation, Some(Observation::Error { .. })))
            .count(),
        3
    );
}
