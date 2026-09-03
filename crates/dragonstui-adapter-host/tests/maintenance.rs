use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use dragonstui_adapter_host::{
    AdapterId, AdapterInstaller, InstallError, LocalAdapterRoot, Platform, Registry,
};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);
const V1_SHA256: &str = "2b844958fffb66712a08acf27d6d9f81dbf2ec57cc3a5993305b5f380a78c7ed";
const V2_SHA256: &str = "664dcdcc580cf5b4b99da47003feeb2e2371da8c93a21cb0a3e74c873161699b";

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(name: &str) -> Self {
        let nonce = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-maintenance-{name}-{}-{nonce}",
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

fn registry(v1_source: &str, v2_source: &str, v2_checksum: &str) -> Registry {
    Registry::from_json(&format!(
        r#"{{"adapters":[{{"id":"mock","name":"Mock","releases":[{{"version":"0.1.0","protocol_version":1,"artifacts":[{{"os":"macos","architecture":"aarch64","source":"{v1_source}","size":16,"sha256":"{V1_SHA256}","executable":"bin/mock"}}]}},{{"version":"0.2.0","protocol_version":1,"artifacts":[{{"os":"macos","architecture":"aarch64","source":"{v2_source}","size":21,"sha256":"{v2_checksum}","executable":"bin/mock"}}]}}]}}]}}"#
    ))
    .unwrap()
}

#[test]
fn update_replaces_an_older_adapter_but_preserves_it_when_verification_fails() {
    let fixture = TempRoot::new("update");
    let v1 = fixture.path.join("v1");
    let v2 = fixture.path.join("v2");
    fs::write(&v1, "mock executable\n").unwrap();
    fs::write(&v2, "mock v0.2 executable\n").unwrap();
    let source1 = format!("file://{}", v1.display());
    let source2 = format!("file://{}", v2.display());
    let store = fixture.path.join("adapters");
    let installer = AdapterInstaller::new(&store);
    let id = AdapterId::new("mock").unwrap();
    let platform = Platform::new("macos", "aarch64").unwrap();

    installer
        .install(
            &registry(&source1, &source2, V2_SHA256),
            &id,
            Some("0.1.0"),
            &platform,
        )
        .unwrap();
    let failure = installer.update(
        &registry(&source1, &source2, &"0".repeat(64)),
        &id,
        &platform,
    );
    assert!(matches!(
        failure,
        Err(InstallError::ChecksumMismatch { .. })
    ));
    assert_eq!(
        LocalAdapterRoot::new(&store).discover().unwrap()[0]
            .manifest()
            .unwrap()
            .version,
        "0.1.0"
    );

    let receipt = installer
        .update(&registry(&source1, &source2, V2_SHA256), &id, &platform)
        .unwrap();
    assert_eq!(receipt.version, "0.2.0");
    assert_eq!(
        LocalAdapterRoot::new(&store).discover().unwrap()[0]
            .manifest()
            .unwrap()
            .version,
        "0.2.0"
    );
}

#[test]
fn remove_only_deletes_the_direct_adapter_directory() {
    let fixture = TempRoot::new("remove");
    let v1 = fixture.path.join("v1");
    let v2 = fixture.path.join("v2");
    fs::write(&v1, "mock executable\n").unwrap();
    fs::write(&v2, "mock v0.2 executable\n").unwrap();
    let source1 = format!("file://{}", v1.display());
    let source2 = format!("file://{}", v2.display());
    let store = fixture.path.join("adapters");
    let sentinel = fixture.path.join("must-survive");
    fs::write(&sentinel, "outside store").unwrap();
    let installer = AdapterInstaller::new(&store);
    let id = AdapterId::new("mock").unwrap();
    let platform = Platform::new("macos", "aarch64").unwrap();
    installer
        .install(
            &registry(&source1, &source2, V2_SHA256),
            &id,
            Some("0.1.0"),
            &platform,
        )
        .unwrap();

    installer.remove(&id).unwrap();
    assert!(!store.join("mock").exists());
    assert!(sentinel.exists());
    assert!(matches!(
        installer.remove(&id),
        Err(InstallError::NotInstalled(_))
    ));
}
