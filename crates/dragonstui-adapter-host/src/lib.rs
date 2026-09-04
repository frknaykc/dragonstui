//! Process-isolated, capability-driven adapter hosting for DragonsTUI.
//!
//! This crate is intentionally separate from `dragons_tui`: framework-only consumers do not
//! need protocol serialization, local discovery, or child-process management.

#![forbid(unsafe_code)]

mod capabilities;
mod controller;
mod controller_ipc;
mod discovery;
mod installer;
mod management;
mod manager;
mod manifest;
mod operations;
mod process;
mod protocol;
mod registry;
mod runtime;

pub use capabilities::CapabilityRegistry;
pub use controller::{AdapterController, ControllerError};
pub use controller_ipc::{
    ControllerActionClient, ControllerActionOutcome, ControllerActionResponse, ControllerClient,
    ControllerIpcCommand, ControllerIpcDiagnostics, ControllerIpcError, ControllerIpcServer,
    ControllerIpcStatus, ControllerManagementClient, ControllerManagementClientError,
    ControllerManagementRequest, ControllerManagementResponse, ControllerOperationClient,
    LocalControllerError, local_controller_action_client, local_controller_diagnostics,
    local_controller_live_data, local_controller_management_client,
    local_controller_operation_client,
};
pub use discovery::{
    AdapterClassification, DiscoveredAdapter, DiscoveryError, LocalAdapterRoot, MANIFEST_FILE_NAME,
};
pub use installer::{
    AdapterInstaller, INSTALL_METADATA_FILE_NAME, InstallError, InstallMetadata, InstallReceipt,
    PreparedUpdate,
};
pub use management::{
    AdapterManagement, AdapterManagementAction, AdapterManagementError, AdapterManagementOutcome,
};
pub use manager::{
    AdapterDiagnostics, AdapterDisconnect, AdapterLiveData, AdapterManager, ManagerError,
};
pub use manifest::{AdapterManifest, ExecutablePath, ManifestError};
pub use operations::{AdapterOperation, OperationId, OperationState};
pub use process::{AdapterProcess, AdapterProcessConfig, ProcessError, ProcessStatus};
pub use protocol::{
    ActionId, AdapterAction, AdapterId, AdapterInfo, Capability, ErrorMessage, Event, Hello,
    IdentifierError, Observation, ObservationKind, ObservationSeverity, ObservationStatus,
    PROTOCOL_VERSION, ProtocolMessage, Request, RequestId, Response, Shutdown, ShutdownAck,
};
pub use registry::{
    AdapterArtifact, AdapterEntry, AdapterRelease, ArtifactSource, Platform, Registry,
    RegistryError, RegistryLoadError, Sha256Checksum,
};
pub use runtime::{
    AdapterEvent, AdapterRuntime, AdapterRuntimeConfig, AdapterStartError, AdapterState, RpcError,
    RpcOutcome,
};
