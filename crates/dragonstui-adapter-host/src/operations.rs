use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::Value;

use crate::{ActionId, AdapterId, IdentifierError};

/// Controller-owned stable identity for one generic action operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(String);

impl OperationId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        ActionId::new(value.clone()).map(|_| Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for OperationId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OperationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// The authoritative lifecycle of one adapter action invocation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OperationState {
    Pending,
    Running,
    Succeeded { payload: Value },
    Failed { code: String, message: String },
}

impl OperationState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded { .. } | Self::Failed { .. })
    }
}

/// A bounded controller-owned projection of an adapter action operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterOperation {
    pub id: OperationId,
    pub adapter_id: AdapterId,
    pub action_id: ActionId,
    pub action_label: String,
    pub state: OperationState,
}
