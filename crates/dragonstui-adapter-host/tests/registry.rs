use dragonstui_adapter_host::{PROTOCOL_VERSION, Platform, Registry, RegistryError};

const REGISTRY: &str = r#"{
  "adapters": [
    {
      "id": "mock",
      "name": "Test Mock",
      "description": "Deterministic process fixture",
      "homepage": "https://example.invalid/mock",
      "releases": [
        {
          "version": "0.1.0",
          "protocol_version": 1,
          "artifacts": [
            {
              "os": "macos",
              "architecture": "aarch64",
              "source": "file:///fixtures/mock-macos-aarch64",
              "size": 42,
              "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
              "executable": "bin/mock"
            },
            {
              "os": "linux",
              "architecture": "x86_64",
              "source": "https://example.invalid/mock-linux-x86_64",
              "sha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
            }
          ]
        },
        {
          "version": "0.2.0",
          "protocol_version": 1,
          "artifacts": [
            {
              "os": "macos",
              "architecture": "aarch64",
              "source": "file:///fixtures/mock-v2-macos-aarch64",
              "sha256": "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
            }
          ]
        }
      ]
    },
    {
      "id": "other",
      "name": "Other Provider",
      "description": "A second fixture",
      "releases": []
    }
  ]
}"#;

#[test]
fn registry_parses_searches_and_selects_exact_platform_artifacts() {
    let registry = Registry::from_json(REGISTRY).unwrap();
    assert_eq!(registry.adapters().len(), 2);
    assert_eq!(registry.search("PROCESS").len(), 1);
    assert_eq!(registry.search("mock")[0].id.as_str(), "mock");

    let release = registry.adapter("mock").unwrap().release("0.1.0").unwrap();
    assert_eq!(release.protocol_version, PROTOCOL_VERSION);
    let artifact = release
        .artifact_for(&Platform::new("macos", "aarch64").unwrap())
        .unwrap();
    assert_eq!(
        artifact.executable.as_ref().unwrap().as_path().to_str(),
        Some("bin/mock")
    );
    assert_eq!(artifact.expected_size, Some(42));
    assert!(
        release
            .artifact_for(&Platform::new("windows", "x86_64").unwrap())
            .is_none()
    );
}

#[test]
fn registry_rejects_ambiguous_or_unsafe_metadata() {
    for invalid in [
        REGISTRY.replace("\"id\": \"other\"", "\"id\": \"mock\""),
        REGISTRY.replace("\"version\": \"0.2.0\"", "\"version\": \"0.1.0\""),
        REGISTRY
            .replace("\"os\": \"linux\"", "\"os\": \"macos\"")
            .replace(
                "\"architecture\": \"x86_64\"",
                "\"architecture\": \"aarch64\"",
            ),
        REGISTRY.replace(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "missing",
        ),
        REGISTRY.replace(
            "file:///fixtures/mock-macos-aarch64",
            "http://example.invalid/artifact",
        ),
    ] {
        assert!(matches!(
            Registry::from_json(&invalid),
            Err(RegistryError::Parse(_) | RegistryError::Validation(_))
        ));
    }
}
