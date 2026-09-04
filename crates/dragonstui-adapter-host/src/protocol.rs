use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::Value;

/// The only protocol version supported by this host foundation.
pub const PROTOCOL_VERSION: u32 = 1;

/// A validated local adapter identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AdapterId(String);

impl AdapterId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_segment(&value, "adapter id")?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AdapterId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// An extensible, dot-separated capability identifier such as `containers.logs`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Capability(String);

impl Capability {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        if value
            .split('.')
            .any(|segment| validate_segment(segment, "capability").is_err())
        {
            return Err(IdentifierError::new("capability", value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A producer-owned, stable identity for one adapter-declared action.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActionId(String);

impl ActionId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        if value
            .split('.')
            .any(|segment| validate_segment(segment, "action id").is_err())
        {
            return Err(IdentifierError::new("action id", value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Correlates a request with a response or error envelope.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(String);

impl RequestId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':')
            });
        if !valid {
            return Err(IdentifierError::new("request id", value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A deterministic validation failure for a protocol identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentifierError {
    kind: &'static str,
    value: String,
}

impl IdentifierError {
    fn new(kind: &'static str, value: String) -> Self {
        Self { kind, value }
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid {}: {:?}", self.kind, self.value)
    }
}

impl Error for IdentifierError {}

fn validate_segment(value: &str, kind: &'static str) -> Result<(), IdentifierError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'_' | b'-'))
        });
    if valid {
        Ok(())
    } else {
        Err(IdentifierError::new(kind, value.to_owned()))
    }
}

macro_rules! string_identifier_serde {
    ($type:ty) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(D::Error::custom)
            }
        }
    };
}

string_identifier_serde!(AdapterId);
string_identifier_serde!(Capability);
string_identifier_serde!(ActionId);
string_identifier_serde!(RequestId);

/// Host-to-adapter startup greeting.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    pub host_version: String,
}

/// Adapter identity and runtime-authoritative capabilities.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub protocol: u32,
    pub id: AdapterId,
    pub version: String,
    pub capabilities: Vec<Capability>,
    /// Optional producer-declared actions. Legacy adapters decode with none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<AdapterAction>,
}

/// Metadata describing one generic action that an adapter explicitly exposes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterAction {
    pub id: ActionId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Existing capability RPC operation used to execute this declared action.
    pub operation: Capability,
}

/// Generic capability operation sent by the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub protocol: u32,
    pub id: RequestId,
    pub operation: Capability,
    /// Optional producer-declared action identity for an action invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionId>,
    pub payload: Value,
}

/// Successful correlated adapter result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub protocol: u32,
    pub id: RequestId,
    pub payload: Value,
}

/// Correlated or uncorrelated adapter failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub protocol: u32,
    pub id: Option<RequestId>,
    pub code: String,
    pub message: String,
}

/// Producer-declared, capability-neutral observability metadata for an event.
///
/// This describes an observation's data rather than a future UI. The host does
/// not infer this classification from `stream`, `kind`, or `payload`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Observation {
    /// A human-readable record suitable for a future log projection.
    Log {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<ObservationSeverity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp_millis: Option<u64>,
    },
    /// One finite JSON numeric sample for a named series.
    Metric {
        name: String,
        value: serde_json::Number,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp_millis: Option<u64>,
    },
    /// One generic entity/check state observation.
    Status {
        entity: String,
        check: String,
        status: ObservationStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp_millis: Option<u64>,
    },
    /// A chronological, human-readable observation without product semantics.
    Event {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp_millis: Option<u64>,
    },
    /// A generic error with optional producer grouping and textual stack lines.
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        stack: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp_millis: Option<u64>,
    },
}

impl Observation {
    /// Returns the explicit semantic class used by future projections.
    pub const fn kind(&self) -> ObservationKind {
        match self {
            Self::Log { .. } => ObservationKind::Log,
            Self::Metric { .. } => ObservationKind::Metric,
            Self::Status { .. } => ObservationKind::Status,
            Self::Event { .. } => ObservationKind::Event,
            Self::Error { .. } => ObservationKind::Error,
        }
    }
}

/// Stable category for routing an explicitly classified observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationKind {
    Log,
    Metric,
    Status,
    Event,
    Error,
}

impl ObservationKind {
    /// Stable lower-case label for a generic observation projection.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Metric => "metric",
            Self::Status => "status",
            Self::Event => "event",
            Self::Error => "error",
        }
    }
}

/// Optional producer-declared severity for a log observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSeverity {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// Generic state carried by a status observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStatus {
    Ok,
    Warning,
    Error,
    Unknown,
}

/// Generic adapter-originated event.
///
/// `observation` is additive. Events emitted by existing adapters decode with
/// `None`, meaning Generic/Unclassified, while preserving all original fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub protocol: u32,
    pub stream: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<Observation>,
    pub payload: Value,
}

/// Host request for graceful adapter shutdown.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Shutdown {
    pub protocol: u32,
}

/// Adapter acknowledgement of a graceful shutdown request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShutdownAck {
    pub protocol: u32,
}

/// All newline-delimited JSON envelopes understood by Protocol v1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolMessage {
    Hello(Hello),
    AdapterInfo(AdapterInfo),
    Request(Request),
    Response(Response),
    Error(ErrorMessage),
    Event(Event),
    Shutdown(Shutdown),
    ShutdownAck(ShutdownAck),
}

impl ProtocolMessage {
    pub fn protocol(&self) -> u32 {
        match self {
            Self::Hello(message) => message.protocol,
            Self::AdapterInfo(message) => message.protocol,
            Self::Request(message) => message.protocol,
            Self::Response(message) => message.protocol,
            Self::Error(message) => message.protocol,
            Self::Event(message) => message.protocol,
            Self::Shutdown(message) => message.protocol,
            Self::ShutdownAck(message) => message.protocol,
        }
    }
}
