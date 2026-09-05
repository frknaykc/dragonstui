use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

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

struct FixtureDirectory {
    path: PathBuf,
}

impl FixtureDirectory {
    fn new(label: &str) -> Self {
        static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-{label}-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn wait_for_file(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.is_file() {
        assert!(Instant::now() < deadline, "timed out waiting for {path:?}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn active_fixture_sessions(marker: &Path) -> Vec<String> {
    fs::read_to_string(marker)
        .unwrap_or_default()
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

fn start_delayed_session_runtime(control: &Path) -> AdapterRuntime {
    AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable())
            .arg("--mode")
            .arg("delayed-sessions")
            .arg("--session-marker")
            .arg(
                control
                    .join("active-sessions")
                    .to_string_lossy()
                    .into_owned(),
            )
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

    let mut runtime = start_runtime("crash-on-request");
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

#[test]
fn session_admission_bounds_pending_opens_before_reading_provider_output() {
    let mut runtime = start_runtime("sessions");
    let capability = Capability::new("fixture.terminal").unwrap();
    for _ in 0..64 {
        runtime
            .open_session(capability.clone(), 24, 80, Duration::from_secs(2))
            .unwrap();
    }
    assert_eq!(
        runtime.open_session(capability, 24, 80, Duration::from_secs(2)),
        Err(RpcError::Backpressure),
        "pending opens must reserve bounded terminal retention before admission"
    );
    assert_eq!(runtime.pending_count(), 64);
}

#[test]
fn session_capacity_is_reusable_only_after_public_terminal_drain() {
    let mut runtime = AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable())
            .arg("--mode")
            .arg("sessions")
            .session_capacity(1),
    )
    .unwrap();
    let capability = Capability::new("fixture.terminal").unwrap();
    for cycle in 0..4 {
        let request = runtime
            .open_session(capability.clone(), 24, 80, Duration::from_secs(2))
            .unwrap();
        let session = runtime
            .wait_session_open(&request, Duration::from_secs(2))
            .unwrap();
        assert_eq!(
            runtime.open_session(capability.clone(), 24, 80, Duration::from_secs(2)),
            Err(RpcError::Backpressure),
            "active sessions must retain their terminal reservation"
        );
        let exit_code = if cycle % 2 == 0 {
            runtime
                .send_session_input(&session, "fixture.exit-nonzero".to_owned())
                .unwrap();
            Some(7)
        } else {
            runtime.close_session(&session).unwrap();
            None
        };
        // This fixture emits only the exit for either operation. Read it without
        // draining, then verify that pumping did not free the admission slot.
        assert!(runtime.pump(Duration::from_secs(2)).unwrap());
        assert_eq!(
            runtime.open_session(capability.clone(), 24, 80, Duration::from_secs(2)),
            Err(RpcError::Backpressure),
            "an undrained exit must retain capacity"
        );
        if cycle % 2 == 0 {
            let exit = runtime.pop_session_exit().unwrap();
            assert_eq!(exit.protocol, PROTOCOL_VERSION);
            assert_eq!(exit.session_id, session);
            assert_eq!(exit.exit_code, exit_code);
        } else {
            assert_eq!(runtime.take_session_exit(&session), Some(exit_code));
        }
        assert_eq!(runtime.take_session_exit(&session), None);
        assert!(runtime.pop_session_exit().is_none());
    }
}

#[test]
fn zero_session_capacity_preserves_generic_rpc() {
    let mut runtime = AdapterRuntime::start(
        manifest(),
        AdapterRuntimeConfig::new(mock_executable())
            .arg("--mode")
            .arg("normal")
            .session_capacity(0),
    )
    .unwrap();
    assert_eq!(
        runtime.open_session(
            Capability::new("fixture.terminal").unwrap(),
            24,
            80,
            Duration::from_secs(2),
        ),
        Err(RpcError::Backpressure)
    );
    assert_eq!(runtime.pending_count(), 0);
    let request = runtime
        .send_request(
            Capability::new("test.echo").unwrap(),
            json!({"still": "available"}),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(
        runtime.wait_response(&request, Duration::from_secs(2)),
        Ok(RpcOutcome::Response(json!({"still": "available"})))
    );
}

#[test]
fn stale_delayed_session_open_closes_the_provider_owned_session() {
    let control = FixtureDirectory::new("stale-session-open");
    let marker = control.path.join("active-sessions");
    let ready = control.path.join("active-sessions.ready");
    let release = control.path.join("active-sessions.release");
    let mut runtime = start_delayed_session_runtime(&control.path);
    let request = runtime
        .open_session(
            Capability::new("fixture.terminal").unwrap(),
            24,
            80,
            Duration::from_millis(50),
        )
        .unwrap();

    wait_for_file(&ready, Duration::from_secs(1));
    assert_eq!(
        active_fixture_sessions(&marker),
        vec!["fixture-session".to_owned()]
    );
    assert!(matches!(
        runtime.wait_session_open(&request, Duration::from_secs(1)),
        Err(RpcError::Timeout)
    ));

    fs::write(&release, "release\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !active_fixture_sessions(&marker).is_empty() {
        assert!(
            Instant::now() < deadline,
            "stale provider session was not closed"
        );
        runtime.pump(Duration::from_millis(20)).unwrap();
    }

    runtime.stop(Duration::from_millis(200)).unwrap();
}
