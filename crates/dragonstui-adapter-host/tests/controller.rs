use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use dragonstui_adapter_host::{AdapterController, AdapterId, AdapterState, PROTOCOL_VERSION};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let nonce = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-controller-{}-{nonce}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("mock/bin")).unwrap();
        let executable = path.join("mock/bin/mock");
        fs::copy(mock_executable(), &executable).unwrap();
        make_executable(&executable);
        fs::write(
            path.join("mock/adapter.json"),
            format!(
                r#"{{"id":"mock","name":"Mock","version":"1.0.0","protocol_version":{PROTOCOL_VERSION},"executable":"bin/mock"}}"#
            ),
        )
        .unwrap();
        Self { path }
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn mock_executable() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_dragonstui-adapter-host-mock").unwrap())
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}

#[test]
fn controller_owns_a_discovered_adapter_lifecycle_across_commands() {
    let root = TempRoot::new();
    let id = AdapterId::new("mock").unwrap();
    let mut controller = AdapterController::new(&root.path, Duration::from_millis(200), 8);

    assert_eq!(controller.state(&id), None);
    controller.start(&id).unwrap();
    assert_eq!(controller.state(&id), Some(AdapterState::Running));
    assert_eq!(
        controller.diagnostics(&id).unwrap().state,
        AdapterState::Running
    );

    controller.stop(&id).unwrap();
    assert_eq!(controller.state(&id), Some(AdapterState::Stopped));

    controller.restart(&id).unwrap();
    assert_eq!(controller.state(&id), Some(AdapterState::Running));

    controller.unregister(&id).unwrap();
    assert_eq!(controller.state(&id), None);
    assert!(controller.diagnostics(&id).is_none());
}
