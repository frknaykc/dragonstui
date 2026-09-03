use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use dragonstui_adapter_host::{
    AdapterId, AdapterManagement, AdapterManagementAction, AdapterManagementOutcome,
    PROTOCOL_VERSION,
};
use sha2::{Digest, Sha256};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(name: &str) -> Self {
        let nonce = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-management-{name}-{}-{nonce}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
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

#[test]
fn management_actions_install_without_starting_then_manage_and_remove_a_mock_adapter() {
    let root = TempRoot::new("root");
    let fixture = TempRoot::new("fixture");
    let artifact = fixture.path.join("mock");
    fs::copy(
        std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap(),
        &artifact,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&artifact).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&artifact, permissions).unwrap();
    }
    let bytes = fs::read(&artifact).unwrap();
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let (os, architecture) = current_platform();
    let registry = fixture.path.join("registry.json");
    fs::write(
        &registry,
        format!(
            r#"{{"adapters":[{{"id":"mock","name":"Managed Mock","releases":[{{"version":"0.1.0","protocol_version":{PROTOCOL_VERSION},"artifacts":[{{"os":"{os}","architecture":"{architecture}","source":"file://{}","size":{},"sha256":"{digest}","executable":"bin/mock"}}]}},{{"version":"0.2.0","protocol_version":{PROTOCOL_VERSION},"artifacts":[{{"os":"{os}","architecture":"{architecture}","source":"file://{}","size":{},"sha256":"{digest}","executable":"bin/mock"}}]}}]}}]}}"#,
            artifact.display(),
            bytes.len(),
            artifact.display(),
            bytes.len(),
        ),
    )
    .unwrap();
    let id = AdapterId::new("mock").unwrap();
    let mut management = AdapterManagement::new(&root.path);

    let installed = management
        .execute(AdapterManagementAction::Install {
            id: id.clone(),
            registry_source: registry.display().to_string(),
            version: Some("0.1.0".to_owned()),
        })
        .unwrap();
    assert_eq!(
        installed,
        AdapterManagementOutcome::Installed {
            id: id.clone(),
            version: "0.1.0".to_owned()
        }
    );
    assert!(management.diagnostics(&id).is_none());

    management
        .execute(AdapterManagementAction::Start { id: id.clone() })
        .unwrap();
    let before_failed_update = management.diagnostics(&id).unwrap();
    let invalid_registry = fixture.path.join("invalid-registry.json");
    fs::write(
        &invalid_registry,
        fs::read_to_string(&registry)
            .unwrap()
            .replace(&digest, &"0".repeat(64)),
    )
    .unwrap();
    assert!(
        management
            .execute(AdapterManagementAction::Update {
                id: id.clone(),
                registry_source: invalid_registry.display().to_string(),
            })
            .is_err()
    );
    let after_failed_update = management.diagnostics(&id).unwrap();
    assert_eq!(after_failed_update.state, "running");
    assert_eq!(after_failed_update.version, before_failed_update.version);
    assert_eq!(
        after_failed_update.capabilities,
        before_failed_update.capabilities
    );

    management
        .execute(AdapterManagementAction::Stop { id: id.clone() })
        .unwrap();
    let updated = management
        .execute(AdapterManagementAction::Update {
            id: id.clone(),
            registry_source: registry.display().to_string(),
        })
        .unwrap();
    assert_eq!(
        updated,
        AdapterManagementOutcome::Updated {
            id: id.clone(),
            version: "0.2.0".to_owned()
        }
    );
    assert!(management.diagnostics(&id).is_none());

    management
        .execute(AdapterManagementAction::Start { id: id.clone() })
        .unwrap();
    let running = management.diagnostics(&id).unwrap();
    assert_eq!(running.state, "running");
    assert!(
        running
            .capabilities
            .iter()
            .any(|capability| capability == "test.echo")
    );

    management
        .execute(AdapterManagementAction::Stop { id: id.clone() })
        .unwrap();
    assert_eq!(management.diagnostics(&id).unwrap().state, "stopped");
    management
        .execute(AdapterManagementAction::Restart { id: id.clone() })
        .unwrap();
    assert_eq!(management.diagnostics(&id).unwrap().state, "running");

    let removed = management
        .execute(AdapterManagementAction::Remove { id: id.clone() })
        .unwrap();
    assert_eq!(
        removed,
        AdapterManagementOutcome::Removed { id: id.clone() }
    );
    assert!(!root.path.join("mock").exists());
    assert!(management.diagnostics(&id).is_none());
}
