use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use dragonstui_adapter_host::{
    AdapterId, AdapterInstaller, InstallError, LocalAdapterRoot, Platform, Registry,
};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);
const CHECKSUM: &str = "2b844958fffb66712a08acf27d6d9f81dbf2ec57cc3a5993305b5f380a78c7ed";

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(name: &str) -> Self {
        let nonce = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-installer-{name}-{}-{nonce}",
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

fn registry(source: &str, checksum: &str) -> Registry {
    Registry::from_json(&format!(r#"{{"adapters":[{{"id":"mock","name":"Test mock","releases":[{{"version":"0.1.0","protocol_version":1,"artifacts":[{{"os":"macos","architecture":"aarch64","source":"{source}","size":16,"sha256":"{checksum}","executable":"bin/mock"}}]}}]}}]}}"#)).unwrap()
}

#[test]
fn installer_stages_verifies_and_atomically_installs_without_starting() {
    let fixture = TempRoot::new("valid");
    let artifact = fixture.path.join("mock-artifact");
    fs::write(&artifact, "mock executable\n").unwrap();
    let source = format!("file://{}", artifact.display());
    let store = fixture.path.join("adapters");
    let installer = AdapterInstaller::new(&store)
        .registry_source("file:///fixture/registry.json")
        .max_artifact_bytes(64);
    let id = AdapterId::new("mock").unwrap();
    let receipt = installer
        .install(
            &registry(&source, CHECKSUM),
            &id,
            None,
            &Platform::new("macos", "aarch64").unwrap(),
        )
        .unwrap();

    assert_eq!(receipt.version, "0.1.0");
    assert!(receipt.adapter_dir.starts_with(&store));
    assert!(receipt.adapter_dir.join("adapter.json").is_file());
    assert!(receipt.adapter_dir.join("bin/mock").is_file());
    let metadata = installer.install_metadata(&id).unwrap();
    assert_eq!(metadata.adapter_id, "mock");
    assert_eq!(metadata.version, "0.1.0");
    assert_eq!(
        metadata.registry_source.as_deref(),
        Some("file:///fixture/registry.json")
    );
    assert_eq!(metadata.artifact_source, source);
    assert_eq!(metadata.sha256, CHECKSUM);
    assert_eq!(metadata.platform_os, "macos");
    assert_eq!(metadata.platform_architecture, "aarch64");
    assert!(
        LocalAdapterRoot::new(&store).discover().unwrap()[0]
            .resolved_executable()
            .is_some()
    );
    assert!(!store.join(".staging").exists());
    assert!(matches!(
        installer.install(
            &registry(&source, CHECKSUM),
            &id,
            None,
            &Platform::new("macos", "aarch64").unwrap()
        ),
        Err(InstallError::AlreadyInstalled(_))
    ));
}

#[test]
fn installer_rejects_invalid_integrity_or_size_before_any_install() {
    let fixture = TempRoot::new("reject");
    let artifact = fixture.path.join("mock-artifact");
    fs::write(&artifact, "mock executable\n").unwrap();
    let source = format!("file://{}", artifact.display());
    let store = fixture.path.join("adapters");
    let id = AdapterId::new("mock").unwrap();

    let checksum_failure = AdapterInstaller::new(&store).install(
        &registry(&source, &"0".repeat(64)),
        &id,
        None,
        &Platform::new("macos", "aarch64").unwrap(),
    );
    assert!(matches!(
        checksum_failure,
        Err(InstallError::ChecksumMismatch { .. })
    ));
    assert!(!store.join("mock").exists());

    let oversize = AdapterInstaller::new(&store).max_artifact_bytes(8).install(
        &registry(&source, CHECKSUM),
        &id,
        None,
        &Platform::new("macos", "aarch64").unwrap(),
    );
    assert!(matches!(
        oversize,
        Err(InstallError::ArtifactTooLarge { .. })
    ));
    assert!(!store.join("mock").exists());
}
