use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use dragonstui_adapter_host::{
    AdapterId, AdapterManager, AdapterRuntimeConfig, AdapterState, Capability, LocalAdapterRoot,
    RpcOutcome,
};
use serde_json::json;

struct FixtureRoot(PathBuf);
impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn bounded_env(name: &str, default: usize, max: usize) -> usize {
    let value = std::env::var(name)
        .map(|value| value.parse::<usize>().expect("integer stress parameter"))
        .unwrap_or(default);
    assert!((1..=max).contains(&value), "{name} outside 1..={max}");
    value
}

#[test]
fn multi_adapter_stream_pressure_preserves_rpc_and_queue_accounting() {
    let count = bounded_env("DRAGONSTUI_STRESS_ADAPTERS", 4, 16);
    let rounds = bounded_env("DRAGONSTUI_STRESS_ROUNDS", 32, 20_000);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = FixtureRoot(
        std::env::temp_dir().join(format!("dragonstui-m69-{}-{nonce}", std::process::id())),
    );
    fs::create_dir(&root.0).unwrap();
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_dragonstui-adapter-host-mock"));
    let ids: Vec<_> = (0..count)
        .map(|i| AdapterId::new(format!("stress-{i}")).unwrap())
        .collect();
    for id in &ids {
        let dir = root.0.join(id.as_str());
        fs::create_dir(&dir).unwrap();
        fs::copy(&executable, dir.join("mock")).unwrap();
        fs::write(
            dir.join("adapter.json"),
            json!({
                "id": id.as_str(), "name": "Stress fixture", "version": "1.0.0",
                "protocol_version": 1, "executable": "mock"
            })
            .to_string(),
        )
        .unwrap();
    }
    // Declared after the root so child shutdown precedes fixture directory removal.
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
                    .ingress_queue_capacity(8)
                    .event_queue_capacity(8)
                    .response_queue_capacity(4),
            )
            .unwrap();
    }
    let pids: Vec<_> = ids
        .iter()
        .map(|id| manager.diagnostics(id).unwrap().pid.unwrap())
        .collect();
    println!(
        "M69_READY {}",
        json!({"host_pid": std::process::id(), "adapter_pids": pids})
    );
    let echo = Capability::new("test.echo").unwrap();
    let stream = Capability::new("test.stream").unwrap();
    // A response is an authoritative stdout barrier before the measured workload.
    for id in &ids {
        let req = manager
            .request(id, echo.clone(), json!(null), Duration::from_secs(10))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            manager.poll(Duration::ZERO);
            manager.take_events();
            if let Some(outcome) = manager.take_response(id, &req) {
                assert_eq!(outcome, RpcOutcome::Response(json!(null)));
                break;
            }
            assert!(Instant::now() < deadline, "startup barrier deadline");
        }
    }
    manager.take_events();
    for held_consumer in [false, true] {
        let initial_drops = manager.dropped_event_count();
        let started = Instant::now();
        let mut delivered = 0;
        let mut peak_queue = 0;
        let mut latency_us = Vec::with_capacity(rounds * count * 2);
        for round in 0..rounds {
            let mut pending = Vec::with_capacity(count * 2);
            for id in &ids {
                for (operation, payload, expected) in [
                    (stream.clone(), json!({}), json!({"streamed": true})),
                    (
                        echo.clone(),
                        json!({"adapter": id.as_str(), "round": round}),
                        json!({"adapter": id.as_str(), "round": round}),
                    ),
                ] {
                    let sent = Instant::now();
                    let req = manager
                        .request(id, operation, payload, Duration::from_secs(10))
                        .unwrap();
                    pending.push((id.clone(), req, expected, sent));
                }
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while !pending.is_empty() {
                manager.poll(Duration::ZERO);
                peak_queue = peak_queue.max(manager.event_queue_len());
                assert!(manager.event_queue_len() <= manager.event_queue_capacity());
                if !held_consumer {
                    let events = manager.take_events();
                    assert!(events.iter().all(|event| ids.contains(&event.adapter_id)));
                    delivered += events.len();
                }
                pending.retain(|(id, request, expected, sent)| {
                    if let Some(outcome) = manager.take_response(id, request) {
                        assert_eq!(outcome, RpcOutcome::Response(expected.clone()));
                        latency_us.push(sent.elapsed().as_micros() as u64);
                        false
                    } else {
                        true
                    }
                });
                assert!(Instant::now() < deadline, "RPC deadline: {pending:?}");
            }
        }
        let remaining = manager.take_events();
        assert!(
            remaining
                .iter()
                .all(|event| ids.contains(&event.adapter_id))
        );
        delivered += remaining.len();
        let dropped = manager.dropped_event_count() - initial_drops;
        let emitted = count * rounds * 129;
        assert_eq!(delivered + dropped, emitted, "exact event conservation");
        if held_consumer {
            assert_eq!(delivered, manager.event_queue_capacity().min(emitted));
            assert!(dropped > 0);
        } else {
            assert_eq!(dropped, 0);
        }
        for id in &ids {
            let diagnostics = manager.diagnostics(id).unwrap();
            assert_eq!(diagnostics.state, AdapterState::Running);
            assert_eq!(diagnostics.pending_request_count, 0);
            assert_eq!(diagnostics.response_queue_len, 0);
            assert_eq!(diagnostics.dropped_event_count, 0);
            assert!(diagnostics.event_queue_len <= diagnostics.event_queue_capacity);
        }
        latency_us.sort_unstable();
        let seconds = started.elapsed().as_secs_f64();
        println!(
            "M69_RESULT {}",
            json!({
                "adapters": count, "rounds": rounds, "held_consumer": held_consumer,
                "emitted_events": emitted, "delivered_events": delivered, "dropped_events": dropped,
                "manager_queue_capacity": 32, "manager_queue_peak": peak_queue,
                "runtime_event_capacity": 8, "ingress_capacity": 8, "response_capacity": 4,
                "rpc_completed": latency_us.len(), "elapsed_seconds": seconds,
                "events_per_second": emitted as f64 / seconds,
                "rpc_p50_us": latency_us[latency_us.len() / 2],
                "rpc_p95_us": latency_us[(latency_us.len() - 1) * 95 / 100],
                "rpc_max_us": latency_us.last().unwrap()
            })
        );
    }
    for id in &ids {
        manager.stop(id).unwrap();
        assert_eq!(manager.state(id), Some(AdapterState::Stopped));
        assert!(manager.diagnostics(id).unwrap().pid.is_none());
    }
    assert!(manager.providers_for(&echo).is_empty());
    println!("M69_CLEANUP {}", json!({"stopped_adapters": count}));
}
