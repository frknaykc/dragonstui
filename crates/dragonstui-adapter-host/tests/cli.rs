use std::{
    fs,
    net::SocketAddr,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use dragonstui_adapter_host::ControllerClient;
use sha2::{Digest, Sha256};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_path(name: &str) -> std::path::PathBuf {
    let nonce = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "dragonstui-adapter-cli-{name}-{}-{nonce}",
        std::process::id()
    ))
}

fn current_platform() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "windows"
    };
    let architecture = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    (os, architecture)
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    fs::metadata(path).unwrap().permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

#[test]
fn first_run_empty_root_is_read_only_and_has_install_guidance() {
    let root = temp_path("first-run-missing");
    assert!(!root.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_dragonstui-adapter"))
        .arg("--root")
        .arg(&root)
        .arg("list")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "ID\tVERSION\tSTATE\tPROTOCOL\n"
    );
    let hint = String::from_utf8_lossy(&output.stderr);
    for expected in [
        "No installed adapters",
        "install <id>",
        "--registry",
        "does not start",
    ] {
        assert!(hint.contains(expected), "missing {expected}");
    }
    assert!(!root.exists());
}

#[test]
fn first_run_non_directory_root_error_has_recovery_hint() {
    let root = temp_path("first-run-file");
    fs::write(&root, "retain this file").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dragonstui-adapter"))
        .arg("--root")
        .arg(&root)
        .arg("list")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let hint = String::from_utf8_lossy(&output.stderr);
    assert!(hint.contains("Check --root points to a readable directory"));
    assert_eq!(fs::read_to_string(&root).unwrap(), "retain this file");
    fs::remove_file(root).unwrap();
}

#[test]
fn cli_help_and_local_registry_search_are_plain_terminal_commands() {
    let registry_path = temp_path("registry.json");
    let (os, architecture) = current_platform();
    fs::write(
        &registry_path,
        format!(
            r#"{{"adapters":[{{"id":"mock","name":"Mock Adapter","description":"Deterministic test adapter","releases":[{{"version":"0.1.0","protocol_version":1,"artifacts":[{{"os":"{os}","architecture":"{architecture}","source":"file:///tmp/mock","sha256":"0000000000000000000000000000000000000000000000000000000000000000","executable":"bin/mock"}}]}}]}}]}}"#
        ),
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_dragonstui-adapter");

    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("adapter"));

    let search = Command::new(binary)
        .args(["search", "mock", "--registry"])
        .arg(&registry_path)
        .output()
        .unwrap();
    assert!(search.status.success());
    let stdout = String::from_utf8_lossy(&search.stdout);
    assert!(stdout.contains("mock"));
    assert!(stdout.contains("Mock Adapter"));
    assert!(!stdout.contains("\x1b[?1049h"));

    let _ = fs::remove_file(registry_path);
}

#[test]
fn cli_installs_lists_and_inspects_a_local_adapter_without_starting_it() {
    let root = temp_path("store");
    let artifact = temp_path("artifact");
    let registry_path = temp_path("install-registry.json");
    let (os, architecture) = current_platform();
    fs::write(&artifact, "mock executable\n").unwrap();
    fs::write(
        &registry_path,
        format!(
            r#"{{"adapters":[{{"id":"mock","name":"Mock Adapter","description":"Deterministic test adapter","releases":[{{"version":"0.1.0","protocol_version":1,"artifacts":[{{"os":"{os}","architecture":"{architecture}","source":"file://{}","size":16,"sha256":"2b844958fffb66712a08acf27d6d9f81dbf2ec57cc3a5993305b5f380a78c7ed","executable":"bin/mock"}}]}}]}}]}}"#,
            artifact.display(),
        ),
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_dragonstui-adapter");

    let install = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .args(["install", "mock", "--registry"])
        .arg(&registry_path)
        .output()
        .unwrap();
    assert!(install.status.success());
    assert!(String::from_utf8_lossy(&install.stdout).contains("Installed mock 0.1.0"));
    assert!(root.join("mock/bin/mock").is_file());
    assert!(is_executable(&root.join("mock/bin/mock")));

    let list = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .arg("list")
        .output()
        .unwrap();
    assert!(list.status.success());
    let list_stdout = String::from_utf8_lossy(&list.stdout);
    assert!(list_stdout.contains("mock"));
    assert!(list_stdout.contains("0.1.0"));
    assert!(list_stdout.contains("stopped"));

    let info = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .args(["info", "mock"])
        .output()
        .unwrap();
    assert!(info.status.success());
    let info_stdout = String::from_utf8_lossy(&info.stdout);
    assert!(info_stdout.contains("ID: mock"));
    assert!(info_stdout.contains("Version: 0.1.0"));
    assert!(info_stdout.contains("State: stopped"));
    assert!(info_stdout.contains("SHA-256: 2b844958"));

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_file(artifact);
    let _ = fs::remove_file(registry_path);
}

#[test]
fn cli_updates_and_removes_an_installed_adapter() {
    let root = temp_path("maintenance-store");
    let v1 = temp_path("v1-artifact");
    let v2 = temp_path("v2-artifact");
    let registry_path = temp_path("maintenance-registry.json");
    let (os, architecture) = current_platform();
    fs::write(&v1, "mock executable\n").unwrap();
    fs::write(&v2, "mock v0.2 executable\n").unwrap();
    fs::write(
        &registry_path,
        format!(
            r#"{{"adapters":[{{"id":"mock","name":"Mock Adapter","releases":[{{"version":"0.1.0","protocol_version":1,"artifacts":[{{"os":"{os}","architecture":"{architecture}","source":"file://{}","size":16,"sha256":"2b844958fffb66712a08acf27d6d9f81dbf2ec57cc3a5993305b5f380a78c7ed","executable":"bin/mock"}}]}},{{"version":"0.2.0","protocol_version":1,"artifacts":[{{"os":"{os}","architecture":"{architecture}","source":"file://{}","size":21,"sha256":"664dcdcc580cf5b4b99da47003feeb2e2371da8c93a21cb0a3e74c873161699b","executable":"bin/mock"}}]}}]}}]}}"#,
            v1.display(),
            v2.display(),
        ),
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_dragonstui-adapter");

    let install = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .args(["install", "mock", "--version", "0.1.0", "--registry"])
        .arg(&registry_path)
        .output()
        .unwrap();
    assert!(install.status.success());

    let update = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .args(["update", "mock", "--registry"])
        .arg(&registry_path)
        .output()
        .unwrap();
    assert!(update.status.success());
    assert!(String::from_utf8_lossy(&update.stdout).contains("Updated mock 0.2.0"));

    let remove = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .args(["remove", "mock", "--yes"])
        .output()
        .unwrap();
    assert!(remove.status.success());
    assert!(String::from_utf8_lossy(&remove.stdout).contains("Removed mock"));
    assert!(!root.join("mock").exists());

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_file(v1);
    let _ = fs::remove_file(v2);
    let _ = fs::remove_file(registry_path);
}

#[test]
fn cli_returns_a_nonzero_exit_code_for_an_unknown_adapter() {
    let root = temp_path("unknown-store");
    let binary = env!("CARGO_BIN_EXE_dragonstui-adapter");

    let output = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .args(["info", "missing"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not installed: missing"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("\x1b[?1049h"));
}

#[test]
fn cli_rejects_nonlocal_endpoint_without_replacing_it() {
    let root = temp_path("endpoint-policy");
    let path = root.join(".controller/endpoint.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let fixture = br#"{"address":"192.0.2.1:1","token":"fixture-only"}"#;
    fs::write(&path, fixture).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dragonstui-adapter"))
        .arg("--root")
        .arg(&root)
        .args(["start", "mock"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must use a loopback address"));
    assert!(fs::read(&path).unwrap() == fixture);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_lifecycle_command_autostarts_an_authenticated_controller_and_leaves_no_daemon() {
    let root = temp_path("controller-store");
    let binary = env!("CARGO_BIN_EXE_dragonstui-adapter");

    let start = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .args(["start", "missing"])
        .output()
        .unwrap();
    assert_eq!(start.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&start.stderr).contains("dragonstui-adapter:"));

    let endpoint_path = root.join(".controller/endpoint.json");
    let endpoint: serde_json::Value =
        serde_json::from_slice(&fs::read(&endpoint_path).unwrap()).unwrap();
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(&endpoint_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let address: SocketAddr = endpoint["address"].as_str().unwrap().parse().unwrap();
    let token = endpoint["token"].as_str().unwrap();
    ControllerClient::new(address, token).shutdown().unwrap();

    for _ in 0..100 {
        if !endpoint_path.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!endpoint_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cli_start_stop_restart_and_live_state_share_the_persistent_controller() {
    let root = temp_path("lifecycle-store");
    let executable = root.join("mock/bin/mock");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::copy(
        std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap(),
        &executable,
    )
    .unwrap();
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
    }
    fs::write(
        root.join("mock/adapter.json"),
        r#"{"id":"mock","name":"Mock","version":"1.0.0","protocol_version":1,"executable":"bin/mock"}"#,
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_dragonstui-adapter");

    let start = run_lifecycle_command(binary, &root, "start");
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    assert!(String::from_utf8_lossy(&start.stdout).contains("Started mock"));

    let list = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .arg("list")
        .output()
        .unwrap();
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stdout).contains("running"));

    let stop = run_lifecycle_command(binary, &root, "stop");
    assert!(
        stop.status.success(),
        "{}",
        String::from_utf8_lossy(&stop.stderr)
    );
    let restart = run_lifecycle_command(binary, &root, "restart");
    assert!(
        restart.status.success(),
        "{}",
        String::from_utf8_lossy(&restart.stderr)
    );

    let info = Command::new(binary)
        .args(["--root"])
        .arg(&root)
        .args(["info", "mock"])
        .output()
        .unwrap();
    assert!(info.status.success());
    assert!(String::from_utf8_lossy(&info.stdout).contains("State: running"));

    // Registry preparation failure must not stop the controller-owned adapter.
    let failed_update = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .args(["update", "mock", "--registry"])
        .arg(root.join("missing-registry.json"))
        .output()
        .unwrap();
    assert!(!failed_update.status.success());
    let after_failure = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .args(["info", "mock"])
        .output()
        .unwrap();
    assert!(after_failure.status.success());
    assert!(String::from_utf8_lossy(&after_failure.stdout).contains("State: running"));

    let registry_path = root.join("replacement-registry.json");
    let (os, architecture) = current_platform();
    let mut registry = serde_json::json!({"adapters": [{
        "id": "mock", "name": "Mock", "releases": [{
            "version": "2.0.0", "protocol_version": 1, "artifacts": [{
                "os": os, "architecture": architecture,
                "source": format!("file://{}", executable.display()),
                "sha256": "0".repeat(64), "executable": "bin/mock"
            }]
        }]
    }]});
    fs::write(&registry_path, serde_json::to_vec(&registry).unwrap()).unwrap();
    let checksum_failure = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .args(["update", "mock", "--registry"])
        .arg(&registry_path)
        .output()
        .unwrap();
    assert!(!checksum_failure.status.success());
    let after_checksum = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .args(["info", "mock"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&after_checksum.stdout).contains("State: running"));
    registry["adapters"][0]["releases"][0]["artifacts"][0]["sha256"] = serde_json::json!(format!(
        "{:x}",
        Sha256::digest(fs::read(&executable).unwrap())
    ));
    fs::write(&registry_path, serde_json::to_vec(&registry).unwrap()).unwrap();

    // A failed maintenance status check must preserve both files and runtime state.
    // Only this test-owned endpoint is changed; the actual credential is never logged.
    let endpoint_path = root.join(".controller/endpoint.json");
    let original = fs::read(&endpoint_path).unwrap();
    let mut endpoint: serde_json::Value = serde_json::from_slice(&original).unwrap();
    endpoint["token"] = serde_json::json!("fixture-invalid-credential");
    fs::write(&endpoint_path, serde_json::to_vec(&endpoint).unwrap()).unwrap();
    let failed_remove = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .args(["remove", "mock", "--yes"])
        .output()
        .unwrap();
    let failed_update = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .args(["update", "mock", "--registry"])
        .arg(&registry_path)
        .output()
        .unwrap();
    let failed_start = run_lifecycle_command(binary, &root, "start");
    assert!(fs::read(&endpoint_path).unwrap() == serde_json::to_vec(&endpoint).unwrap());
    fs::write(&endpoint_path, original).unwrap();
    assert!(!failed_remove.status.success());
    assert!(!failed_update.status.success());
    assert!(!failed_start.status.success());
    assert!(root.join("mock/adapter.json").is_file());
    assert!(!root.join(".staging").exists());

    let update = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .args(["update", "mock", "--registry"])
        .arg(&registry_path)
        .output()
        .unwrap();
    assert!(
        update.status.success(),
        "{}",
        String::from_utf8_lossy(&update.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("mock/adapter.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], "2.0.0");

    shutdown_controller(&root);
    let _ = fs::remove_dir_all(root);
}

fn run_lifecycle_command(
    binary: &str,
    root: &std::path::Path,
    command: &str,
) -> std::process::Output {
    Command::new(binary)
        .args(["--root"])
        .arg(root)
        .args([command, "mock"])
        .output()
        .unwrap()
}

fn shutdown_controller(root: &std::path::Path) {
    let endpoint_path = root.join(".controller/endpoint.json");
    let endpoint: serde_json::Value =
        serde_json::from_slice(&fs::read(&endpoint_path).unwrap()).unwrap();
    let address: SocketAddr = endpoint["address"].as_str().unwrap().parse().unwrap();
    let token = endpoint["token"].as_str().unwrap();
    ControllerClient::new(address, token).shutdown().unwrap();
    for _ in 0..100 {
        if !endpoint_path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("controller daemon did not remove its endpoint");
}
