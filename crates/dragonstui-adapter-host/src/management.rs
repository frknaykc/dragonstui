use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    AdapterController, AdapterId, AdapterInstaller, ControllerError, ControllerIpcDiagnostics,
    InstallError, Platform, Registry, RegistryLoadError,
};
use serde::{Deserialize, Serialize};

/// Host-owned management operation. Installation remains distinct from start.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdapterManagementAction {
    Install {
        id: AdapterId,
        registry_source: String,
        version: Option<String>,
    },
    Update {
        id: AdapterId,
        registry_source: String,
    },
    Remove {
        id: AdapterId,
    },
    Start {
        id: AdapterId,
    },
    Stop {
        id: AdapterId,
    },
    Restart {
        id: AdapterId,
    },
}

impl AdapterManagementAction {
    pub fn id(&self) -> &AdapterId {
        match self {
            Self::Install { id, .. }
            | Self::Update { id, .. }
            | Self::Remove { id }
            | Self::Start { id }
            | Self::Stop { id }
            | Self::Restart { id } => id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdapterManagementOutcome {
    Installed { id: AdapterId, version: String },
    Updated { id: AdapterId, version: String },
    Removed { id: AdapterId },
    Started { id: AdapterId },
    Stopped { id: AdapterId },
    Restarted { id: AdapterId },
}

/// Synchronous host-side coordinator for an application-owned background worker.
/// It centralizes installer and lifecycle calls without adding any dependency to
/// the core `dragons_tui` framework.
#[derive(Debug)]
pub struct AdapterManagement {
    root: PathBuf,
    controller: AdapterController,
}

impl AdapterManagement {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            controller: AdapterController::new(&root, Duration::from_secs(2), 128),
            root,
        }
    }

    pub fn with_controller(controller: AdapterController) -> Self {
        Self {
            root: controller.root().to_path_buf(),
            controller,
        }
    }

    pub fn execute(
        &mut self,
        action: AdapterManagementAction,
    ) -> Result<AdapterManagementOutcome, AdapterManagementError> {
        match action {
            AdapterManagementAction::Install {
                id,
                registry_source,
                version,
            } => {
                let receipt = AdapterInstaller::new(&self.root)
                    .registry_source(&registry_source)
                    .install(
                        &Registry::load(&registry_source)
                            .map_err(AdapterManagementError::Registry)?,
                        &id,
                        version.as_deref(),
                        &Platform::current().map_err(AdapterManagementError::Platform)?,
                    )
                    .map_err(AdapterManagementError::Install)?;
                Ok(AdapterManagementOutcome::Installed {
                    id: receipt.adapter_id,
                    version: receipt.version,
                })
            }
            AdapterManagementAction::Update {
                id,
                registry_source,
            } => {
                let was_running = self.controller.diagnostics(&id).is_some_and(|diagnostics| {
                    matches!(diagnostics.state, crate::AdapterState::Running)
                });
                let installer = AdapterInstaller::new(&self.root).registry_source(&registry_source);
                let prepared = installer
                    .prepare_update(
                        &Registry::load(&registry_source)
                            .map_err(AdapterManagementError::Registry)?,
                        &id,
                        &Platform::current().map_err(AdapterManagementError::Platform)?,
                    )
                    .map_err(AdapterManagementError::Install)?;
                self.controller
                    .unregister_if_present(&id)
                    .map_err(AdapterManagementError::Controller)?;
                let receipt = match installer.commit_update(prepared) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        if was_running {
                            let _ = self.controller.start(&id);
                        }
                        return Err(AdapterManagementError::Install(error));
                    }
                };
                if was_running {
                    self.controller
                        .start(&id)
                        .map_err(AdapterManagementError::Controller)?;
                }
                Ok(AdapterManagementOutcome::Updated {
                    id: receipt.adapter_id,
                    version: receipt.version,
                })
            }
            AdapterManagementAction::Remove { id } => {
                self.controller
                    .unregister_if_present(&id)
                    .map_err(AdapterManagementError::Controller)?;
                AdapterInstaller::new(&self.root)
                    .remove(&id)
                    .map_err(AdapterManagementError::Install)?;
                Ok(AdapterManagementOutcome::Removed { id })
            }
            AdapterManagementAction::Start { id } => {
                self.controller
                    .start(&id)
                    .map_err(AdapterManagementError::Controller)?;
                Ok(AdapterManagementOutcome::Started { id })
            }
            AdapterManagementAction::Stop { id } => {
                self.controller
                    .stop(&id)
                    .map_err(AdapterManagementError::Controller)?;
                Ok(AdapterManagementOutcome::Stopped { id })
            }
            AdapterManagementAction::Restart { id } => {
                self.controller
                    .restart(&id)
                    .map_err(AdapterManagementError::Controller)?;
                Ok(AdapterManagementOutcome::Restarted { id })
            }
        }
    }

    pub fn diagnostics(&self, id: &AdapterId) -> Option<ControllerIpcDiagnostics> {
        self.controller
            .diagnostics(id)
            .map(ControllerIpcDiagnostics::from)
    }

    pub fn state(&self, id: &AdapterId) -> Option<crate::AdapterState> {
        self.controller.state(id)
    }

    pub fn poll(&mut self, per_adapter_timeout: Duration) {
        self.controller.poll(per_adapter_timeout);
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug)]
pub enum AdapterManagementError {
    Registry(RegistryLoadError),
    Platform(crate::RegistryError),
    Install(InstallError),
    Controller(ControllerError),
}

impl fmt::Display for AdapterManagementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => error.fmt(formatter),
            Self::Platform(error) => error.fmt(formatter),
            Self::Install(error) => error.fmt(formatter),
            Self::Controller(error) => error.fmt(formatter),
        }
    }
}

impl Error for AdapterManagementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Registry(error) => Some(error),
            Self::Platform(error) => Some(error),
            Self::Install(error) => Some(error),
            Self::Controller(error) => Some(error),
        }
    }
}
