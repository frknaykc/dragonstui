use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    AdapterDiagnostics, AdapterId, AdapterLiveData, AdapterManager, AdapterState, DiscoveryError,
    LocalAdapterRoot, ManagerError,
};

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

    pub fn poll(&mut self, per_adapter_timeout: Duration) {
        self.manager.poll(per_adapter_timeout);
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
