use std::collections::{BTreeMap, BTreeSet};

use crate::{AdapterId, Capability};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityRegistry {
    by_provider: BTreeMap<AdapterId, BTreeSet<Capability>>,
    by_capability: BTreeMap<Capability, BTreeSet<AdapterId>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_provider(&mut self, adapter_id: AdapterId, capabilities: Vec<Capability>) {
        self.remove_provider(&adapter_id);

        let capabilities: BTreeSet<_> = capabilities.into_iter().collect();
        for capability in &capabilities {
            self.by_capability
                .entry(capability.clone())
                .or_default()
                .insert(adapter_id.clone());
        }
        self.by_provider.insert(adapter_id, capabilities);
    }

    pub fn remove_provider(&mut self, adapter_id: &AdapterId) {
        let Some(capabilities) = self.by_provider.remove(adapter_id) else {
            return;
        };
        for capability in capabilities {
            let remove_capability = if let Some(providers) = self.by_capability.get_mut(&capability)
            {
                providers.remove(adapter_id);
                providers.is_empty()
            } else {
                false
            };
            if remove_capability {
                self.by_capability.remove(&capability);
            }
        }
    }

    pub fn providers_for(&self, capability: &Capability) -> Vec<AdapterId> {
        self.by_capability
            .get(capability)
            .map(|providers| providers.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn capabilities_for(&self, adapter_id: &AdapterId) -> Vec<Capability> {
        self.by_provider
            .get(adapter_id)
            .map(|capabilities| capabilities.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn provider_count(&self) -> usize {
        self.by_provider.len()
    }
}
