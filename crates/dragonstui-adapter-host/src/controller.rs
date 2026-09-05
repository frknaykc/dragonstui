use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    ActionId, AdapterAction, AdapterDiagnostics, AdapterId, AdapterLiveData, AdapterManager,
    AdapterOperation, AdapterSession, AdapterSessionEvent, AdapterState, Capability,
    DiscoveryError, LocalAdapterRoot, ManagerError, RpcOutcome, SessionId,
};
use serde_json::Value;

/// Stateful lifecycle owner intended for a long-lived host controller process.
///
/// The controller performs discovery only when an adapter has not yet entered
/// host state. It never installs or updates artifacts, and its lifetime—not a
/// one-shot CLI invocation—owns every started child process.
#[derive(Debug)]
pub struct AdapterController {
    root: PathBuf,
    manager: AdapterManager,
}

impl AdapterController {
    pub fn new(root: impl Into<PathBuf>, stop_timeout: Duration, event_capacity: usize) -> Self {
        Self {
            root: root.into(),
            manager: AdapterManager::new(stop_timeout, event_capacity),
        }
    }

    /// Discovers current local manifests without starting any process.
    pub fn discover(&mut self) -> Result<(), ControllerError> {
        self.manager
            .discover(LocalAdapterRoot::new(&self.root))
            .map(|_| ())
            .map_err(ControllerError::Discovery)
    }

    pub fn start(&mut self, id: &AdapterId) -> Result<(), ControllerError> {
        self.ensure_discovered(id)?;
        self.manager.start(id).map_err(ControllerError::Manager)
    }

    pub fn stop(&mut self, id: &AdapterId) -> Result<(), ControllerError> {
        self.ensure_discovered(id)?;
        self.manager.stop(id).map_err(ControllerError::Manager)
    }

    pub fn restart(&mut self, id: &AdapterId) -> Result<(), ControllerError> {
        self.ensure_discovered(id)?;
        self.manager.restart(id).map_err(ControllerError::Manager)
    }

    /// Stops a running adapter if necessary and drops all controller-owned
    /// lifecycle, diagnostics, and capability state before replacement/removal.
    pub fn unregister(&mut self, id: &AdapterId) -> Result<(), ControllerError> {
        self.ensure_discovered(id)?;
        self.manager
            .unregister(id)
            .map_err(ControllerError::Manager)
    }

    /// Stops and clears controller-owned state when an installed adapter is
    /// known locally; returns false when no valid local adapter exists.
    pub fn unregister_if_present(&mut self, id: &AdapterId) -> Result<bool, ControllerError> {
        if self.manager.state(id).is_none() {
            self.discover()?;
        }
        if self.manager.state(id).is_none() {
            return Ok(false);
        }
        self.manager
            .unregister(id)
            .map_err(ControllerError::Manager)?;
        Ok(true)
    }

    pub fn state(&self, id: &AdapterId) -> Option<AdapterState> {
        self.manager.state(id)
    }

    pub fn diagnostics(&self, id: &AdapterId) -> Option<AdapterDiagnostics> {
        self.manager.diagnostics(id)
    }

    /// Returns adapter-declared actions from the currently running runtime.
    pub fn actions(&self, id: &AdapterId) -> Result<Vec<AdapterAction>, ControllerError> {
        self.manager.actions(id).map_err(ControllerError::Manager)
    }

    /// Returns only the session declarations provided by a running adapter.
    pub fn sessions(&self, id: &AdapterId) -> Result<Vec<AdapterSession>, ControllerError> {
        self.manager.sessions(id).map_err(ControllerError::Manager)
    }

    /// Invokes one exact producer-declared action through the existing runtime
    /// request queue and returns its typed generic RPC outcome.
    pub fn invoke_action(
        &mut self,
        id: &AdapterId,
        action_id: &ActionId,
        payload: Value,
    ) -> Result<RpcOutcome, ControllerError> {
        self.ensure_discovered(id)?;
        let request_id = self
            .manager
            .invoke_action(id, action_id, payload, Duration::from_secs(2))
            .map_err(ControllerError::Manager)?;
        self.manager
            .wait_response(id, &request_id, Duration::from_secs(2))
            .map_err(ControllerError::Manager)
    }

    /// Opens one provider-declared interactive session through the manager-owned
    /// adapter runtime. The controller remains the lifecycle authority.
    pub fn open_session(
        &mut self,
        id: &AdapterId,
        capability: &Capability,
        rows: u16,
        columns: u16,
    ) -> Result<SessionId, ControllerError> {
        self.ensure_discovered(id)?;
        self.manager
            .open_session(id, capability, rows, columns, Duration::from_secs(2))
            .map_err(ControllerError::Manager)
    }

    pub fn input_session(
        &mut self,
        id: &AdapterId,
        session_id: &SessionId,
        data: String,
    ) -> Result<(), ControllerError> {
        self.manager
            .input_session(id, session_id, data)
            .map_err(ControllerError::Manager)
    }

    pub fn resize_session(
        &mut self,
        id: &AdapterId,
        session_id: &SessionId,
        rows: u16,
        columns: u16,
    ) -> Result<(), ControllerError> {
        self.manager
            .resize_session(id, session_id, rows, columns)
            .map_err(ControllerError::Manager)
    }

    pub fn close_session(
        &mut self,
        id: &AdapterId,
        session_id: &SessionId,
    ) -> Result<(), ControllerError> {
        self.manager
            .close_session(id, session_id)
            .map_err(ControllerError::Manager)
    }

    /// Returns controller-owned session liveness for bounded-event reconciliation.
    pub fn session_active(
        &mut self,
        id: &AdapterId,
        session_id: &SessionId,
    ) -> Result<bool, ControllerError> {
        self.ensure_discovered(id)?;
        Ok(self.manager.session_active(id, session_id))
    }

    /// Drains bounded typed session output without reusing observability data.
    pub fn take_session_events(&mut self) -> Vec<AdapterSessionEvent> {
        self.manager.take_session_events()
    }

    pub fn poll(&mut self, per_adapter_timeout: Duration) {
        self.manager.poll(per_adapter_timeout);
    }

    /// Starts one controller-owned asynchronous action operation.
    pub fn start_action_operation(
        &mut self,
        id: &AdapterId,
        action_id: &ActionId,
        payload: Value,
    ) -> Result<AdapterOperation, ControllerError> {
        self.ensure_discovered(id)?;
        self.manager
            .start_action_operation(id, action_id, payload)
            .map_err(ControllerError::Manager)
    }

    /// Returns the controller's bounded action operation window.
    pub fn operations(&self) -> Vec<AdapterOperation> {
        self.manager.operations()
    }

    /// Drains generic live data produced by already-running adapters.
    pub fn take_live_data(&mut self) -> AdapterLiveData {
        self.manager.take_live_data()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn ensure_discovered(&mut self, id: &AdapterId) -> Result<(), ControllerError> {
        if self.manager.state(id).is_none() {
            self.discover()?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum ControllerError {
    Discovery(DiscoveryError),
    Manager(ManagerError),
}

impl fmt::Display for ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discovery(error) => error.fmt(formatter),
            Self::Manager(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Discovery(error) => Some(error),
            Self::Manager(error) => Some(error),
        }
    }
}
