use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use dragonstui_adapter_host::{AdapterClassification, LocalAdapterRoot, PROTOCOL_VERSION};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(name: &str) -> Self {
        let nonce = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "dragonstui-adapter-host-{name}-{}-{nonce}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn adapter(&self, dir: &str, id: &str, protocol: u32, executable: &str) {
        let adapter_dir = self.path.join(dir);
        fs::create_dir_all(adapter_dir.join("bin")).unwrap();
        write_executable(&adapter_dir.join("bin/mock-adapter"));
        fs::write(
            adapter_dir.join("adapter.json"),
            format!(
                r#"{{
  "id": "{id}",
  "name": "Mock {id}",
  "version": "1.2.3",
  "protocol_version": {protocol},
  "executable": "{executable}"
}}"#
            ),
        )
        .unwrap();
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_executable(path: &Path) {
    fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}

#[test]
fn discovery_returns_no_adapters_for_empty_local_root() {
    let temp = TempRoot::new("empty");

    let discovered = LocalAdapterRoot::new(temp.path()).discover().unwrap();

    assert!(discovered.is_empty());
}

#[test]
fn discovery_ignores_host_private_directories() {
    let temp = TempRoot::new("private-directories");
    temp.adapter("mock", "mock", PROTOCOL_VERSION, "bin/mock-adapter");
    fs::create_dir_all(temp.path().join(".controller")).unwrap();
    fs::create_dir_all(temp.path().join(".staging")).unwrap();

    let discovered = LocalAdapterRoot::new(temp.path()).discover().unwrap();

    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].manifest().unwrap().id.as_str(), "mock");
}

#[test]
fn discovery_classifies_valid_multiple_malformed_missing_duplicate_unsafe_and_unsupported() {
    let temp = TempRoot::new("classifications");
    temp.adapter("alpha", "alpha", PROTOCOL_VERSION, "bin/mock-adapter");
    temp.adapter("beta", "beta", PROTOCOL_VERSION, "bin/mock-adapter");
    fs::create_dir_all(temp.path().join("malformed")).unwrap();
    fs::write(temp.path().join("malformed/adapter.json"), "{not json}").unwrap();
    temp.adapter("missing", "missing", PROTOCOL_VERSION, "bin/not-present");
    temp.adapter("dup-a", "dupe", PROTOCOL_VERSION, "bin/mock-adapter");
    temp.adapter("dup-b", "dupe", PROTOCOL_VERSION, "bin/mock-adapter");
    temp.adapter("unsafe", "unsafe", PROTOCOL_VERSION, "../outside-adapter");
    temp.adapter("future", "future", PROTOCOL_VERSION + 1, "bin/mock-adapter");

    let discovered = LocalAdapterRoot::new(temp.path()).discover().unwrap();
    let classifications: Vec<_> = discovered
        .iter()
        .map(|entry| {
            (
                entry
                    .adapter_dir()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                entry.classification(),
            )
        })
        .collect();

    assert_eq!(
        classifications,
        vec![
            ("alpha".to_owned(), AdapterClassification::Valid),
            ("beta".to_owned(), AdapterClassification::Valid),
            ("dup-a".to_owned(), AdapterClassification::InvalidManifest),
            ("dup-b".to_owned(), AdapterClassification::InvalidManifest),
            (
                "future".to_owned(),
                AdapterClassification::UnsupportedProtocol
            ),
            (
                "malformed".to_owned(),
                AdapterClassification::InvalidManifest
            ),
            (
                "missing".to_owned(),
                AdapterClassification::MissingExecutable
            ),
            ("unsafe".to_owned(), AdapterClassification::InvalidManifest),
        ]
    );

    let valid = discovered
        .iter()
        .find(|entry| entry.adapter_dir().ends_with("alpha"))
        .unwrap();
    assert!(
        valid
            .resolved_executable()
            .unwrap()
            .starts_with(fs::canonicalize(temp.path()).unwrap())
    );
}
