use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::{AdapterId, AdapterManifest, ManifestError, PROTOCOL_VERSION};

pub const MANIFEST_FILE_NAME: &str = "adapter.json";

/// Filesystem root containing one directory per locally installed adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalAdapterRoot {
    path: PathBuf,
}

impl LocalAdapterRoot {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn discover(&self) -> Result<Vec<DiscoveredAdapter>, DiscoveryError> {
        let mut candidates = Vec::new();
        for entry in fs::read_dir(&self.path).map_err(DiscoveryError::ReadRoot)? {
            let entry = entry.map_err(DiscoveryError::ReadRoot)?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }
            let file_type = entry.file_type().map_err(DiscoveryError::ReadRoot)?;
            if file_type.is_dir() {
                candidates.push(entry.path());
            }
        }
        candidates.sort();

        let mut parsed = Vec::new();
        let mut ids: BTreeMap<AdapterId, usize> = BTreeMap::new();
        for adapter_dir in candidates {
            let manifest_path = adapter_dir.join(MANIFEST_FILE_NAME);
            let source = match fs::read_to_string(&manifest_path) {
                Ok(source) => source,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    parsed.push(ParsedDiscovery::invalid(
                        adapter_dir,
                        manifest_path,
                        format!("missing {MANIFEST_FILE_NAME}"),
                    ));
                    continue;
                }
                Err(error) => return Err(DiscoveryError::ReadManifest(manifest_path, error)),
            };

            match AdapterManifest::from_json(&source) {
                Ok(manifest) => {
                    *ids.entry(manifest.id.clone()).or_default() += 1;
                    parsed.push(ParsedDiscovery::manifest(
                        adapter_dir,
                        manifest_path,
                        manifest,
                    ));
                }
                Err(error) => {
                    parsed.push(ParsedDiscovery::invalid(
                        adapter_dir,
                        manifest_path,
                        error.to_string(),
                    ));
                }
            }
        }

        let duplicate_ids: BTreeSet<_> = ids
            .into_iter()
            .filter_map(|(id, count)| (count > 1).then_some(id))
            .collect();

        parsed
            .into_iter()
            .map(|entry| self.classify(entry, &duplicate_ids))
            .collect()
    }

    fn classify(
        &self,
        entry: ParsedDiscovery,
        duplicate_ids: &BTreeSet<AdapterId>,
    ) -> Result<DiscoveredAdapter, DiscoveryError> {
        let Some(manifest) = entry.manifest else {
            return Ok(DiscoveredAdapter::new(
                entry.adapter_dir,
                entry.manifest_path,
                AdapterClassification::InvalidManifest,
                None,
                None,
                entry.error,
            ));
        };

        if duplicate_ids.contains(&manifest.id) {
            return Ok(DiscoveredAdapter::new(
                entry.adapter_dir,
                entry.manifest_path,
                AdapterClassification::InvalidManifest,
                Some(manifest),
                None,
                Some("duplicate adapter id".to_owned()),
            ));
        }

        if manifest.protocol_version != PROTOCOL_VERSION {
            return Ok(DiscoveredAdapter::new(
                entry.adapter_dir,
                entry.manifest_path,
                AdapterClassification::UnsupportedProtocol,
                Some(manifest),
                None,
                Some("unsupported protocol version".to_owned()),
            ));
        }

        let executable = entry.adapter_dir.join(manifest.executable.as_path());
        if !is_executable_file(&executable) {
            return Ok(DiscoveredAdapter::new(
                entry.adapter_dir,
                entry.manifest_path,
                AdapterClassification::MissingExecutable,
                Some(manifest),
                None,
                Some("executable not found".to_owned()),
            ));
        }

        let adapter_dir = fs::canonicalize(&entry.adapter_dir)
            .map_err(|error| DiscoveryError::ResolveExecutable(entry.adapter_dir.clone(), error))?;
        let resolved_executable = fs::canonicalize(&executable)
            .map_err(|error| DiscoveryError::ResolveExecutable(executable.clone(), error))?;

        if !resolved_executable.starts_with(&adapter_dir) {
            return Ok(DiscoveredAdapter::new(
                entry.adapter_dir,
                entry.manifest_path,
                AdapterClassification::InvalidManifest,
                Some(manifest),
                None,
                Some(ManifestError::InvalidExecutablePath(resolved_executable).to_string()),
            ));
        }

        Ok(DiscoveredAdapter::new(
            entry.adapter_dir,
            entry.manifest_path,
            AdapterClassification::Valid,
            Some(manifest),
            Some(resolved_executable),
            None,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterClassification {
    Valid,
    InvalidManifest,
    MissingExecutable,
    UnsupportedProtocol,
}

/// Discovery result for one directory under a [`LocalAdapterRoot`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredAdapter {
    adapter_dir: PathBuf,
    manifest_path: PathBuf,
    classification: AdapterClassification,
    manifest: Option<AdapterManifest>,
    resolved_executable: Option<PathBuf>,
    error: Option<String>,
}

impl DiscoveredAdapter {
    fn new(
        adapter_dir: PathBuf,
        manifest_path: PathBuf,
        classification: AdapterClassification,
        manifest: Option<AdapterManifest>,
        resolved_executable: Option<PathBuf>,
        error: Option<String>,
    ) -> Self {
        Self {
            adapter_dir,
            manifest_path,
            classification,
            manifest,
            resolved_executable,
            error,
        }
    }

    pub fn adapter_dir(&self) -> &Path {
        &self.adapter_dir
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn classification(&self) -> AdapterClassification {
        self.classification
    }

    pub fn manifest(&self) -> Option<&AdapterManifest> {
        self.manifest.as_ref()
    }

    pub fn resolved_executable(&self) -> Option<&Path> {
        self.resolved_executable.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Debug)]
pub enum DiscoveryError {
    ReadRoot(io::Error),
    ReadManifest(PathBuf, io::Error),
    ResolveExecutable(PathBuf, io::Error),
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadRoot(error) => write!(formatter, "failed to read adapter root: {error}"),
            Self::ReadManifest(path, error) => {
                write!(
                    formatter,
                    "failed to read manifest {}: {error}",
                    path.display()
                )
            }
            Self::ResolveExecutable(path, error) => {
                write!(formatter, "failed to resolve {}: {error}", path.display())
            }
        }
    }
}

impl Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReadRoot(error)
            | Self::ReadManifest(_, error)
            | Self::ResolveExecutable(_, error) => Some(error),
        }
    }
}

struct ParsedDiscovery {
    adapter_dir: PathBuf,
    manifest_path: PathBuf,
    manifest: Option<AdapterManifest>,
    error: Option<String>,
}

impl ParsedDiscovery {
    fn manifest(adapter_dir: PathBuf, manifest_path: PathBuf, manifest: AdapterManifest) -> Self {
        Self {
            adapter_dir,
            manifest_path,
            manifest: Some(manifest),
            error: None,
        }
    }

    fn invalid(adapter_dir: PathBuf, manifest_path: PathBuf, error: String) -> Self {
        Self {
            adapter_dir,
            manifest_path,
            manifest: None,
            error: Some(error),
        }
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
