use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
    path::Path,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ActionId, AdapterAction, AdapterEvent, AdapterId, AdapterOperation, AdapterRuntime,
    AdapterRuntimeConfig, AdapterSession, AdapterStartError, AdapterState, Capability,
    CapabilityRegistry, DiscoveredAdapter, DiscoveryError, LocalAdapterRoot, OperationId,
    OperationState, ProcessStatus, RequestId, RpcError, RpcOutcome, SessionId,
};

pub const OPERATION_HISTORY_CAPACITY: usize = 16;
pub const ACTIVE_SESSION_CAPACITY: usize = 8;
pub const SESSION_EVENT_QUEUE_CAPACITY: usize = 64;

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
    operations: BTreeMap<OperationId, OperationEntry>,
    operation_order: VecDeque<OperationId>,
    active_sessions: BTreeMap<SessionId, AdapterId>,
    closing_sessions: BTreeSet<SessionId>,
    session_events: VecDeque<AdapterSessionEvent>,
    next_operation: u64,
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
            operations: BTreeMap::new(),
            operation_order: VecDeque::with_capacity(OPERATION_HISTORY_CAPACITY),
            active_sessions: BTreeMap::new(),
            closing_sessions: BTreeSet::new(),
            session_events: VecDeque::with_capacity(SESSION_EVENT_QUEUE_CAPACITY),
            next_operation: 1,
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
        self.fail_active_operations(
            id,
            "adapter_restarted",
            format!("adapter {id} restarted before the operation completed"),
        );
        self.remove_sessions_for_adapter(id);
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
        self.fail_active_operations(
            id,
            "adapter_stopped",
            format!("adapter {id} stopped before the operation completed"),
        );
        self.remove_sessions_for_adapter(id);
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
        self.dispatch_pending_operations();
        let ids: Vec<_> = self.runtimes.keys().cloned().collect();
        for id in ids {
            let was_running = self.state(&id) == Some(AdapterState::Running);
            let mut failure = None;
            let mut events = Vec::new();
            let mut session_events = Vec::new();
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
                while let Some(event) = runtime.pop_session_event() {
                    session_events.push(event);
                }
            }
            for event in events {
                self.push_event(event);
            }
            for event in session_events {
                match event {
                    crate::runtime::RuntimeSessionEvent::Output { session_id, data }
                        if self.active_sessions.get(&session_id) == Some(&id) =>
                    {
                        self.push_session_event(AdapterSessionEvent::Output {
                            adapter_id: id.clone(),
                            session_id,
                            data,
                        });
                    }
                    crate::runtime::RuntimeSessionEvent::Exited {
                        session_id,
                        exit_code,
                    } if self.active_sessions.remove(&session_id) == Some(id.clone()) => {
                        self.closing_sessions.remove(&session_id);
                        self.push_session_event(AdapterSessionEvent::Exited {
                            adapter_id: id.clone(),
                            session_id,
                            exit_code,
                        });
                    }
                    _ => {}
                }
            }
            if let Some(error) = failure {
                self.capabilities.remove_provider(&id);
                self.states.insert(
                    id.clone(),
                    ManagedState::with_error(AdapterState::Crashed, error.clone()),
                );
                if was_running {
                    self.push_disconnect(AdapterDisconnect {
                        adapter_id: id.clone(),
                        reason: error.clone(),
                    });
                }
                self.fail_active_operations(
                    &id,
                    "adapter_crashed",
                    "adapter process crashed before the operation completed".to_owned(),
                );
                self.disconnect_sessions_for_adapter(&id, error);
            }
        }
        self.reconcile_operations();
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

    /// Returns only the interactive session surfaces declared by this provider.
    pub fn sessions(&self, id: &AdapterId) -> Result<Vec<AdapterSession>, ManagerError> {
        self.runtimes
            .get(id)
            .map(|runtime| runtime.sessions().to_vec())
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

    /// Opens an explicitly provider-declared session through the running
    /// adapter runtime. The manager remains the sole routing authority.
    pub fn open_session(
        &mut self,
        id: &AdapterId,
        capability: &Capability,
        rows: u16,
        columns: u16,
        timeout: Duration,
    ) -> Result<crate::SessionId, ManagerError> {
        if self.active_sessions.len() == ACTIVE_SESSION_CAPACITY {
            return Err(ManagerError::SessionCapacity);
        }
        let session_id = {
            let runtime = self
                .runtimes
                .get_mut(id)
                .ok_or_else(|| ManagerError::NotRunning(id.clone()))?;
            if !runtime
                .sessions()
                .iter()
                .any(|session| session.capability == *capability)
            {
                return Err(ManagerError::UnknownSessionCapability {
                    adapter_id: id.clone(),
                    capability: capability.clone(),
                });
            }
            let request_id = runtime
                .open_session(capability.clone(), rows, columns, timeout)
                .map_err(ManagerError::Rpc)?;
            runtime
                .wait_session_open(&request_id, timeout)
                .map_err(ManagerError::Rpc)?
        };
        if self.active_sessions.contains_key(&session_id) {
            return Err(ManagerError::DuplicateSession(session_id));
        }
        self.active_sessions.insert(session_id.clone(), id.clone());
        Ok(session_id)
    }

    /// Routes input by an explicit manager-owned session identity.
    pub fn input_session(
        &mut self,
        id: &AdapterId,
        session_id: &SessionId,
        data: String,
    ) -> Result<(), ManagerError> {
        self.session_runtime(id, session_id)?
            .send_session_input(session_id, data)
            .map_err(ManagerError::Rpc)
    }

    /// Routes geometry changes by an explicit manager-owned session identity.
    pub fn resize_session(
        &mut self,
        id: &AdapterId,
        session_id: &SessionId,
        rows: u16,
        columns: u16,
    ) -> Result<(), ManagerError> {
        self.session_runtime(id, session_id)?
            .resize_session(session_id, rows, columns)
            .map_err(ManagerError::Rpc)
    }

    /// Requests closure through the session's declared provider.
    pub fn close_session(
        &mut self,
        id: &AdapterId,
        session_id: &SessionId,
    ) -> Result<(), ManagerError> {
        self.session_runtime(id, session_id)?
            .close_session(session_id)
            .map_err(ManagerError::Rpc)?;
        self.closing_sessions.insert(session_id.clone());
        Ok(())
    }

    /// Creates a controller-owned pending action operation. Dispatch and every
    /// later lifecycle transition occur only from this manager's poll loop.
    pub fn start_action_operation(
        &mut self,
        id: &AdapterId,
        action_id: &ActionId,
        payload: Value,
    ) -> Result<AdapterOperation, ManagerError> {
        let action = self
            .runtimes
            .get(id)
            .ok_or_else(|| ManagerError::NotRunning(id.clone()))?
            .actions()
            .iter()
            .find(|action| action.id == *action_id)
            .cloned()
            .ok_or_else(|| ManagerError::UnknownAction {
                adapter_id: id.clone(),
                action_id: action_id.clone(),
            })?;
        self.evict_completed_operations();
        if self.operations.len() == OPERATION_HISTORY_CAPACITY {
            return Err(ManagerError::OperationCapacity);
        }
        let operation_id = OperationId::new(format!("operation-{}", self.next_operation))
            .map_err(|error| ManagerError::OperationId(error.to_string()))?;
        self.next_operation = self.next_operation.saturating_add(1);
        let operation = AdapterOperation {
            id: operation_id.clone(),
            adapter_id: id.clone(),
            action_id: action.id.clone(),
            action_label: action.label.clone(),
            state: OperationState::Pending,
        };
        self.operation_order.push_back(operation_id.clone());
        self.operations.insert(
            operation_id,
            OperationEntry {
                operation: operation.clone(),
                action,
                payload: Some(payload),
                request_id: None,
            },
        );
        Ok(operation)
    }

    /// Returns the bounded controller-owned operation window in creation order.
    pub fn operations(&self) -> Vec<AdapterOperation> {
        self.operation_order
            .iter()
            .filter_map(|id| self.operations.get(id))
            .map(|entry| entry.operation.clone())
            .collect()
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

    /// Drains the bounded session-output queue owned by this manager.
    pub fn take_session_events(&mut self) -> Vec<AdapterSessionEvent> {
        std::mem::take(&mut self.session_events)
            .into_iter()
            .collect()
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

    fn dispatch_pending_operations(&mut self) {
        let pending = self
            .operations
            .iter()
            .filter_map(|(id, entry)| {
                matches!(entry.operation.state, OperationState::Pending).then_some(id.clone())
            })
            .collect::<Vec<_>>();
        for operation_id in pending {
            let Some(entry) = self.operations.get(&operation_id) else {
                continue;
            };
            let adapter_id = entry.operation.adapter_id.clone();
            let action = entry.action.clone();
            let payload = entry.payload.clone().unwrap_or(Value::Null);
            let result = self
                .runtimes
                .get_mut(&adapter_id)
                .ok_or(RpcError::Crashed)
                .and_then(|runtime| {
                    runtime.send_action_request(&action, payload, Duration::from_secs(2))
                });
            let Some(entry) = self.operations.get_mut(&operation_id) else {
                continue;
            };
            match result {
                Ok(request_id) => {
                    entry.request_id = Some(request_id);
                    entry.payload = None;
                    entry.operation.state = OperationState::Running;
                }
                Err(error) => {
                    let (code, message) = operation_rpc_failure(error);
                    entry.payload = None;
                    entry.operation.state = OperationState::Failed { code, message };
                }
            }
        }
    }

    fn reconcile_operations(&mut self) {
        let running = self
            .operations
            .iter()
            .filter_map(
                |(id, entry)| match (&entry.operation.state, &entry.request_id) {
                    (OperationState::Running, Some(request_id)) => Some((
                        id.clone(),
                        entry.operation.adapter_id.clone(),
                        request_id.clone(),
                    )),
                    _ => None,
                },
            )
            .collect::<Vec<_>>();
        for (operation_id, adapter_id, request_id) in running {
            let terminal = self.runtimes.get_mut(&adapter_id).map_or_else(
                || {
                    Some(OperationState::Failed {
                        code: "adapter_unavailable".to_owned(),
                        message: format!("adapter {adapter_id} is not running"),
                    })
                },
                |runtime| {
                    runtime
                        .take_outcome(&request_id)
                        .map(|outcome| match outcome {
                            RpcOutcome::Response(payload) => OperationState::Succeeded { payload },
                            RpcOutcome::AdapterError { code, message } => {
                                OperationState::Failed { code, message }
                            }
                        })
                        .or_else(|| {
                            runtime.take_request_failure(&request_id).map(|error| {
                                let (code, message) = operation_rpc_failure(error);
                                OperationState::Failed { code, message }
                            })
                        })
                },
            );
            if let (Some(state), Some(entry)) = (terminal, self.operations.get_mut(&operation_id)) {
                entry.request_id = None;
                entry.operation.state = state;
            }
        }
    }

    fn fail_active_operations(&mut self, adapter_id: &AdapterId, code: &str, message: String) {
        for entry in self.operations.values_mut().filter(|entry| {
            entry.operation.adapter_id == *adapter_id && !entry.operation.state.is_terminal()
        }) {
            entry.payload = None;
            entry.request_id = None;
            entry.operation.state = OperationState::Failed {
                code: code.to_owned(),
                message: message.clone(),
            };
        }
    }

    fn evict_completed_operations(&mut self) {
        while self.operations.len() >= OPERATION_HISTORY_CAPACITY {
            let Some(index) = self.operation_order.iter().position(|id| {
                self.operations
                    .get(id)
                    .is_some_and(|entry| entry.operation.state.is_terminal())
            }) else {
                break;
            };
            if let Some(id) = self.operation_order.remove(index) {
                self.operations.remove(&id);
            }
        }
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

    fn push_session_event(&mut self, event: AdapterSessionEvent) {
        if self.session_events.len() == SESSION_EVENT_QUEUE_CAPACITY {
            self.session_events.pop_front();
        }
        self.session_events.push_back(event);
    }

    fn session_runtime(
        &mut self,
        id: &AdapterId,
        session_id: &SessionId,
    ) -> Result<&mut AdapterRuntime, ManagerError> {
        if self.closing_sessions.contains(session_id) {
            return Err(ManagerError::SessionClosing(session_id.clone()));
        }
        match self.active_sessions.get(session_id) {
            Some(owner) if owner == id => self
                .runtimes
                .get_mut(id)
                .ok_or_else(|| ManagerError::NotRunning(id.clone())),
            Some(_) => Err(ManagerError::SessionAdapterMismatch {
                adapter_id: id.clone(),
                session_id: session_id.clone(),
            }),
            None => Err(ManagerError::UnknownSession(session_id.clone())),
        }
    }

    fn remove_sessions_for_adapter(&mut self, adapter_id: &AdapterId) {
        self.active_sessions.retain(|session_id, owner| {
            if owner == adapter_id {
                self.closing_sessions.remove(session_id);
                false
            } else {
                true
            }
        });
    }

    fn disconnect_sessions_for_adapter(&mut self, adapter_id: &AdapterId, reason: String) {
        let sessions: Vec<_> = self
            .active_sessions
            .iter()
            .filter_map(|(session_id, owner)| (owner == adapter_id).then_some(session_id.clone()))
            .collect();
        for session_id in sessions {
            self.active_sessions.remove(&session_id);
            self.closing_sessions.remove(&session_id);
            self.push_session_event(AdapterSessionEvent::Disconnected {
                adapter_id: adapter_id.clone(),
                session_id,
                reason: reason.clone(),
            });
        }
    }
}

#[derive(Clone, Debug)]
struct OperationEntry {
    operation: AdapterOperation,
    action: AdapterAction,
    payload: Option<Value>,
    request_id: Option<RequestId>,
}

fn operation_rpc_failure(error: RpcError) -> (String, String) {
    let code = match error {
        RpcError::Timeout => "timeout",
        RpcError::Crashed => "adapter_crashed",
        RpcError::Backpressure => "backpressure",
        RpcError::Failed(_) => "transport_failed",
    }
    .to_owned();
    (code, error.to_string())
}

/// Generic adapter data drained by a controller without capability-specific decoding.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AdapterLiveData {
    pub events: Vec<AdapterEvent>,
    pub disconnects: Vec<AdapterDisconnect>,
}

/// One typed output or terminal-state record from a manager-owned session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AdapterSessionEvent {
    Output {
        adapter_id: AdapterId,
        session_id: SessionId,
        data: String,
    },
    Exited {
        adapter_id: AdapterId,
        session_id: SessionId,
        exit_code: Option<i32>,
    },
    Disconnected {
        adapter_id: AdapterId,
        session_id: SessionId,
        reason: String,
    },
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
    UnknownSessionCapability {
        adapter_id: AdapterId,
        capability: Capability,
    },
    SessionCapacity,
    DuplicateSession(SessionId),
    SessionClosing(SessionId),
    UnknownSession(SessionId),
    SessionAdapterMismatch {
        adapter_id: AdapterId,
        session_id: SessionId,
    },
    OperationCapacity,
    OperationId(String),
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
            Self::UnknownSessionCapability {
                adapter_id,
                capability,
            } => write!(
                formatter,
                "unknown session capability {capability} for adapter {adapter_id}"
            ),
            Self::SessionCapacity => write!(formatter, "active session capacity is full"),
            Self::DuplicateSession(id) => write!(formatter, "duplicate session {id}"),
            Self::SessionClosing(id) => write!(formatter, "session {id} is closing"),
            Self::UnknownSession(id) => write!(formatter, "unknown session {id}"),
            Self::SessionAdapterMismatch {
                adapter_id,
                session_id,
            } => write!(
                formatter,
                "session {session_id} is not owned by adapter {adapter_id}"
            ),
            Self::OperationCapacity => write!(formatter, "operation retention capacity is full"),
            Self::OperationId(error) => error.fmt(formatter),
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
