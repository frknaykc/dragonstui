use std::{
    error::Error,
    fmt, fs,
    io::{self, BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{
    AdapterController, AdapterDiagnostics, AdapterId, AdapterLiveData, AdapterManagementOutcome,
};

/// Loopback-only controller command. A daemon persists the controller and
/// serves one newline-delimited JSON request per client connection.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ControllerIpcCommand {
    Status {
        id: String,
    },
    Diagnostics {
        id: String,
    },
    Start {
        id: String,
    },
    Stop {
        id: String,
    },
    Restart {
        id: String,
    },
    Unregister {
        id: String,
    },
    Management {
        request: ControllerManagementRequest,
    },
    LiveData,
    Shutdown,
}

/// Typed daemon-owned lifecycle request. Legacy lifecycle commands remain
/// available during the migration, but both paths reach the same controller.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ControllerManagementRequest {
    Start { id: String },
    Stop { id: String },
    Restart { id: String },
    Diagnostics { id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ControllerManagementResponse {
    Lifecycle {
        outcome: AdapterManagementOutcome,
    },
    Diagnostics {
        diagnostics: Option<Box<ControllerIpcDiagnostics>>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ControllerIpcStatus {
    Missing,
    State(String),
    Diagnostics(Box<ControllerIpcDiagnostics>),
    Management(ControllerManagementResponse),
    LiveData(AdapterLiveData),
    Completed,
}

/// Serializable controller-owned runtime diagnostics. This snapshot is host
/// data only; it never reads or executes an adapter on behalf of a client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControllerIpcDiagnostics {
    pub adapter_id: String,
    pub version: Option<String>,
    pub protocol: Option<u32>,
    pub state: String,
    pub pid: Option<u32>,
    pub uptime_millis: Option<u64>,
    pub capabilities: Vec<String>,
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

impl From<AdapterDiagnostics> for ControllerIpcDiagnostics {
    fn from(diagnostics: AdapterDiagnostics) -> Self {
        Self {
            adapter_id: diagnostics.adapter_id.to_string(),
            version: diagnostics.version,
            protocol: diagnostics.protocol,
            state: format!("{:?}", diagnostics.state).to_ascii_lowercase(),
            pid: diagnostics.pid,
            uptime_millis: diagnostics
                .uptime
                .and_then(|uptime| u64::try_from(uptime.as_millis()).ok()),
            capabilities: diagnostics
                .capabilities
                .into_iter()
                .map(|capability| capability.to_string())
                .collect(),
            last_error: diagnostics.last_error,
            stderr_tail: diagnostics.stderr_tail,
            stderr_dropped_line_count: diagnostics.stderr_dropped_line_count,
            dropped_event_count: diagnostics.dropped_event_count,
            pending_request_count: diagnostics.pending_request_count,
            response_queue_capacity: diagnostics.response_queue_capacity,
            response_queue_len: diagnostics.response_queue_len,
            event_queue_capacity: diagnostics.event_queue_capacity,
            event_queue_len: diagnostics.event_queue_len,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ManagementCommand {
    Start(AdapterId),
    Stop(AdapterId),
    Restart(AdapterId),
    Diagnostics(AdapterId),
}

#[derive(Debug)]
enum ManagementResult {
    Lifecycle(AdapterManagementOutcome),
    Diagnostics(Box<Option<ControllerIpcDiagnostics>>),
}

fn adapter_id(id: &str) -> Result<AdapterId, ControllerIpcError> {
    AdapterId::new(id).map_err(|error| ControllerIpcError::InvalidId(error.to_string()))
}

fn legacy_management_status(result: ManagementResult) -> ControllerIpcStatus {
    match result {
        ManagementResult::Lifecycle(_) => ControllerIpcStatus::Completed,
        ManagementResult::Diagnostics(diagnostics) => match *diagnostics {
            Some(diagnostics) => ControllerIpcStatus::Diagnostics(Box::new(diagnostics)),
            None => ControllerIpcStatus::Missing,
        },
    }
}

fn typed_management_response(result: ManagementResult) -> ControllerManagementResponse {
    match result {
        ManagementResult::Lifecycle(outcome) => ControllerManagementResponse::Lifecycle { outcome },
        ManagementResult::Diagnostics(diagnostics) => ControllerManagementResponse::Diagnostics {
            diagnostics: (*diagnostics).map(Box::new),
        },
    }
}

fn legacy_management_command(
    command: &ControllerIpcCommand,
) -> Result<Option<ManagementCommand>, ControllerIpcError> {
    match command {
        ControllerIpcCommand::Start { id } => {
            adapter_id(id).map(ManagementCommand::Start).map(Some)
        }
        ControllerIpcCommand::Stop { id } => adapter_id(id).map(ManagementCommand::Stop).map(Some),
        ControllerIpcCommand::Restart { id } => {
            adapter_id(id).map(ManagementCommand::Restart).map(Some)
        }
        ControllerIpcCommand::Diagnostics { id } => {
            adapter_id(id).map(ManagementCommand::Diagnostics).map(Some)
        }
        _ => Ok(None),
    }
}

fn typed_management_command(
    request: &ControllerManagementRequest,
) -> Result<ManagementCommand, ControllerIpcError> {
    match request {
        ControllerManagementRequest::Start { id } => adapter_id(id).map(ManagementCommand::Start),
        ControllerManagementRequest::Stop { id } => adapter_id(id).map(ManagementCommand::Stop),
        ControllerManagementRequest::Restart { id } => {
            adapter_id(id).map(ManagementCommand::Restart)
        }
        ControllerManagementRequest::Diagnostics { id } => {
            adapter_id(id).map(ManagementCommand::Diagnostics)
        }
    }
}

#[derive(Debug)]
pub struct ControllerIpcServer {
    listener: TcpListener,
    controller: AdapterController,
    token: String,
}

impl ControllerIpcServer {
    pub fn new(
        listener: TcpListener,
        controller: AdapterController,
        token: impl Into<String>,
    ) -> Self {
        Self {
            listener,
            controller,
            token: token.into(),
        }
    }

    /// Serves an explicit bounded number of clients. The daemon runner owns
    /// the unbounded loop so tests can remain deterministic.
    pub fn serve_requests(mut self, count: usize) -> Result<(), ControllerIpcError> {
        for _ in 0..count {
            let (stream, _) = self.listener.accept().map_err(ControllerIpcError::Accept)?;
            if self.serve_one(stream)? {
                break;
            }
        }
        Ok(())
    }

    /// Runs until a locally authenticated client requests shutdown.
    pub fn serve_forever(mut self) -> Result<(), ControllerIpcError> {
        self.listener
            .set_nonblocking(true)
            .map_err(ControllerIpcError::Accept)?;
        loop {
            self.controller.poll(Duration::ZERO);
            match self.listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .map_err(ControllerIpcError::Accept)?;
                    if self.serve_one(stream)? {
                        return Ok(());
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(ControllerIpcError::Accept(error)),
            }
        }
    }

    fn serve_one(&mut self, mut stream: TcpStream) -> Result<bool, ControllerIpcError> {
        let (response, shutdown) = match read_request(&stream) {
            Ok(request) if request.token != self.token => {
                (WireResponse::failure("authentication failed"), false)
            }
            Ok(request) => match self.handle(request.command) {
                Ok((status, shutdown)) => (WireResponse::success(status), shutdown),
                Err(error) => (WireResponse::failure(error.to_string()), false),
            },
            Err(error) => (WireResponse::failure(error.to_string()), false),
        };
        if let Err(error) = serde_json::to_writer(&mut stream, &response) {
            if error.io_error_kind().is_some_and(is_peer_disconnect) {
                return Ok(shutdown);
            }
            return Err(ControllerIpcError::Encode(error));
        }
        if let Err(error) = stream.write_all(b"\n") {
            if is_peer_disconnect(error.kind()) {
                return Ok(shutdown);
            }
            return Err(ControllerIpcError::Write(error));
        }
        Ok(shutdown)
    }

    fn handle(
        &mut self,
        command: ControllerIpcCommand,
    ) -> Result<(ControllerIpcStatus, bool), ControllerIpcError> {
        if matches!(command, ControllerIpcCommand::Shutdown) {
            return Ok((ControllerIpcStatus::Completed, true));
        }
        if let Some(management) = legacy_management_command(&command)? {
            return self
                .dispatch_management(management)
                .map(|result| (legacy_management_status(result), false));
        }
        match command {
            ControllerIpcCommand::Status { id } => {
                let id = adapter_id(&id)?;
                Ok((
                    self.controller
                        .state(&id)
                        .map(|state| {
                            ControllerIpcStatus::State(format!("{state:?}").to_ascii_lowercase())
                        })
                        .unwrap_or(ControllerIpcStatus::Missing),
                    false,
                ))
            }
            ControllerIpcCommand::Unregister { id } => {
                let id = adapter_id(&id)?;
                self.controller
                    .unregister(&id)
                    .map_err(ControllerIpcError::Controller)
                    .map(|_| (ControllerIpcStatus::Completed, false))
            }
            ControllerIpcCommand::Management { request } => self
                .dispatch_management(typed_management_command(&request)?)
                .map(|result| {
                    (
                        ControllerIpcStatus::Management(typed_management_response(result)),
                        false,
                    )
                }),
            ControllerIpcCommand::LiveData => Ok((
                ControllerIpcStatus::LiveData(self.controller.take_live_data()),
                false,
            )),
            ControllerIpcCommand::Diagnostics { .. }
            | ControllerIpcCommand::Start { .. }
            | ControllerIpcCommand::Stop { .. }
            | ControllerIpcCommand::Restart { .. } => {
                unreachable!("legacy management commands return above")
            }
            ControllerIpcCommand::Shutdown => unreachable!("shutdown is handled above"),
        }
    }

    fn dispatch_management(
        &mut self,
        command: ManagementCommand,
    ) -> Result<ManagementResult, ControllerIpcError> {
        match command {
            ManagementCommand::Start(id) => self
                .controller
                .start(&id)
                .map_err(ControllerIpcError::Controller)
                .map(|_| ManagementResult::Lifecycle(AdapterManagementOutcome::Started { id })),
            ManagementCommand::Stop(id) => self
                .controller
                .stop(&id)
                .map_err(ControllerIpcError::Controller)
                .map(|_| ManagementResult::Lifecycle(AdapterManagementOutcome::Stopped { id })),
            ManagementCommand::Restart(id) => self
                .controller
                .restart(&id)
                .map_err(ControllerIpcError::Controller)
                .map(|_| ManagementResult::Lifecycle(AdapterManagementOutcome::Restarted { id })),
            ManagementCommand::Diagnostics(id) => Ok(ManagementResult::Diagnostics(Box::new(
                self.controller
                    .diagnostics(&id)
                    .map(ControllerIpcDiagnostics::from),
            ))),
        }
    }
}

fn is_peer_disconnect(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
    )
}

#[derive(Clone, Debug)]
pub struct ControllerClient {
    address: SocketAddr,
    token: String,
}

impl ControllerClient {
    pub fn new(address: SocketAddr, token: impl Into<String>) -> Self {
        Self {
            address,
            token: token.into(),
        }
    }

    pub fn status(&self, id: &AdapterId) -> Result<ControllerIpcStatus, ControllerIpcError> {
        self.call(ControllerIpcCommand::Status { id: id.to_string() })
    }

    pub fn diagnostics(
        &self,
        id: &AdapterId,
    ) -> Result<Option<ControllerIpcDiagnostics>, ControllerIpcError> {
        match self.call(ControllerIpcCommand::Diagnostics { id: id.to_string() })? {
            ControllerIpcStatus::Diagnostics(diagnostics) => Ok(Some(*diagnostics)),
            ControllerIpcStatus::Missing => Ok(None),
            status => Err(ControllerIpcError::UnexpectedStatus(format!("{status:?}"))),
        }
    }

    pub fn start(&self, id: &AdapterId) -> Result<ControllerIpcStatus, ControllerIpcError> {
        self.call(ControllerIpcCommand::Start { id: id.to_string() })
    }

    pub fn stop(&self, id: &AdapterId) -> Result<ControllerIpcStatus, ControllerIpcError> {
        self.call(ControllerIpcCommand::Stop { id: id.to_string() })
    }

    pub fn restart(&self, id: &AdapterId) -> Result<ControllerIpcStatus, ControllerIpcError> {
        self.call(ControllerIpcCommand::Restart { id: id.to_string() })
    }

    pub fn unregister(&self, id: &AdapterId) -> Result<ControllerIpcStatus, ControllerIpcError> {
        self.call(ControllerIpcCommand::Unregister { id: id.to_string() })
    }

    pub fn shutdown(&self) -> Result<ControllerIpcStatus, ControllerIpcError> {
        self.call(ControllerIpcCommand::Shutdown)
    }

    pub fn live_data(&self) -> Result<AdapterLiveData, ControllerIpcError> {
        match self.call(ControllerIpcCommand::LiveData)? {
            ControllerIpcStatus::LiveData(data) => Ok(data),
            status => Err(ControllerIpcError::UnexpectedStatus(format!("{status:?}"))),
        }
    }

    fn call(
        &self,
        command: ControllerIpcCommand,
    ) -> Result<ControllerIpcStatus, ControllerIpcError> {
        let mut attempts_remaining = CONTROLLER_CONNECT_RETRIES;
        let mut stream = loop {
            match TcpStream::connect_timeout(&self.address, Duration::from_millis(100)) {
                Ok(stream) => break stream,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) && attempts_remaining > 0 =>
                {
                    attempts_remaining -= 1;
                    thread::sleep(CONTROLLER_CONNECT_RETRY_DELAY);
                }
                Err(error) => return Err(ControllerIpcError::Connect(error)),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(ControllerIpcError::Read)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(ControllerIpcError::Write)?;
        let request = WireRequest {
            token: self.token.clone(),
            command,
        };
        serde_json::to_writer(&mut stream, &request).map_err(ControllerIpcError::Encode)?;
        stream.write_all(b"\n").map_err(ControllerIpcError::Write)?;
        let response: WireResponse = read_response(&stream)?;
        response.status.ok_or_else(|| {
            ControllerIpcError::Remote(
                response
                    .error
                    .unwrap_or_else(|| "invalid controller response".to_owned()),
            )
        })
    }
}

/// Typed lifecycle client for the authoritative local controller daemon.
///
/// Each method uses the existing authenticated connect-per-operation transport.
/// This is intentionally blocking; application UI code must call it from its
/// background worker rather than its render/input thread.
#[derive(Clone, Debug)]
pub struct ControllerManagementClient {
    client: ControllerClient,
}

impl ControllerManagementClient {
    pub fn new(address: SocketAddr, token: impl Into<String>) -> Self {
        Self {
            client: ControllerClient::new(address, token),
        }
    }

    pub fn start(
        &self,
        id: &AdapterId,
    ) -> Result<AdapterManagementOutcome, ControllerManagementClientError> {
        self.lifecycle(ControllerManagementRequest::Start { id: id.to_string() })
    }

    pub fn stop(
        &self,
        id: &AdapterId,
    ) -> Result<AdapterManagementOutcome, ControllerManagementClientError> {
        self.lifecycle(ControllerManagementRequest::Stop { id: id.to_string() })
    }

    pub fn restart(
        &self,
        id: &AdapterId,
    ) -> Result<AdapterManagementOutcome, ControllerManagementClientError> {
        self.lifecycle(ControllerManagementRequest::Restart { id: id.to_string() })
    }

    pub fn diagnostics(
        &self,
        id: &AdapterId,
    ) -> Result<Option<ControllerIpcDiagnostics>, ControllerManagementClientError> {
        match self.request(ControllerManagementRequest::Diagnostics { id: id.to_string() })? {
            ControllerManagementResponse::Diagnostics { diagnostics } => {
                Ok(diagnostics.map(|item| *item))
            }
            response => Err(ControllerManagementClientError::UnexpectedResponse(
                response,
            )),
        }
    }

    pub fn live_data(&self) -> Result<AdapterLiveData, ControllerManagementClientError> {
        self.client
            .live_data()
            .map_err(ControllerManagementClientError::from_ipc)
    }

    fn lifecycle(
        &self,
        request: ControllerManagementRequest,
    ) -> Result<AdapterManagementOutcome, ControllerManagementClientError> {
        let response = self.request(request.clone())?;
        match response {
            ControllerManagementResponse::Lifecycle { outcome }
                if matches!(
                    (&request, &outcome),
                    (
                        ControllerManagementRequest::Start { .. },
                        AdapterManagementOutcome::Started { .. }
                    ) | (
                        ControllerManagementRequest::Stop { .. },
                        AdapterManagementOutcome::Stopped { .. }
                    ) | (
                        ControllerManagementRequest::Restart { .. },
                        AdapterManagementOutcome::Restarted { .. }
                    )
                ) =>
            {
                Ok(outcome)
            }
            response => Err(ControllerManagementClientError::UnexpectedResponse(
                response,
            )),
        }
    }

    fn request(
        &self,
        request: ControllerManagementRequest,
    ) -> Result<ControllerManagementResponse, ControllerManagementClientError> {
        match self
            .client
            .call(ControllerIpcCommand::Management { request })
            .map_err(ControllerManagementClientError::from_ipc)?
        {
            ControllerIpcStatus::Management(response) => Ok(response),
            status => Err(ControllerManagementClientError::UnexpectedStatus(status)),
        }
    }
}

const CONTROLLER_DIRECTORY: &str = ".controller";
const CONTROLLER_ENDPOINT: &str = "endpoint.json";
const CONTROLLER_CONNECT_RETRIES: u16 = 250;
const CONTROLLER_CONNECT_RETRY_DELAY: Duration = Duration::from_millis(10);

#[derive(Deserialize)]
struct LocalControllerEndpoint {
    address: SocketAddr,
    token: String,
}

/// Reads the private local controller endpoint without exposing its credential
/// to application callers.
pub fn local_controller_management_client(
    root: &Path,
) -> Result<Option<ControllerManagementClient>, LocalControllerError> {
    let endpoint_path = root.join(CONTROLLER_DIRECTORY).join(CONTROLLER_ENDPOINT);
    let body = match fs::read(&endpoint_path) {
        Ok(body) => body,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(LocalControllerError::Read(error)),
    };
    let endpoint: LocalControllerEndpoint =
        serde_json::from_slice(&body).map_err(LocalControllerError::Decode)?;
    if !endpoint.address.ip().is_loopback() {
        return Err(LocalControllerError::NonLoopbackEndpoint);
    }
    Ok(Some(ControllerManagementClient::new(
        endpoint.address,
        endpoint.token,
    )))
}

/// Returns typed daemon diagnostics if the adapter has controller-owned
/// lifecycle state. The endpoint credential never leaves this module.
pub fn local_controller_diagnostics(
    root: &Path,
    id: &AdapterId,
) -> Result<Option<ControllerIpcDiagnostics>, LocalControllerError> {
    let Some(client) = local_controller_management_client(root)? else {
        return Ok(None);
    };
    client
        .diagnostics(id)
        .map_err(LocalControllerError::Management)
}

/// Drains generic live data from the authenticated local controller.
pub fn local_controller_live_data(
    root: &Path,
) -> Result<Option<AdapterLiveData>, LocalControllerError> {
    let Some(client) = local_controller_management_client(root)? else {
        return Ok(None);
    };
    client
        .live_data()
        .map(Some)
        .map_err(LocalControllerError::Management)
}

#[derive(Deserialize, Serialize)]
struct WireRequest {
    token: String,
    command: ControllerIpcCommand,
}
#[derive(Deserialize, Serialize)]
struct WireResponse {
    status: Option<ControllerIpcStatus>,
    error: Option<String>,
}
impl WireResponse {
    fn success(status: ControllerIpcStatus) -> Self {
        Self {
            status: Some(status),
            error: None,
        }
    }
    fn failure(error: impl Into<String>) -> Self {
        Self {
            status: None,
            error: Some(error.into()),
        }
    }
}

fn read_request(stream: &TcpStream) -> Result<WireRequest, ControllerIpcError> {
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(ControllerIpcError::Read)?;
    serde_json::from_str(&line).map_err(ControllerIpcError::Decode)
}
fn read_response(stream: &TcpStream) -> Result<WireResponse, ControllerIpcError> {
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(ControllerIpcError::Read)?;
    serde_json::from_str(&line).map_err(ControllerIpcError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_management_mapping_covers_lifecycle_and_excludes_other_commands() {
        let id = AdapterId::new("mock").unwrap();
        assert_eq!(
            legacy_management_command(&ControllerIpcCommand::Start { id: id.to_string() }).unwrap(),
            Some(ManagementCommand::Start(id.clone()))
        );
        assert_eq!(
            legacy_management_command(&ControllerIpcCommand::Stop { id: id.to_string() }).unwrap(),
            Some(ManagementCommand::Stop(id.clone()))
        );
        assert_eq!(
            legacy_management_command(&ControllerIpcCommand::Restart { id: id.to_string() })
                .unwrap(),
            Some(ManagementCommand::Restart(id.clone()))
        );
        assert_eq!(
            legacy_management_command(&ControllerIpcCommand::Diagnostics { id: id.to_string() })
                .unwrap(),
            Some(ManagementCommand::Diagnostics(id))
        );
        assert_eq!(
            legacy_management_command(&ControllerIpcCommand::Status { id: "mock".into() }).unwrap(),
            None
        );
    }

    #[test]
    fn typed_management_mapping_covers_all_supported_operations() {
        let id = AdapterId::new("mock").unwrap();
        assert_eq!(
            typed_management_command(&ControllerManagementRequest::Start { id: id.to_string() })
                .unwrap(),
            ManagementCommand::Start(id.clone())
        );
        assert_eq!(
            typed_management_command(&ControllerManagementRequest::Stop { id: id.to_string() })
                .unwrap(),
            ManagementCommand::Stop(id.clone())
        );
        assert_eq!(
            typed_management_command(&ControllerManagementRequest::Restart { id: id.to_string() })
                .unwrap(),
            ManagementCommand::Restart(id.clone())
        );
        assert_eq!(
            typed_management_command(&ControllerManagementRequest::Diagnostics {
                id: id.to_string()
            })
            .unwrap(),
            ManagementCommand::Diagnostics(id)
        );
    }

    #[test]
    fn peer_disconnect_classifies_all_socket_close_variants() {
        assert!(is_peer_disconnect(io::ErrorKind::BrokenPipe));
        assert!(is_peer_disconnect(io::ErrorKind::ConnectionReset));
        assert!(is_peer_disconnect(io::ErrorKind::ConnectionAborted));
        assert!(is_peer_disconnect(io::ErrorKind::NotConnected));
        assert!(!is_peer_disconnect(io::ErrorKind::WouldBlock));
    }
}

#[derive(Debug)]
pub enum ControllerManagementClientError {
    Authentication,
    Connection(ControllerIpcError),
    Protocol(ControllerIpcError),
    AdapterNotFound(String),
    Lifecycle(ControllerIpcError),
    UnexpectedStatus(ControllerIpcStatus),
    UnexpectedResponse(ControllerManagementResponse),
}

impl ControllerManagementClientError {
    fn from_ipc(error: ControllerIpcError) -> Self {
        match error {
            ControllerIpcError::Connect(error) => {
                Self::Connection(ControllerIpcError::Connect(error))
            }
            ControllerIpcError::Encode(error) => Self::Protocol(ControllerIpcError::Encode(error)),
            ControllerIpcError::Decode(error) => Self::Protocol(ControllerIpcError::Decode(error)),
            ControllerIpcError::Remote(message) if message == "authentication failed" => {
                Self::Authentication
            }
            ControllerIpcError::Remote(message) => match message.strip_prefix("unknown adapter ") {
                Some(id) => Self::AdapterNotFound(id.to_owned()),
                None => Self::Lifecycle(ControllerIpcError::Remote(message)),
            },
            error => Self::Lifecycle(error),
        }
    }
}

impl fmt::Display for ControllerManagementClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authentication => formatter.write_str("controller authentication failed"),
            Self::Connection(error) | Self::Protocol(error) | Self::Lifecycle(error) => {
                error.fmt(formatter)
            }
            Self::AdapterNotFound(id) => write!(formatter, "unknown adapter {id}"),
            Self::UnexpectedStatus(status) => {
                write!(formatter, "unexpected controller status: {status:?}")
            }
            Self::UnexpectedResponse(response) => {
                write!(
                    formatter,
                    "unexpected controller management response: {response:?}"
                )
            }
        }
    }
}

impl Error for ControllerManagementClientError {}

#[derive(Debug)]
pub enum ControllerIpcError {
    Accept(io::Error),
    Connect(io::Error),
    Read(io::Error),
    Write(io::Error),
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    InvalidId(String),
    UnexpectedStatus(String),
    Controller(crate::ControllerError),
    Remote(String),
}
impl fmt::Display for ControllerIpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accept(e) | Self::Connect(e) | Self::Read(e) | Self::Write(e) => e.fmt(f),
            Self::Encode(e) | Self::Decode(e) => e.fmt(f),
            Self::InvalidId(e) | Self::UnexpectedStatus(e) | Self::Remote(e) => f.write_str(e),
            Self::Controller(e) => e.fmt(f),
        }
    }
}
impl Error for ControllerIpcError {}

#[derive(Debug)]
pub enum LocalControllerError {
    Read(io::Error),
    Decode(serde_json::Error),
    NonLoopbackEndpoint,
    Ipc(ControllerIpcError),
    Management(ControllerManagementClientError),
}

impl fmt::Display for LocalControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::Decode(error) => write!(formatter, "invalid local controller endpoint: {error}"),
            Self::NonLoopbackEndpoint => {
                formatter.write_str("local controller endpoint is not loopback")
            }
            Self::Ipc(error) => error.fmt(formatter),
            Self::Management(error) => error.fmt(formatter),
        }
    }
}

impl Error for LocalControllerError {}
