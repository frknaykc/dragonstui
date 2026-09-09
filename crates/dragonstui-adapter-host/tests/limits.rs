use dragonstui_adapter_host::{AdapterManifest, ExecutablePath, ManifestError};

#[test]
fn manifest_byte_budget_accepts_exact_boundary_and_rejects_next_byte() {
    let source = r#"{"id":"fixture","name":"Fixture","version":"1","protocol_version":1,"executable":"fixture"}"#;
    let mut padded = format!("{source}{}", " ".repeat(65536 - source.len()));
    assert!(AdapterManifest::from_json(&padded).is_ok());
    padded.push(' ');
    assert!(matches!(
        AdapterManifest::from_json(&padded),
        Err(ManifestError::TooLarge)
    ));
}

#[test]
fn executable_path_rejects_nul_and_bounds_encoded_length() {
    assert!(ExecutablePath::new("bin/mock").is_ok());
    assert!(ExecutablePath::new("bin/\0mock").is_err());
    assert!(ExecutablePath::new("a".repeat(4096)).is_ok());
    assert!(ExecutablePath::new("a".repeat(4097)).is_err());
}
