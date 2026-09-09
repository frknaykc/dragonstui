//! Opt-in measurements: no timing thresholds, network, or installed providers.
use std::{
    fs,
    hint::black_box,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use dragonstui_adapter_host::{
    AdapterId, AdapterManager, AdapterRuntimeConfig, AdapterState, Capability, LocalAdapterRoot,
    PROTOCOL_VERSION, ProtocolMessage, Request, RequestId, RpcOutcome,
};
use serde_json::json;

struct FixtureRoot(PathBuf);
impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn percentile(samples: &[u64], percent: usize) -> u64 {
    samples[(samples.len() - 1) * percent / 100]
}

#[test]
fn percentile_uses_observed_values() {
    assert_eq!(percentile(&[7], 95), 7);
    assert_eq!(percentile(&[1, 2, 3, 4, 5], 50), 3);
    assert_eq!(percentile(&[1, 2, 3, 4, 5], 100), 5);
}

#[test]
#[ignore = "opt-in release measurements; run with --release --ignored --nocapture"]
// Debug CI must compile this test, but explicitly running it must reject debug timings.
#[allow(clippy::assertions_on_constants)]
fn adapter_host_performance_matrix() {
    assert!(!cfg!(debug_assertions), "measure a release build");
    for bytes in [64, 4096, 65536] {
        let message = ProtocolMessage::Request(Request {
            protocol: PROTOCOL_VERSION,
            id: RequestId::new("measurement-1").unwrap(),
            operation: Capability::new("test.echo").unwrap(),
            action: None,
            payload: json!({"text": "x".repeat(bytes)}),
        });
        let wire = serde_json::to_vec(&message).unwrap();
        assert_eq!(
            serde_json::from_slice::<ProtocolMessage>(&wire).unwrap(),
            message
        );
        let iterations = 1000;
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(serde_json::to_vec(black_box(&message)).unwrap());
        }
        let encode_ns = start.elapsed().as_nanos() / iterations;
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(serde_json::from_slice::<ProtocolMessage>(black_box(&wire)).unwrap());
        }
        println!(
            "M72_RESULT {}",
            json!({
                "case": "serde", "payload_bytes": bytes, "wire_bytes": wire.len(),
                "iterations": iterations, "encode_ns_per_message": encode_ns,
                "decode_ns_per_message": start.elapsed().as_nanos() / iterations
            })
        );
    }
    for count in [1, 4, 8] {
        measure_manager(count);
    }
}

fn measure_manager(count: usize) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = FixtureRoot(std::env::temp_dir().join(format!(
        "dragonstui-m72-{}-{nonce}-{count}",
        std::process::id()
    )));
    fs::create_dir(&root.0).unwrap();
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_dragonstui-adapter-host-mock"));
    let ids: Vec<_> = (0..count)
        .map(|i| AdapterId::new(format!("perf-{i}")).unwrap())
        .collect();
    for id in &ids {
        let directory = root.0.join(id.as_str());
        fs::create_dir(&directory).unwrap();
        fs::copy(&executable, directory.join("mock")).unwrap();
        fs::write(
            directory.join("adapter.json"),
            json!({
                "id": id.as_str(), "name": "Performance fixture", "version": "1.0.0",
                "protocol_version": PROTOCOL_VERSION, "executable": "mock"
            })
            .to_string(),
        )
        .unwrap();
    }
    // Manager drops before the fixture root, including on assertion failure.
    let mut manager = AdapterManager::new(Duration::from_millis(300), 32);
    assert_eq!(
        manager
            .discover(LocalAdapterRoot::new(&root.0))
            .unwrap()
            .len(),
        count
    );
    for id in &ids {
        manager
            .start_with_config(
                id,
                AdapterRuntimeConfig::new(&executable)
                    .arg("--id")
                    .arg(id.as_str())
                    .arg("--mode")
                    .arg("stress-requests")
                    .event_rate_limit(10_000_000)
                    .ingress_queue_capacity(8)
                    .event_queue_capacity(8)
                    .response_queue_capacity(4),
            )
            .unwrap();
    }
    let echo = Capability::new("test.echo").unwrap();
    // Warm-up RPC provides an ordered stdout barrier and excludes startup events.
    for id in &ids {
        let request = manager
            .request(id, echo.clone(), json!(null), Duration::from_secs(10))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            manager.poll(Duration::ZERO);
            manager.take_events();
            if let Some(outcome) = manager.take_response(id, &request) {
                assert_eq!(outcome, RpcOutcome::Response(json!(null)));
                break;
            }
            assert!(Instant::now() < deadline, "warm-up deadline");
        }
    }
    for timeout_ms in [0, 1] {
        let start = Instant::now();
        for _ in 0..100 {
            manager.poll(Duration::from_millis(timeout_ms));
            assert!(manager.take_events().is_empty());
        }
        println!(
            "M72_RESULT {}",
            json!({
                "case": "idle_poll", "adapters": count, "per_adapter_timeout_ms": timeout_ms,
                "iterations": 100, "ns_per_poll": start.elapsed().as_nanos() / 100
            })
        );
    }
    for (operation, bytes) in [("test.echo", 64), ("test.echo", 4096), ("test.stream", 0)] {
        let capability = Capability::new(operation).unwrap();
        let payload = json!({"text": "x".repeat(bytes)});
        let expected = if operation == "test.stream" {
            json!({"streamed": true})
        } else {
            payload.clone()
        };
        let rounds = 100;
        let initial_drops = manager.dropped_event_count();
        let mut events = 0;
        let mut poll_calls = 0;
        let mut poll_ns = 0;
        let mut latencies = Vec::with_capacity(rounds * count);
        let mut per_adapter = vec![0; count];
        let start = Instant::now();
        for _ in 0..rounds {
            let mut pending = Vec::with_capacity(count);
            for (index, id) in ids.iter().enumerate() {
                let sent = Instant::now();
                let request = manager
                    .request(
                        id,
                        capability.clone(),
                        payload.clone(),
                        Duration::from_secs(10),
                    )
                    .unwrap();
                pending.push((index, request, sent));
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while !pending.is_empty() {
                let poll_start = Instant::now();
                manager.poll(Duration::ZERO);
                poll_ns += poll_start.elapsed().as_nanos();
                poll_calls += 1;
                assert!(manager.event_queue_len() <= 32);
                let batch = manager.take_events();
                assert!(batch.iter().all(|event| ids.contains(&event.adapter_id)));
                events += batch.len();
                pending.retain(|(index, request, sent)| {
                    if let Some(outcome) = manager.take_response(&ids[*index], request) {
                        assert_eq!(outcome, RpcOutcome::Response(expected.clone()));
                        per_adapter[*index] += 1;
                        latencies.push(sent.elapsed().as_micros() as u64);
                        false
                    } else {
                        true
                    }
                });
                assert!(Instant::now() < deadline, "measurement RPC deadline");
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        assert!(manager.take_events().is_empty());
        assert_eq!(manager.dropped_event_count(), initial_drops);
        assert_eq!(
            events,
            if operation == "test.stream" {
                count * rounds * 129
            } else {
                0
            }
        );
        assert_eq!(per_adapter, vec![rounds; count]);
        latencies.sort_unstable();
        println!(
            "M72_RESULT {}",
            json!({
                "case": operation, "adapters": count, "payload_bytes": bytes, "rounds": rounds,
                "completed_per_adapter": per_adapter, "rpc_completed": latencies.len(),
                "events": events, "dropped_events": 0, "elapsed_seconds": elapsed,
                "rpc_per_second": latencies.len() as f64 / elapsed,
                "events_per_second": events as f64 / elapsed,
                "rpc_p50_us": percentile(&latencies, 50), "rpc_p95_us": percentile(&latencies, 95),
                "rpc_max_us": latencies.last().unwrap(), "poll_calls": poll_calls, "poll_total_ns": poll_ns
            })
        );
    }
    for id in &ids {
        let diagnostics = manager.diagnostics(id).unwrap();
        assert_eq!(diagnostics.state, AdapterState::Running);
        assert_eq!(diagnostics.pending_request_count, 0);
        assert_eq!(diagnostics.response_queue_len, 0);
        manager.stop(id).unwrap();
        assert_eq!(manager.state(id), Some(AdapterState::Stopped));
        assert!(manager.diagnostics(id).unwrap().pid.is_none());
    }
    println!("M72_CLEANUP {}", json!({"stopped_adapters": count}));
}
