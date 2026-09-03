use std::path::Path;

use dragonstui_adapter_host::{AdapterId, AdapterManifest, ManifestError, PROTOCOL_VERSION};

const VALID_MANIFEST: &str = r#"{
  "id": "mock",
  "name": "Test-only mock",
  "version": "0.1.0",
  "protocol_version": 1,
  "executable": "bin/mock-adapter",
  "description": "development fixture"
}"#;

#[test]
fn manifest_parses_validated_identity_and_relative_executable() {
    let manifest = AdapterManifest::from_json(VALID_MANIFEST).unwrap();

    assert_eq!(manifest.id, AdapterId::new("mock").unwrap());
    assert_eq!(manifest.protocol_version, PROTOCOL_VERSION);
    assert_eq!(manifest.executable.as_path(), Path::new("bin/mock-adapter"));
    assert_eq!(manifest.description.as_deref(), Some("development fixture"));
}

#[test]
fn manifest_rejects_unsafe_or_malformed_identity_and_executable_paths() {
    for executable in ["../escape", "/tmp/adapter", "bin/../adapter", ""] {
        let source = VALID_MANIFEST.replace("bin/mock-adapter", executable);
        assert!(matches!(
            AdapterManifest::from_json(&source),
            Err(ManifestError::InvalidExecutablePath(_))
        ));
    }

    let bad_id = VALID_MANIFEST.replace("\"mock\"", "\"../mock\"");
    assert!(matches!(
        AdapterManifest::from_json(&bad_id),
        Err(ManifestError::Parse(_))
    ));
    assert!(matches!(
        AdapterManifest::from_json("{not json}"),
        Err(ManifestError::Parse(_))
    ));
}
