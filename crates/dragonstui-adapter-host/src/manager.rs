use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    fmt,
    path::Path,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ActionId, AdapterAction, AdapterEvent, AdapterId, AdapterRuntime, AdapterRuntimeConfig,
    AdapterStartError, AdapterState, Capability, CapabilityRegistry, DiscoveredAdapter,
    DiscoveryError, LocalAdapterRoot, ProcessStatus, RpcError, RpcOutcome,
};

/// Coordinates discovered adapters without imposing an event loop on a DragonsTUI application.
#[derive(Debug)]
pub struct AdapterManager {
    discovered: BTreeMap<AdapterId, DiscoveredAdapter>,
    runtimes: BTreeMap<AdapterId, AdapterRuntime>,
    states: BTreeMap<AdapterId, ManagedState>,
    capabilities: CapabilityRegistry,
    events: VecDeque<AdapterEvent>,
    disconnects: VecDeque<AdapterDisconnect>,
    event_capacity: usize,
    dropped_events: usize,
    stop_timeout: Duration,
}

impl AdapterManager {
    /// `stop_timeout` bounds graceful shutdown before the process runtime performs its kill fallback.
    /// `event_capacity` bounds the manager-wide drain queue; overflow drops the oldest event.
    pub fn new(stop_timeout: Duration, event_capacity: usize) -> Self {
        Self {
            discovered: BTreeMap::new(),
            runtimes: BTreeMap::new(),
            states: BTreeMap::new(),
            capabilities: CapabilityRegistry::new(),
            events: VecDeque::with_capacity(event_capacity),
            disconnects: VecDeque::with_capacity(event_capacity),
            event_capacity,
            dropped_events: 0,
            stop_timeout,
        }
    }

    /// Discovers local metadata only; this never executes an adapter.
    pub fn discover(
        &mut self,
        root: LocalAdapterRoot,
    ) -> Result<Vec<DiscoveredAdapter>, DiscoveryError> {
        let entries = root.discover()?;
        for entry in &entries {
            if let (Some(manifest), true) = (
                entry.manifest(),
                entry.classification() == crate::AdapterClassification::Valid,
            ) {
                self.discovered.insert(manifest.id.clone(), entry.clone());
                self.states.insert(
                    manifest.id.clone(),
                    ManagedState::new(AdapterState::Discovered),
                );
            }
        }
        Ok(entries)
    }

    pub fn start(&mut self, id: &AdapterId) -> Result<(), ManagerError> {
        let entry = self
            .discovered
            .get(id)
            .ok_or_else(|| ManagerError::UnknownAdapter(id.clone()))?;
        let executable = entry
            .resolved_executable()
            .ok_or_else(|| ManagerError::UnknownAdapter(id.clone()))?;
        self.start_with_config(id, AdapterRuntimeConfig::new(executable))
    }

    pub fn start_with_config(
        &mut self,
        id: &AdapterId,
        config: AdapterRuntimeConfig,
    ) -> Result<(), ManagerError> {
        let entry = self
            .discovered
            .get(id)
            .ok_or_else(|| ManagerError::UnknownAdapter(id.clone()))?;
        let manifest = entry
            .manifest()
            .ok_or_else(|| ManagerError::UnknownAdapter(id.clone()))?
            .clone();
        self.capabilities.remove_provider(id);
        self.runtimes.remove(id);
        match AdapterRuntime::start(manifest, config) {
            Ok(runtime) => {
                self.capabilities
                    .update_provider(id.clone(), runtime.capabilities().to_vec());
                self.states
                    .insert(id.clone(), ManagedState::new(AdapterState::Running));
                self.runtimes.insert(id.clone(), runtime);
                Ok(())
            }
            Err(error) => {
                let state = start_error_state(&error);
                self.states.insert(
                    id.clone(),
                    ManagedState::with_error(state, error.to_string()),
                );
                Err(ManagerError::Start(error))
            }
        }
    }

    pub fn stop(&mut self, id: &AdapterId) -> Result<(), ManagerError> {
        let Some(mut runtime) = self.runtimes.remove(id) else {
            return Err(ManagerError::UnknownAdapter(id.clone()));
        };
        self.states
            .insert(id.clone(), ManagedState::new(AdapterState::Stopping));
        self.capabilities.remove_provider(id);
        runtime
            .stop(self.stop_timeout)
            .map_err(ManagerError::Stop)?;
        self.states
            .insert(id.clone(), ManagedState::new(AdapterState::Stopped));
        Ok(())
    }

    pub fn restart(&mut self, id: &AdapterId) -> Result<(), ManagerError> {
        if self.runtimes.contains_key(id) {
            self.stop(id)?;
        } else {
            self.capabilities.remove_provider(id);
        }
        self.start(id)
    }

    pub fn restart_with_config(
        &mut self,
        id: &AdapterId,
        config: AdapterRuntimeConfig,
    ) -> Result<(), ManagerError> {
        if self.runtimes.contains_key(id) {
            self.stop(id)?;
        } else {
            self.capabilities.remove_provider(id);
        }
        self.start_with_config(id, config)
    }

    /// Stops a live process if necessary, then removes all host-owned state for
    /// one adapter before its installation directory is replaced or deleted.
    pub fn unregister(&mut self, id: &AdapterId) -> Result<(), ManagerError> {
        if !self.discovered.contains_key(id) {
            return Err(ManagerError::UnknownAdapter(id.clone()));
        }
        if self.runtimes.contains_key(id) {
            self.stop(id)?;
        }
        self.capabilities.remove_provider(id);
        self.discovered.remove(id);
        self.states.remove(id);
        Ok(())
    }

    /// Reads whatever is currently available without waiting for a request completion.
    pub fn poll(&mut self, per_adapter_timeout: Duration) {
        let ids: Vec<_> = self.runtimes.keys().cloned().collect();
        for id in ids {
            let was_running = self.state(&id) == Some(AdapterState::Running);
            let mut failure = None;
            let mut events = Vec::new();
            if let Some(runtime) = self.runtimes.get_mut(&id) {
                match runtime.pump(per_adapter_timeout) {
                    Ok(_) => {}
                    Err(error) => failure = Some(error.to_string()),
                }
                if matches!(runtime.process_status(), ProcessStatus::Exited { .. }) {
                    failure.get_or_insert_with(|| "adapter process exited".to_owned());
                }
                if let Some(error) = &failure {
                    runtime.mark_crashed(error.clone());
                }
                while let Some(event) = runtime.pop_event() {
                    events.push(event);
                }
            }
            for event in events {
                self.push_event(event);
            }
            if let Some(error) = failure {
                self.capabilities.remove_provider(&id);
                self.states.insert(
                    id.clone(),
                    ManagedState::with_error(AdapterState::Crashed, error.clone()),
                );
                if was_running {
                    self.push_disconnect(AdapterDisconnect {
                        adapter_id: id,
                        reason: error,
                    });
                }
            }
        }
    }

    pub fn request(
        &mut self,
        id: &AdapterId,
        operation: Capability,
        payload: Value,
        timeout: Duration,
    ) -> Result<crate::RequestId, ManagerError> {
        let runtime = self
            .runtimes
            .get_mut(id)
            .ok_or_else(|| ManagerError::NotRunning(id.clone()))?;
        runtime
            .send_request(operation, payload, timeout)
            .map_err(ManagerError::Rpc)
    }

    /// Returns producer-declared actions from the currently running adapter.
    pub fn actions(&self, id: &AdapterId) -> Result<Vec<AdapterAction>, ManagerError> {
        self.runtimes
            .get(id)
            .map(|runtime| runtime.actions().to_vec())
            .ok_or_else(|| ManagerError::NotRunning(id.clone()))
    }

    /// Resolves an exact producer-declared action identity before writing an RPC request.
    pub fn invoke_action(
        &mut self,
        id: &AdapterId,
        action_id: &ActionId,
        payload: Value,
        timeout: Duration,
    ) -> Result<crate::RequestId, ManagerError> {
        let runtime = self
            .runtimes
            .get_mut(id)
            .ok_or_else(|| ManagerError::NotRunning(id.clone()))?;
        let action = runtime
            .actions()
            .iter()
            .find(|action| action.id == *action_id)
            .cloned()
            .ok_or_else(|| ManagerError::UnknownAction {
                adapter_id: id.clone(),
                action_id: action_id.clone(),
            })?;
        runtime
            .send_action_request(&action, payload, timeout)
            .map_err(ManagerError::Rpc)
    }

    /// Retrieves a completed response/error outcome without blocking the UI thread.
    pub fn take_response(
        &mut self,
        id: &AdapterId,
        request_id: &crate::RequestId,
    ) -> Option<RpcOutcome> {
        self.runtimes.get_mut(id)?.take_outcome(request_id)
    }

    pub fn wait_response(
        &mut self,
        id: &AdapterId,
        request_id: &crate::RequestId,
        timeout: Duration,
    ) -> Result<RpcOutcome, ManagerError> {
        let runtime = self
            .runtimes
            .get_mut(id)
            .ok_or_else(|| ManagerError::NotRunning(id.clone()))?;
        runtime
            .wait_response(request_id, timeout)
            .map_err(ManagerError::Rpc)
    }

    pub fn take_events(&mut self) -> Vec<AdapterEvent> {
        std::mem::take(&mut self.events).into_iter().collect()
    }

    /// Drains generic adapter events and one-shot stream disconnects together.
    /// Both queues use the manager's fixed event capacity.
    pub fn take_live_data(&mut self) -> AdapterLiveData {
        AdapterLiveData {
            events: self.take_events(),
            disconnects: std::mem::take(&mut self.disconnects).into_iter().collect(),
        }
    }

    pub fn providers_for(&self, capability: &Capability) -> Vec<AdapterId> {
        self.capabilities.providers_for(capability)
    }

    pub fn state(&self, id: &AdapterId) -> Option<AdapterState> {
        self.states.get(id).map(|state| state.state)
    }

    pub fn diagnostics(&self, id: &AdapterId) -> Option<AdapterDiagnostics> {
        let state = self.states.get(id)?;
        let discovered = self.discovered.get(id);
        let runtime = self.runtimes.get(id);
        let manifest = runtime
            .map(AdapterRuntime::manifest)
            .or_else(|| discovered.and_then(DiscoveredAdapter::manifest));
        Some(AdapterDiagnostics {
            adapter_id: id.clone(),
            version: runtime
                .map(|runtime| runtime.info().version.clone())
                .or_else(|| manifest.map(|item| item.version.clone())),
            protocol: runtime
                .map(|runtime| runtime.info().protocol)
                .or_else(|| manifest.map(|item| item.protocol_version)),
            state: state.state,
            pid: runtime.map(AdapterRuntime::pid),
            uptime: runtime.map(AdapterRuntime::uptime),
            capabilities: runtime
                .map(|runtime| runtime.capabilities().to_vec())
                .unwrap_or_default(),
            last_error: runtime
                .and_then(AdapterRuntime::last_error)
                .map(ToOwned::to_owned)
                .or_else(|| state.last_error.clone()),
            stderr_tail: runtime.map(AdapterRuntime::stderr_tail).unwrap_or_default(),
            stderr_dropped_line_count: runtime
                .map(AdapterRuntime::stderr_dropped_lines)
                .unwrap_or_default(),
            dropped_event_count: runtime
                .map(AdapterRuntime::dropped_event_count)
                .unwrap_or_default(),
            pending_request_count: runtime
                .map(AdapterRuntime::pending_count)
                .unwrap_or_default(),
            response_queue_capacity: runtime
                .map(AdapterRuntime::response_queue_capacity)
                .unwrap_or_default(),
            response_queue_len: runtime
                .map(AdapterRuntime::response_queue_len)
                .unwrap_or_default(),
            event_queue_capacity: runtime
                .map(AdapterRuntime::event_queue_capacity)
                .unwrap_or_default(),
            event_queue_len: runtime
                .map(AdapterRuntime::event_queue_len)
                .unwrap_or_default(),
        })
    }

    pub fn event_queue_len(&self) -> usize {
        self.events.len()
    }
    pub fn event_queue_capacity(&self) -> usize {
        self.event_capacity
    }
    pub fn dropped_event_count(&self) -> usize {
        self.dropped_events
    }

    fn push_event(&mut self, event: AdapterEvent) {
        if self.event_capacity == 0 {
            self.dropped_events += 1;
            return;
        }
        if self.events.len() == self.event_capacity {
            self.events.pop_front();
            self.dropped_events += 1;
        }
        self.events.push_back(event);
    }

    fn push_disconnect(&mut self, disconnect: AdapterDisconnect) {
        if self.event_capacity == 0 {
            return;
        }
        if self.disconnects.len() == self.event_capacity {
            self.disconnects.pop_front();
        }
        self.disconnects.push_back(disconnect);
    }
}

/// Generic adapter data drained by a controller without capability-specific decoding.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AdapterLiveData {
    pub events: Vec<AdapterEvent>,
    pub disconnects: Vec<AdapterDisconnect>,
}

/// A single transition from a running adapter stream to a terminal failure state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterDisconnect {
    pub adapter_id: AdapterId,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdapterDiagnostics {
    pub adapter_id: AdapterId,
    pub version: Option<String>,
    pub protocol: Option<u32>,
    pub state: AdapterState,
    pub pid: Option<u32>,
    pub uptime: Option<Duration>,
    pub capabilities: Vec<Capability>,
    pub last_error: Option<String>,
    pub stderr_tail: String,
    pub stderr_dropped_line_count: usize,
    pub dropped_event_count: usize,
    pub pending_request_count: usize,
    pub response_queue_capacity: usize,
    pub response_queue_len: usize,
    pub event_queue_capacity: usize,
    pub event_queue_len: usize,
}

#[derive(Clone, Debug)]
struct ManagedState {
    state: AdapterState,
    last_error: Option<String>,
}
impl ManagedState {
    fn new(state: AdapterState) -> Self {
        Self {
            state,
            last_error: None,
        }
    }
    fn with_error(state: AdapterState, last_error: String) -> Self {
        Self {
            state,
            last_error: Some(last_error),
        }
    }
}

#[derive(Debug)]
pub enum ManagerError {
    UnknownAdapter(AdapterId),
    NotRunning(AdapterId),
    UnknownAction {
        adapter_id: AdapterId,
        action_id: ActionId,
    },
    Start(AdapterStartError),
    Stop(crate::ProcessError),
    Rpc(RpcError),
}
impl fmt::Display for ManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAdapter(id) => write!(formatter, "unknown adapter {id}"),
            Self::NotRunning(id) => write!(formatter, "adapter {id} is not running"),
            Self::UnknownAction {
                adapter_id,
                action_id,
            } => write!(
                formatter,
                "unknown action {action_id} for adapter {adapter_id}"
            ),
            Self::Start(error) => error.fmt(formatter),
            Self::Stop(error) => error.fmt(formatter),
            Self::Rpc(error) => error.fmt(formatter),
        }
    }
}
impl Error for ManagerError {}

fn start_error_state(error: &AdapterStartError) -> AdapterState {
    match error {
        AdapterStartError::Incompatible(_) => AdapterState::Incompatible,
        AdapterStartError::Crashed(_) => AdapterState::Crashed,
        AdapterStartError::Failed(_) | AdapterStartError::Timeout => AdapterState::Failed,
    }
}

#[allow(dead_code)]
fn _adapter_directory(_: &Path) {}
