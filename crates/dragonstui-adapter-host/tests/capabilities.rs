use dragonstui_adapter_host::{AdapterId, Capability, CapabilityRegistry};

fn adapter(id: &str) -> AdapterId {
    AdapterId::new(id).unwrap()
}

fn capability(id: &str) -> Capability {
    Capability::new(id).unwrap()
}

#[test]
fn registry_replaces_removes_and_restarts_multi_provider_capabilities_without_stale_entries() {
    let mut registry = CapabilityRegistry::new();
    let echo = capability("test.echo");
    let stream = capability("test.stream");

    registry.update_provider(adapter("mock-a"), vec![echo.clone(), stream.clone()]);
    registry.update_provider(adapter("mock-b"), vec![echo.clone()]);

    assert_eq!(
        registry.providers_for(&echo),
        vec![adapter("mock-a"), adapter("mock-b")]
    );

    registry.update_provider(adapter("mock-a"), vec![stream.clone()]);

    assert_eq!(registry.providers_for(&echo), vec![adapter("mock-b")]);
    assert_eq!(registry.providers_for(&stream), vec![adapter("mock-a")]);

    registry.remove_provider(&adapter("mock-b"));
    assert!(registry.providers_for(&echo).is_empty());

    registry.update_provider(adapter("mock-a"), vec![echo.clone()]);

    assert_eq!(registry.providers_for(&stream), Vec::<AdapterId>::new());
    assert_eq!(registry.providers_for(&echo), vec![adapter("mock-a")]);
}
