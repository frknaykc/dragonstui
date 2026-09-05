use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{
    ActionId, AdapterAction, AdapterId, AdapterInfo, AdapterManifest, AdapterProcess,
    AdapterProcessConfig, Capability, Hello, PROTOCOL_VERSION, ProcessError, ProcessStatus,
    ProtocolMessage, Request, RequestId, SessionClose, SessionId, SessionInput, SessionOpen,
    SessionResize,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 128;
const DEFAULT_RESPONSE_QUEUE_CAPACITY: usize = 128;
const DEFAULT_SESSION_EVENT_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterRuntimeConfig {
    process: AdapterProcessConfig,
    handshake_timeout: Duration,
    event_queue_capacity: usize,
    response_queue_capacity: usize,
}

impl AdapterRuntimeConfig {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            process: AdapterProcessConfig::new(executable),
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
            event_queue_capacity: DEFAULT_EVENT_QUEUE_CAPACITY,
            response_queue_capacity: DEFAULT_RESPONSE_QUEUE_CAPACITY,
        }
    }

    pub fn arg(mut self, value: impl Into<String>) -> Self {
        self.process = self.process.arg(value);
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.process = self.process.env(key, value);
        self
    }

    pub fn current_dir(mut self, value: impl Into<PathBuf>) -> Self {
        self.process = self.process.current_dir(value);
        self
    }

    pub fn handshake_timeout(mut self, value: Duration) -> Self {
        self.handshake_timeout = value;
        self
    }

    pub fn stderr_tail_lines(mut self, value: usize) -> Self {
        self.process = self.process.stderr_tail_lines(value);
        self
    }

    /// Bounds decoded protocol ingress and applies child-pipe backpressure when full.
    pub fn ingress_queue_capacity(mut self, value: usize) -> Self {
        self.process = self.process.stdout_queue_capacity(value);
        self
    }

    pub fn event_queue_capacity(mut self, value: usize) -> Self {
        self.event_queue_capacity = value;
        self
    }

    pub fn response_queue_capacity(mut self, value: usize) -> Self {
        self.response_queue_capacity = value;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterState {
    Discovered,
    Starting,
    Handshaking,
    Running,
    Stopping,
    Incompatible,
    Failed,
    Crashed,
    Stopped,
}

#[derive(Debug)]
pub struct AdapterRuntime {
    manifest: AdapterManifest,
    info: AdapterInfo,
    process: AdapterProcess,
    state: AdapterState,
    state_history: Vec<AdapterState>,
    started_at: Instant,
    last_error: Option<String>,
    next_request: u64,
    pending: BTreeMap<RequestId, PendingRequest>,
    session_opened: BTreeMap<RequestId, SessionId>,
    active_sessions: BTreeSet<SessionId>,
    outcomes: BTreeMap<RequestId, RpcOutcome>,
    request_failures: BTreeMap<RequestId, RpcError>,
    unknown_responses: usize,
    events: BoundedQueue<AdapterEvent>,
    session_events: BoundedQueue<RuntimeSessionEvent>,
    response_queue_capacity: usize,
}

impl AdapterRuntime {
    pub fn start(
        manifest: AdapterManifest,
        config: AdapterRuntimeConfig,
    ) -> Result<Self, AdapterStartError> {
        let mut state_history = vec![AdapterState::Discovered, AdapterState::Starting];
        let mut process = AdapterProcess::start(config.process)
            .map_err(|error| AdapterStartError::Failed(error.to_string()))?;
        state_history.push(AdapterState::Handshaking);
        process
            .write_message(&ProtocolMessage::Hello(Hello {
                protocol: PROTOCOL_VERSION,
                host_version: env!("CARGO_PKG_VERSION").to_owned(),
            }))
            .map_err(|error| AdapterStartError::Failed(error.to_string()))?;

        let message = match process.read_stdout_message(config.handshake_timeout) {
            Ok(message) => message,
            Err(ProcessError::Timeout) => {
                return Err(AdapterStartError::Timeout);
            }
            Err(ProcessError::DecodeStdout(error)) => {
                return Err(AdapterStartError::Failed(error.to_string()));
            }
            Err(ProcessError::StdoutClosed) => {
                return Err(AdapterStartError::Crashed(
                    "adapter exited during handshake".to_owned(),
                ));
            }
            Err(error) => return Err(AdapterStartError::Failed(error.to_string())),
        };

        let ProtocolMessage::AdapterInfo(info) = message else {
            return Err(AdapterStartError::Failed(
                "expected adapter_info during handshake".to_owned(),
            ));
        };
        validate_info(&manifest, &info)?;
        state_history.push(AdapterState::Running);

        Ok(Self {
            manifest,
            info,
            process,
            state: AdapterState::Running,
            state_history,
            started_at: Instant::now(),
            last_error: None,
            next_request: 1,
            pending: BTreeMap::new(),
            session_opened: BTreeMap::new(),
            active_sessions: BTreeSet::new(),
            outcomes: BTreeMap::new(),
            request_failures: BTreeMap::new(),
            unknown_responses: 0,
            events: BoundedQueue::new(config.event_queue_capacity),
            session_events: BoundedQueue::new(DEFAULT_SESSION_EVENT_QUEUE_CAPACITY),
            response_queue_capacity: config.response_queue_capacity,
        })
    }

    pub fn adapter_id(&self) -> &AdapterId {
        &self.info.id
    }

    pub fn state(&self) -> AdapterState {
        self.state
    }

    pub fn state_history(&self) -> &[AdapterState] {
        &self.state_history
    }

    pub fn info(&self) -> &AdapterInfo {
        &self.info
    }

    pub fn capabilities(&self) -> &[Capability] {
        &self.info.capabilities
    }

    pub fn actions(&self) -> &[AdapterAction] {
        &self.info.actions
    }

    /// Returns provider-declared interactive session surfaces without inferring
    /// semantics from capability identifiers or output text.
    pub fn sessions(&self) -> &[crate::AdapterSession] {
        &self.info.sessions
    }

    pub fn pid(&self) -> u32 {
        self.process.pid()
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn stderr_tail(&self) -> String {
        self.process.stderr_tail()
    }

    pub fn stderr_dropped_lines(&self) -> usize {
        self.process.stderr_dropped_lines()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn stop(&mut self, graceful_timeout: Duration) -> Result<(), ProcessError> {
        self.state = AdapterState::Stopping;
        self.state_history.push(AdapterState::Stopping);
        let _ = self.process.stop(graceful_timeout, graceful_timeout)?;
        self.state = AdapterState::Stopped;
        self.state_history.push(AdapterState::Stopped);
        Ok(())
    }

    pub fn send_request(
        &mut self,
        operation: Capability,
        payload: Value,
        timeout: Duration,
    ) -> Result<RequestId, RpcError> {
        self.send_request_with_action(operation, None, payload, timeout)
    }

    pub fn send_action_request(
        &mut self,
        action: &AdapterAction,
        payload: Value,
        timeout: Duration,
    ) -> Result<RequestId, RpcError> {
        self.send_request_with_action(
            action.operation.clone(),
            Some(action.id.clone()),
            payload,
            timeout,
        )
    }

    /// Sends an explicit typed session-open request.
    pub fn open_session(
        &mut self,
        capability: Capability,
        rows: u16,
        columns: u16,
        timeout: Duration,
    ) -> Result<RequestId, RpcError> {
        if self.state != AdapterState::Running {
            return Err(RpcError::Crashed);
        }
        let id = RequestId::new(format!("{}:{}", self.info.id, self.next_request))
            .map_err(|error| RpcError::Failed(error.to_string()))?;
        self.next_request += 1;
        self.process
            .write_message(&ProtocolMessage::SessionOpen(SessionOpen {
                protocol: PROTOCOL_VERSION,
                id: id.clone(),
                capability,
                rows,
                columns,
            }))
            .map_err(|error| {
                self.mark_crashed(error.to_string());
                RpcError::Crashed
            })?;
        self.pending.insert(
            id.clone(),
            PendingRequest {
                deadline: Instant::now() + timeout,
            },
        );
        Ok(id)
    }

    /// Waits for the provider's typed acknowledgement of a session-open request.
    pub fn wait_session_open(
        &mut self,
        id: &RequestId,
        timeout: Duration,
    ) -> Result<SessionId, RpcError> {
        let deadline = Instant::now() + timeout;
        loop {
            self.expire_pending();
            if let Some(session_id) = self.session_opened.remove(id) {
                return Ok(session_id);
            }
            if let Some(error) = self.request_failures.remove(id) {
                return Err(error);
            }
            if self.outcomes.remove(id).is_some() || !self.pending.contains_key(id) {
                return Err(RpcError::Failed(
                    "session open was not acknowledged".to_owned(),
                ));
            }
            if Instant::now() >= deadline {
                self.pending.remove(id);
                return Err(RpcError::Timeout);
            }
            match self.pump(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20)),
            ) {
                Ok(_) | Err(RpcError::Timeout) => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Forwards input only to a session previously opened by this runtime.
    pub fn send_session_input(
        &mut self,
        session_id: &SessionId,
        data: String,
    ) -> Result<(), RpcError> {
        if !self.active_sessions.contains(session_id) {
            return Err(RpcError::Failed("unknown session".to_owned()));
        }
        self.process
            .write_message(&ProtocolMessage::SessionInput(SessionInput {
                protocol: PROTOCOL_VERSION,
                session_id: session_id.clone(),
                data,
            }))
            .map_err(|error| {
                self.mark_crashed(error.to_string());
                RpcError::Crashed
            })
    }

    /// Forwards a geometry change only to a session opened by this runtime.
    pub fn resize_session(
        &mut self,
        session_id: &SessionId,
        rows: u16,
        columns: u16,
    ) -> Result<(), RpcError> {
        if !self.active_sessions.contains(session_id) {
            return Err(RpcError::Failed("unknown session".to_owned()));
        }
        self.process
            .write_message(&ProtocolMessage::SessionResize(SessionResize {
                protocol: PROTOCOL_VERSION,
                session_id: session_id.clone(),
                rows,
                columns,
            }))
            .map_err(|error| {
                self.mark_crashed(error.to_string());
                RpcError::Crashed
            })
    }

    /// Requests provider-owned cleanup; the session remains active until its
    /// typed exit record is received.
    pub fn close_session(&mut self, session_id: &SessionId) -> Result<(), RpcError> {
        if !self.active_sessions.contains(session_id) {
            return Err(RpcError::Failed("unknown session".to_owned()));
        }
        self.process
            .write_message(&ProtocolMessage::SessionClose(SessionClose {
                protocol: PROTOCOL_VERSION,
                session_id: session_id.clone(),
            }))
            .map_err(|error| {
                self.mark_crashed(error.to_string());
                RpcError::Crashed
            })
    }

    fn send_request_with_action(
        &mut self,
        operation: Capability,
        action: Option<ActionId>,
        payload: Value,
        timeout: Duration,
    ) -> Result<RequestId, RpcError> {
        if self.state != AdapterState::Running {
            return Err(RpcError::Crashed);
        }
        let id = RequestId::new(format!("{}:{}", self.info.id, self.next_request))
            .map_err(|error| RpcError::Failed(error.to_string()))?;
        self.next_request += 1;
        self.process
            .write_message(&ProtocolMessage::Request(Request {
                protocol: PROTOCOL_VERSION,
                id: id.clone(),
                operation,
                action,
                payload,
            }))
            .map_err(|error| {
                self.mark_crashed(error.to_string());
                RpcError::Crashed
            })?;
        self.pending.insert(
            id.clone(),
            PendingRequest {
                deadline: Instant::now() + timeout,
            },
        );
        Ok(id)
    }

    pub fn wait_response(
        &mut self,
        id: &RequestId,
        timeout: Duration,
    ) -> Result<RpcOutcome, RpcError> {
        let deadline = Instant::now() + timeout;
        loop {
            self.expire_pending();
            if let Some(outcome) = self.outcomes.remove(id) {
                return Ok(outcome);
            }
            if let Some(error) = self.request_failures.remove(id) {
                return Err(error);
            }
            if !self.pending.contains_key(id) {
                return Err(RpcError::Timeout);
            }
            if Instant::now() >= deadline {
                self.pending.remove(id);
                return Err(RpcError::Timeout);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let poll_for = remaining.min(Duration::from_millis(20));
            match self.pump(poll_for) {
                Ok(_) => {}
                Err(RpcError::Timeout) => {}
                Err(error @ RpcError::Crashed) => return Err(error),
                Err(error) => return Err(error),
            }
        }
    }

    pub fn pump(&mut self, timeout: Duration) -> Result<bool, RpcError> {
        self.expire_pending();
        if self.response_queue_len() >= self.response_queue_capacity {
            return Err(RpcError::Backpressure);
        }
        let message = match self.process.read_stdout_message(timeout) {
            Ok(message) => message,
            Err(ProcessError::Timeout) => return Ok(false),
            Err(ProcessError::StdoutClosed) => {
                self.mark_crashed("adapter stdout closed");
                return Err(RpcError::Crashed);
            }
            Err(ProcessError::DecodeStdout(error)) => {
                self.last_error = Some(error.to_string());
                return Err(RpcError::Failed(error.to_string()));
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
                return Err(RpcError::Failed(error.to_string()));
            }
        };
        self.handle_message(message);
        Ok(true)
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn unknown_response_count(&self) -> usize {
        self.unknown_responses
    }

    /// Retrieves a completed outcome without waiting for more process output.
    pub fn take_outcome(&mut self, id: &RequestId) -> Option<RpcOutcome> {
        self.outcomes.remove(id)
    }

    /// Retrieves a terminal transport failure without waiting for more output.
    pub fn take_request_failure(&mut self, id: &RequestId) -> Option<RpcError> {
        self.request_failures.remove(id)
    }

    pub fn pop_event(&mut self) -> Option<AdapterEvent> {
        self.events.pop_front()
    }

    pub(crate) fn pop_session_event(&mut self) -> Option<RuntimeSessionEvent> {
        self.session_events.pop_front()
    }

    pub fn event_queue_capacity(&self) -> usize {
        self.events.capacity()
    }

    pub fn event_queue_len(&self) -> usize {
        self.events.len()
    }

    pub fn dropped_event_count(&self) -> usize {
        self.events.dropped()
    }

    pub fn response_queue_capacity_value(&self) -> usize {
        self.response_queue_capacity
    }

    pub fn response_queue_capacity(&self) -> usize {
        self.response_queue_capacity
    }

    pub fn response_queue_len(&self) -> usize {
        self.outcomes.len() + self.request_failures.len()
    }

    pub(crate) fn mark_crashed(&mut self, error: impl Into<String>) {
        self.state = AdapterState::Crashed;
        self.state_history.push(AdapterState::Crashed);
        self.last_error = Some(error.into());
        for id in std::mem::take(&mut self.pending).into_keys() {
            self.request_failures.insert(id, RpcError::Crashed);
        }
        self.pending.clear();
    }

    pub(crate) fn process_status(&mut self) -> ProcessStatus {
        self.process.status()
    }

    pub(crate) fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn handle_message(&mut self, message: ProtocolMessage) {
        match message {
            ProtocolMessage::SessionOpened(opened) => {
                if self.pending.remove(&opened.id).is_some() {
                    self.active_sessions.insert(opened.session_id.clone());
                    self.session_opened.insert(opened.id, opened.session_id);
                } else {
                    self.unknown_responses += 1;
                }
            }
            ProtocolMessage::SessionOutput(output) => {
                if self.active_sessions.contains(&output.session_id) {
                    self.session_events.push(RuntimeSessionEvent::Output {
                        session_id: output.session_id,
                        data: output.data,
                    });
                } else {
                    self.unknown_responses += 1;
                }
            }
            ProtocolMessage::SessionExit(exit) => {
                if self.active_sessions.remove(&exit.session_id) {
                    self.session_events.push(RuntimeSessionEvent::Exited {
                        session_id: exit.session_id,
                        exit_code: exit.exit_code,
                    });
                } else {
                    self.unknown_responses += 1;
                }
            }
            ProtocolMessage::Response(response) => {
                if self.pending.remove(&response.id).is_some() {
                    self.outcomes
                        .insert(response.id, RpcOutcome::Response(response.payload));
                } else {
                    self.unknown_responses += 1;
                }
            }
            ProtocolMessage::Error(error) => {
                if let Some(id) = error.id {
                    if self.pending.remove(&id).is_some() {
                        self.outcomes.insert(
                            id,
                            RpcOutcome::AdapterError {
                                code: error.code,
                                message: error.message,
                            },
                        );
                    } else {
                        self.unknown_responses += 1;
                    }
                } else {
                    self.last_error = Some(format!("{}: {}", error.code, error.message));
                }
            }
            ProtocolMessage::Event(event) => {
                self.events.push(AdapterEvent {
                    adapter_id: self.info.id.clone(),
                    stream: event.stream,
                    kind: event.kind,
                    observation: event.observation,
                    payload: event.payload,
                });
            }
            _ => {}
        }
    }

    fn expire_pending(&mut self) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(id, pending)| (pending.deadline <= now).then_some(id.clone()))
            .collect();
        for id in expired {
            self.pending.remove(&id);
            self.request_failures.insert(id, RpcError::Timeout);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterEvent {
    pub adapter_id: AdapterId,
    pub stream: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<crate::Observation>,
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RpcOutcome {
    Response(Value),
    AdapterError { code: String, message: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RpcError {
    Timeout,
    Crashed,
    Backpressure,
    Failed(String),
}

impl fmt::Display for RpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(formatter, "adapter request timed out"),
            Self::Crashed => write!(formatter, "adapter process crashed"),
            Self::Backpressure => write!(formatter, "adapter response queue is full"),
            Self::Failed(message) => write!(formatter, "adapter request failed: {message}"),
        }
    }
}

impl Error for RpcError {}

#[derive(Clone, Debug)]
struct PendingRequest {
    deadline: Instant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeSessionEvent {
    Output {
        session_id: SessionId,
        data: String,
    },
    Exited {
        session_id: SessionId,
        exit_code: Option<i32>,
    },
}

#[derive(Clone, Debug)]
struct BoundedQueue<T> {
    items: VecDeque<T>,
    capacity: usize,
    dropped: usize,
}

impl<T> BoundedQueue<T> {
    fn new(capacity: usize) -> Self {
        Self {
            items: VecDeque::with_capacity(capacity),
            capacity,
            dropped: 0,
        }
    }

    fn push(&mut self, item: T) {
        if self.capacity == 0 {
            self.dropped += 1;
            return;
        }
        if self.items.len() == self.capacity {
            self.items.pop_front();
            self.dropped += 1;
        }
        self.items.push_back(item);
    }

    fn pop_front(&mut self) -> Option<T> {
        self.items.pop_front()
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn dropped(&self) -> usize {
        self.dropped
    }
}

#[derive(Debug)]
pub enum AdapterStartError {
    Incompatible(String),
    Failed(String),
    Crashed(String),
    Timeout,
}

impl fmt::Display for AdapterStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incompatible(message) => write!(formatter, "incompatible adapter: {message}"),
            Self::Failed(message) => write!(formatter, "adapter start failed: {message}"),
            Self::Crashed(message) => write!(formatter, "adapter crashed during start: {message}"),
            Self::Timeout => write!(formatter, "adapter handshake timed out"),
        }
    }
}

impl Error for AdapterStartError {}

fn validate_info(manifest: &AdapterManifest, info: &AdapterInfo) -> Result<(), AdapterStartError> {
    if info.protocol != PROTOCOL_VERSION {
        return Err(AdapterStartError::Incompatible(format!(
            "protocol {} is not supported",
            info.protocol
        )));
    }
    if info.id != manifest.id {
        return Err(AdapterStartError::Incompatible(format!(
            "adapter id {} does not match manifest id {}",
            info.id, manifest.id
        )));
    }
    if info.version.is_empty() {
        return Err(AdapterStartError::Incompatible(
            "adapter version must not be empty".to_owned(),
        ));
    }
    if info.capabilities.is_empty() {
        return Err(AdapterStartError::Incompatible(
            "adapter capabilities must not be empty".to_owned(),
        ));
    }
    let mut seen = BTreeSet::new();
    for capability in &info.capabilities {
        if !seen.insert(capability.clone()) {
            return Err(AdapterStartError::Incompatible(format!(
                "duplicate capability {capability}"
            )));
        }
    }
    Ok(())
}
