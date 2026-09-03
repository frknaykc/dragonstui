use std::{collections::BTreeSet, error::Error, fmt, fs, path::PathBuf};

use serde::Deserialize;

use crate::{AdapterId, ExecutablePath, ManifestError, PROTOCOL_VERSION};

/// A validated registry document for downloadable external adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Registry {
    adapters: Vec<AdapterEntry>,
}

impl Registry {
    /// Loads provider-neutral metadata from a local file or HTTPS URL before
    /// applying the same parsing and validation as `from_json`.
    pub fn load(source: &str) -> Result<Self, RegistryLoadError> {
        let body = if source.starts_with("https://") {
            ureq::get(source)
                .call()
                .map_err(|error| RegistryLoadError::Remote(Box::new(error)))?
                .into_string()
                .map_err(RegistryLoadError::Read)?
        } else {
            let path = source.strip_prefix("file://").unwrap_or(source);
            fs::read_to_string(path).map_err(RegistryLoadError::Read)?
        };
        Self::from_json(&body).map_err(RegistryLoadError::Registry)
    }

    pub fn from_json(source: &str) -> Result<Self, RegistryError> {
        let raw: RawRegistry = serde_json::from_str(source).map_err(RegistryError::Parse)?;
        let mut ids = BTreeSet::new();
        let mut adapters = Vec::with_capacity(raw.adapters.len());
        for entry in raw.adapters {
            let id = entry.id;
            if !ids.insert(id.clone()) {
                return Err(RegistryError::Validation(format!(
                    "duplicate adapter id: {id}"
                )));
            }
            let mut versions = BTreeSet::new();
            let mut releases = Vec::with_capacity(entry.releases.len());
            for release in entry.releases {
                validate_version(&release.version)?;
                if !versions.insert(release.version.clone()) {
                    return Err(RegistryError::Validation(format!(
                        "duplicate release version for {id}: {}",
                        release.version
                    )));
                }
                if release.protocol_version != PROTOCOL_VERSION {
                    return Err(RegistryError::Validation(format!(
                        "unsupported protocol release for {id}: {}",
                        release.protocol_version
                    )));
                }
                let mut platforms = BTreeSet::new();
                let mut artifacts = Vec::with_capacity(release.artifacts.len());
                for artifact in release.artifacts {
                    let platform = Platform::new(artifact.os, artifact.architecture)?;
                    if !platforms.insert(platform.clone()) {
                        return Err(RegistryError::Validation(format!(
                            "duplicate artifact platform for {id} {}: {platform}",
                            release.version
                        )));
                    }
                    let source = ArtifactSource::new(artifact.source)?;
                    let sha256 = Sha256Checksum::new(artifact.sha256)?;
                    let executable = artifact
                        .executable
                        .map(ExecutablePath::new)
                        .transpose()
                        .map_err(RegistryError::Manifest)?;
                    artifacts.push(AdapterArtifact {
                        platform,
                        source,
                        expected_size: artifact.size,
                        sha256,
                        executable,
                    });
                }
                releases.push(AdapterRelease {
                    version: release.version,
                    protocol_version: release.protocol_version,
                    artifacts,
                });
            }
            adapters.push(AdapterEntry {
                id,
                name: entry.name,
                description: entry.description,
                homepage: entry.homepage,
                releases,
            });
        }
        Ok(Self { adapters })
    }

    pub fn adapters(&self) -> &[AdapterEntry] {
        &self.adapters
    }

    pub fn adapter(&self, id: &str) -> Option<&AdapterEntry> {
        self.adapters.iter().find(|entry| entry.id.as_str() == id)
    }

    /// Case-insensitive substring search across adapter identity and user-visible metadata.
    pub fn search(&self, query: &str) -> Vec<&AdapterEntry> {
        let query = query.to_ascii_lowercase();
        self.adapters
            .iter()
            .filter(|entry| {
                entry.id.as_str().contains(&query)
                    || entry.name.to_ascii_lowercase().contains(&query)
                    || entry.description.as_deref().is_some_and(|description| {
                        description.to_ascii_lowercase().contains(&query)
                    })
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterEntry {
    pub id: AdapterId,
    pub name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub releases: Vec<AdapterRelease>,
}

impl AdapterEntry {
    pub fn release(&self, version: &str) -> Option<&AdapterRelease> {
        self.releases
            .iter()
            .find(|release| release.version == version)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterRelease {
    pub version: String,
    pub protocol_version: u32,
    pub artifacts: Vec<AdapterArtifact>,
}

impl AdapterRelease {
    pub fn artifact_for(&self, platform: &Platform) -> Option<&AdapterArtifact> {
        self.artifacts
            .iter()
            .find(|artifact| &artifact.platform == platform)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterArtifact {
    pub platform: Platform,
    pub source: ArtifactSource,
    pub expected_size: Option<u64>,
    pub sha256: Sha256Checksum,
    pub executable: Option<ExecutablePath>,
}

/// Exact normalized platform identity used for artifact selection.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Platform {
    os: String,
    architecture: String,
}

impl Platform {
    pub fn new(
        os: impl Into<String>,
        architecture: impl Into<String>,
    ) -> Result<Self, RegistryError> {
        let os = os.into();
        let architecture = architecture.into();
        if !matches!(os.as_str(), "macos" | "linux" | "windows") {
            return Err(RegistryError::Validation(format!(
                "unsupported platform os: {os}"
            )));
        }
        if !matches!(architecture.as_str(), "aarch64" | "x86_64") {
            return Err(RegistryError::Validation(format!(
                "unsupported platform architecture: {architecture}"
            )));
        }
        Ok(Self { os, architecture })
    }

    pub fn current() -> Result<Self, RegistryError> {
        let os = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            return Err(RegistryError::Validation(
                "unsupported host operating system".to_owned(),
            ));
        };
        let architecture = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else {
            return Err(RegistryError::Validation(
                "unsupported host architecture".to_owned(),
            ));
        };
        Self::new(os, architecture)
    }

    pub fn os(&self) -> &str {
        &self.os
    }

    pub fn architecture(&self) -> &str {
        &self.architecture
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.os, self.architecture)
    }
}

/// Generic registry artifact source. Installation transport is deliberately deferred to M36.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactSource(String);

impl ArtifactSource {
    pub fn new(value: impl Into<String>) -> Result<Self, RegistryError> {
        let value = value.into();
        let valid = value
            .strip_prefix("file://")
            .is_some_and(|path| !path.is_empty())
            || value.starts_with("https://");
        if !valid {
            return Err(RegistryError::Validation(
                "artifact source must be a non-empty file:// URL or https:// URL".to_owned(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated, lower-case, 64-character SHA-256 digest from registry metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sha256Checksum(String);

impl Sha256Checksum {
    pub fn new(value: impl Into<String>) -> Result<Self, RegistryError> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(RegistryError::Validation(
                "artifact sha256 must be a 64-character hexadecimal digest".to_owned(),
            ));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub enum RegistryError {
    Parse(serde_json::Error),
    Manifest(ManifestError),
    Validation(String),
}

#[derive(Debug)]
pub enum RegistryLoadError {
    Read(std::io::Error),
    Remote(Box<ureq::Error>),
    Registry(RegistryError),
}

impl fmt::Display for RegistryLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "could not read registry: {error}"),
            Self::Remote(error) => write!(formatter, "could not load registry: {error}"),
            Self::Registry(error) => error.fmt(formatter),
        }
    }
}

impl Error for RegistryLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::Remote(error) => Some(error),
            Self::Registry(error) => Some(error),
        }
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "invalid adapter registry: {error}"),
            Self::Manifest(error) => error.fmt(formatter),
            Self::Validation(message) => write!(formatter, "invalid adapter registry: {message}"),
        }
    }
}

impl Error for RegistryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Manifest(error) => Some(error),
            Self::Validation(_) => None,
        }
    }
}

fn validate_version(value: &str) -> Result<(), RegistryError> {
    let numeric = value.split_once('-').map_or(value, |(numeric, _)| numeric);
    if numeric.split('.').count() != 3
        || numeric.split('.').any(|part| part.parse::<u64>().is_err())
    {
        return Err(RegistryError::Validation(format!(
            "release version must use semantic major.minor.patch form: {value}"
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
struct RawRegistry {
    adapters: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    id: AdapterId,
    name: String,
    description: Option<String>,
    homepage: Option<String>,
    releases: Vec<RawRelease>,
}

#[derive(Deserialize)]
struct RawRelease {
    version: String,
    protocol_version: u32,
    artifacts: Vec<RawArtifact>,
}

#[derive(Deserialize)]
struct RawArtifact {
    os: String,
    architecture: String,
    source: String,
    size: Option<u64>,
    sha256: String,
    executable: Option<PathBuf>,
}
