use std::{
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

use crate::AdapterId;

/// Static local metadata for one installed adapter.
///
/// Runtime handshake data, especially capabilities, remains authoritative over this file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterManifest {
    pub id: AdapterId,
    pub name: String,
    pub version: String,
    pub protocol_version: u32,
    pub executable: ExecutablePath,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub author: Option<String>,
}

impl AdapterManifest {
    pub fn from_json(source: &str) -> Result<Self, ManifestError> {
        let raw: RawManifest = serde_json::from_str(source).map_err(ManifestError::Parse)?;
        let executable = ExecutablePath::new(raw.executable)?;
        Ok(Self {
            id: raw.id,
            name: raw.name,
            version: raw.version,
            protocol_version: raw.protocol_version,
            executable,
            description: raw.description,
            homepage: raw.homepage,
            author: raw.author,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutablePath(PathBuf);

impl ExecutablePath {
    pub fn new(value: impl Into<PathBuf>) -> Result<Self, ManifestError> {
        let value = value.into();
        let is_safe = !value.as_os_str().is_empty()
            && !value.is_absolute()
            && value
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !is_safe {
            return Err(ManifestError::InvalidExecutablePath(value));
        }
        Ok(Self(value))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug)]
pub enum ManifestError {
    Parse(serde_json::Error),
    InvalidExecutablePath(PathBuf),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "invalid adapter manifest: {error}"),
            Self::InvalidExecutablePath(path) => write!(
                formatter,
                "adapter executable must be a non-empty relative path below its adapter directory: {}",
                path.display()
            ),
        }
    }
}

impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::InvalidExecutablePath(_) => None,
        }
    }
}

#[derive(Deserialize)]
struct RawManifest {
    id: AdapterId,
    name: String,
    version: String,
    protocol_version: u32,
    executable: PathBuf,
    description: Option<String>,
    homepage: Option<String>,
    author: Option<String>,
}
