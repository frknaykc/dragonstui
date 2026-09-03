use std::{
    error::Error,
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{AdapterId, AdapterRelease, ArtifactSource, Platform, Registry, Sha256Checksum};

const DEFAULT_MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
pub const INSTALL_METADATA_FILE_NAME: &str = "adapter-install.json";
static STAGING_NONCE: AtomicUsize = AtomicUsize::new(0);

/// A verified replacement that has not yet changed the installed adapter.
#[derive(Debug)]
pub struct PreparedUpdate {
    adapter_id: AdapterId,
    version: String,
    platform: Platform,
    target: PathBuf,
    staging: PathBuf,
    staging_root: PathBuf,
}

impl Drop for PreparedUpdate {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.staging);
        let _ = fs::remove_dir(&self.staging_root);
    }
}

/// Explicit, synchronous installer for a trusted registry selection.
///
/// Installation is filesystem-only: this type never starts an adapter process.
#[derive(Clone, Debug)]
pub struct AdapterInstaller {
    root: PathBuf,
    max_artifact_bytes: u64,
    registry_source: Option<String>,
}

impl AdapterInstaller {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_artifact_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
            registry_source: None,
        }
    }

    pub fn max_artifact_bytes(mut self, value: u64) -> Self {
        self.max_artifact_bytes = value;
        self
    }

    /// Records the registry document provenance in subsequent install metadata.
    pub fn registry_source(mut self, source: impl Into<String>) -> Self {
        self.registry_source = Some(source.into());
        self
    }

    pub fn install(
        &self,
        registry: &Registry,
        id: &AdapterId,
        requested_version: Option<&str>,
        platform: &Platform,
    ) -> Result<InstallReceipt, InstallError> {
        let entry = registry
            .adapter(id.as_str())
            .ok_or_else(|| InstallError::UnknownAdapter(id.clone()))?;
        let release = select_release(entry.releases.as_slice(), requested_version)?;
        let artifact = release
            .artifact_for(platform)
            .ok_or_else(|| InstallError::UnsupportedPlatform(platform.clone()))?;
        let executable = artifact.executable.as_ref().ok_or_else(|| {
            InstallError::InvalidLayout("artifact has no executable layout".to_owned())
        })?;

        fs::create_dir_all(&self.root).map_err(InstallError::CreateRoot)?;
        let target = self.root.join(id.as_str());
        if target.exists() {
            return Err(InstallError::AlreadyInstalled(id.clone()));
        }
        let staging = self.staging_directory(id)?;
        let result = self.install_staged(
            entry,
            release,
            artifact.source.clone(),
            artifact.expected_size,
            &artifact.sha256,
            &artifact.platform,
            executable.as_path(),
            &staging,
        );
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::remove_dir(self.root.join(".staging"));
        }
        result?;

        fs::rename(&staging, &target).map_err(InstallError::AtomicInstall)?;
        let _ = fs::remove_dir(self.root.join(".staging"));
        Ok(InstallReceipt {
            adapter_id: id.clone(),
            version: release.version.clone(),
            platform: platform.clone(),
            adapter_dir: target,
        })
    }

    /// Replaces an installed adapter only after the selected newer artifact has
    /// been staged and verified. This operation never starts a process.
    pub fn update(
        &self,
        registry: &Registry,
        id: &AdapterId,
        platform: &Platform,
    ) -> Result<InstallReceipt, InstallError> {
        self.commit_update(self.prepare_update(registry, id, platform)?)
    }

    /// Downloads and validates a newer release without changing the installed
    /// adapter directory or any controller-owned runtime.
    pub fn prepare_update(
        &self,
        registry: &Registry,
        id: &AdapterId,
        platform: &Platform,
    ) -> Result<PreparedUpdate, InstallError> {
        let entry = registry
            .adapter(id.as_str())
            .ok_or_else(|| InstallError::UnknownAdapter(id.clone()))?;
        let target = self.root.join(id.as_str());
        let installed = read_installed_release(&target, id)?;
        let installed_version = Version::parse(&installed)
            .map_err(|_| InstallError::InvalidInstalledVersion(installed.clone()))?;
        let release = entry
            .releases
            .iter()
            .filter_map(|release| {
                Version::parse(&release.version)
                    .ok()
                    .filter(|version| version > &installed_version)
                    .map(|version| (version, release))
            })
            .max_by(|left, right| left.0.cmp(&right.0))
            .map(|(_, release)| release)
            .ok_or_else(|| InstallError::NoUpdateAvailable(id.clone()))?;
        let artifact = release
            .artifact_for(platform)
            .ok_or_else(|| InstallError::UnsupportedPlatform(platform.clone()))?;
        let executable = artifact.executable.as_ref().ok_or_else(|| {
            InstallError::InvalidLayout("artifact has no executable layout".to_owned())
        })?;

        fs::create_dir_all(&self.root).map_err(InstallError::CreateRoot)?;
        let staging = self.staging_directory(id)?;
        let staged = self.install_staged(
            entry,
            release,
            artifact.source.clone(),
            artifact.expected_size,
            &artifact.sha256,
            &artifact.platform,
            executable.as_path(),
            &staging,
        );
        if let Err(error) = staged {
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::remove_dir(self.root.join(".staging"));
            return Err(error);
        }
        Ok(PreparedUpdate {
            adapter_id: id.clone(),
            version: release.version.clone(),
            platform: platform.clone(),
            target,
            staging,
            staging_root: self.root.join(".staging"),
        })
    }

    /// Atomically commits a previously verified replacement, restoring the
    /// previous directory if the second rename fails.
    pub fn commit_update(&self, prepared: PreparedUpdate) -> Result<InstallReceipt, InstallError> {
        let backup = self.staging_directory(&prepared.adapter_id)?;
        fs::remove_dir(&backup).map_err(InstallError::AtomicInstall)?;
        fs::rename(&prepared.target, &backup).map_err(InstallError::AtomicInstall)?;
        if let Err(error) = fs::rename(&prepared.staging, &prepared.target) {
            let _ = fs::rename(&backup, &prepared.target);
            return Err(InstallError::AtomicInstall(error));
        }
        let _ = fs::remove_dir_all(&backup);
        let _ = fs::remove_dir(self.root.join(".staging"));
        Ok(InstallReceipt {
            adapter_id: prepared.adapter_id.clone(),
            version: prepared.version.clone(),
            platform: prepared.platform.clone(),
            adapter_dir: prepared.target.clone(),
        })
    }

    /// Removes exactly one direct child of the configured adapter root.
    /// Runtime owners must stop an adapter before invoking this filesystem-only operation.
    pub fn remove(&self, id: &AdapterId) -> Result<(), InstallError> {
        let target = self.root.join(id.as_str());
        let metadata = fs::symlink_metadata(&target).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => InstallError::NotInstalled(id.clone()),
            _ => InstallError::Remove(error),
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(InstallError::UnsafeRemoval(target));
        }
        fs::remove_dir_all(target).map_err(InstallError::Remove)
    }

    pub fn install_metadata(&self, id: &AdapterId) -> Result<InstallMetadata, InstallError> {
        let path = self.root.join(id.as_str()).join(INSTALL_METADATA_FILE_NAME);
        let source = fs::read_to_string(path).map_err(InstallError::ReadInstallMetadata)?;
        serde_json::from_str(&source).map_err(InstallError::ParseInstallMetadata)
    }

    fn staging_directory(&self, id: &AdapterId) -> Result<PathBuf, InstallError> {
        let nonce = STAGING_NONCE.fetch_add(1, Ordering::SeqCst);
        let path = self.root.join(".staging").join(format!(
            "{}-{}-{nonce}",
            id.as_str(),
            std::process::id()
        ));
        fs::create_dir_all(&path).map_err(InstallError::CreateStaging)?;
        Ok(path)
    }

    #[allow(clippy::too_many_arguments)]
    fn install_staged(
        &self,
        entry: &crate::AdapterEntry,
        release: &AdapterRelease,
        source: ArtifactSource,
        expected_size: Option<u64>,
        expected_checksum: &Sha256Checksum,
        platform: &Platform,
        executable: &Path,
        staging: &Path,
    ) -> Result<(), InstallError> {
        let staged_executable = staging.join(executable);
        let parent = staged_executable
            .parent()
            .ok_or_else(|| InstallError::InvalidLayout("executable has no parent".to_owned()))?;
        fs::create_dir_all(parent).map_err(InstallError::CreateStaging)?;
        let (size, checksum) =
            download_verify(&source, &staged_executable, self.max_artifact_bytes)?;
        match expected_size {
            Some(expected_size) if size != expected_size => {
                return Err(InstallError::SizeMismatch {
                    expected: expected_size,
                    actual: size,
                });
            }
            _ => {}
        }
        if checksum.as_str() != expected_checksum.as_str() {
            return Err(InstallError::ChecksumMismatch {
                expected: expected_checksum.as_str().to_owned(),
                actual: checksum.as_str().to_owned(),
            });
        }
        make_executable(&staged_executable)?;
        let manifest = serde_json::json!({
            "id": entry.id.as_str(),
            "name": entry.name,
            "version": release.version,
            "protocol_version": release.protocol_version,
            "executable": executable,
            "description": entry.description,
            "homepage": entry.homepage,
        });
        fs::write(
            staging.join(crate::MANIFEST_FILE_NAME),
            serde_json::to_vec_pretty(&manifest).map_err(InstallError::WriteManifest)?,
        )
        .map_err(InstallError::WriteManifestFile)?;
        let metadata = InstallMetadata {
            adapter_id: entry.id.as_str().to_owned(),
            version: release.version.clone(),
            registry_source: self.registry_source.clone(),
            artifact_source: source.as_str().to_owned(),
            sha256: expected_checksum.as_str().to_owned(),
            platform_os: platform.os().to_owned(),
            platform_architecture: platform.architecture().to_owned(),
        };
        fs::write(
            staging.join(INSTALL_METADATA_FILE_NAME),
            serde_json::to_vec_pretty(&metadata).map_err(InstallError::WriteInstallMetadata)?,
        )
        .map_err(InstallError::WriteInstallMetadataFile)
    }
}

/// Local provenance record. SHA-256 establishes byte integrity against
/// registry metadata; it is not a publisher-authenticity assertion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstallMetadata {
    pub adapter_id: String,
    pub version: String,
    pub registry_source: Option<String>,
    pub artifact_source: String,
    pub sha256: String,
    pub platform_os: String,
    pub platform_architecture: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallReceipt {
    pub adapter_id: AdapterId,
    pub version: String,
    pub platform: Platform,
    pub adapter_dir: PathBuf,
}

#[derive(Debug)]
pub enum InstallError {
    UnknownAdapter(AdapterId),
    RequestedVersionUnavailable(String),
    UnsupportedPlatform(Platform),
    AlreadyInstalled(AdapterId),
    NotInstalled(AdapterId),
    NoUpdateAvailable(AdapterId),
    InvalidInstalledVersion(String),
    UnsafeRemoval(PathBuf),
    ArtifactTooLarge { limit: u64, actual: u64 },
    SizeMismatch { expected: u64, actual: u64 },
    ChecksumMismatch { expected: String, actual: String },
    InvalidLayout(String),
    CreateRoot(io::Error),
    CreateStaging(io::Error),
    Download(io::Error),
    RemoteDownload(Box<ureq::Error>),
    WriteArtifact(io::Error),
    WriteManifest(serde_json::Error),
    WriteManifestFile(io::Error),
    WriteInstallMetadata(serde_json::Error),
    WriteInstallMetadataFile(io::Error),
    ReadInstallMetadata(io::Error),
    ParseInstallMetadata(serde_json::Error),
    AtomicInstall(io::Error),
    SetExecutable(io::Error),
    Remove(io::Error),
}

impl fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAdapter(id) => write!(formatter, "adapter is not in the registry: {id}"),
            Self::RequestedVersionUnavailable(version) => write!(
                formatter,
                "requested adapter version is unavailable: {version}"
            ),
            Self::UnsupportedPlatform(platform) => {
                write!(formatter, "no artifact for platform {platform}")
            }
            Self::AlreadyInstalled(id) => write!(formatter, "adapter is already installed: {id}"),
            Self::NotInstalled(id) => write!(formatter, "adapter is not installed: {id}"),
            Self::NoUpdateAvailable(id) => {
                write!(formatter, "no newer release is available for {id}")
            }
            Self::InvalidInstalledVersion(version) => {
                write!(
                    formatter,
                    "installed adapter version is not valid SemVer: {version}"
                )
            }
            Self::UnsafeRemoval(path) => write!(
                formatter,
                "refusing unsafe adapter removal: {}",
                path.display()
            ),
            Self::ArtifactTooLarge { limit, actual } => write!(
                formatter,
                "artifact is too large ({actual} bytes; limit {limit})"
            ),
            Self::SizeMismatch { expected, actual } => write!(
                formatter,
                "artifact size mismatch (expected {expected}; got {actual})"
            ),
            Self::ChecksumMismatch { expected, actual } => write!(
                formatter,
                "artifact checksum mismatch (expected {expected}; got {actual})"
            ),
            Self::InvalidLayout(message) => write!(formatter, "invalid artifact layout: {message}"),
            Self::CreateRoot(error) => write!(formatter, "failed to create adapter root: {error}"),
            Self::CreateStaging(error) => {
                write!(formatter, "failed to create installer staging: {error}")
            }
            Self::Download(error) => write!(formatter, "failed to read adapter artifact: {error}"),
            Self::RemoteDownload(error) => {
                write!(formatter, "failed to download adapter artifact: {error}")
            }
            Self::WriteArtifact(error) => {
                write!(formatter, "failed to stage adapter artifact: {error}")
            }
            Self::WriteManifest(error) => {
                write!(formatter, "failed to encode installed manifest: {error}")
            }
            Self::WriteManifestFile(error) => {
                write!(formatter, "failed to write installed manifest: {error}")
            }
            Self::WriteInstallMetadata(error) => {
                write!(formatter, "failed to encode install metadata: {error}")
            }
            Self::WriteInstallMetadataFile(error) => {
                write!(formatter, "failed to write install metadata: {error}")
            }
            Self::ReadInstallMetadata(error) => {
                write!(formatter, "failed to read install metadata: {error}")
            }
            Self::ParseInstallMetadata(error) => {
                write!(formatter, "failed to parse install metadata: {error}")
            }
            Self::AtomicInstall(error) => {
                write!(formatter, "failed to atomically install adapter: {error}")
            }
            Self::SetExecutable(error) => {
                write!(formatter, "failed to set executable permissions: {error}")
            }
            Self::Remove(error) => write!(formatter, "failed to remove installed adapter: {error}"),
        }
    }
}

impl Error for InstallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CreateRoot(error)
            | Self::CreateStaging(error)
            | Self::Download(error)
            | Self::WriteArtifact(error)
            | Self::WriteManifestFile(error)
            | Self::WriteInstallMetadataFile(error)
            | Self::ReadInstallMetadata(error)
            | Self::AtomicInstall(error)
            | Self::SetExecutable(error)
            | Self::Remove(error) => Some(error),
            Self::RemoteDownload(error) => Some(error),
            Self::WriteManifest(error) => Some(error),
            Self::WriteInstallMetadata(error) | Self::ParseInstallMetadata(error) => Some(error),
            Self::UnknownAdapter(_)
            | Self::RequestedVersionUnavailable(_)
            | Self::UnsupportedPlatform(_)
            | Self::AlreadyInstalled(_)
            | Self::NotInstalled(_)
            | Self::NoUpdateAvailable(_)
            | Self::InvalidInstalledVersion(_)
            | Self::UnsafeRemoval(_)
            | Self::ArtifactTooLarge { .. }
            | Self::SizeMismatch { .. }
            | Self::ChecksumMismatch { .. }
            | Self::InvalidLayout(_) => None,
        }
    }
}

fn read_installed_release(target: &Path, id: &AdapterId) -> Result<String, InstallError> {
    let manifest_path = target.join(crate::MANIFEST_FILE_NAME);
    let source = fs::read_to_string(manifest_path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => InstallError::NotInstalled(id.clone()),
        _ => InstallError::Download(error),
    })?;
    crate::AdapterManifest::from_json(&source)
        .map(|manifest| manifest.version)
        .map_err(|error| InstallError::InvalidLayout(error.to_string()))
}

fn select_release<'a>(
    releases: &'a [AdapterRelease],
    requested: Option<&str>,
) -> Result<&'a AdapterRelease, InstallError> {
    if let Some(version) = requested {
        return releases
            .iter()
            .find(|release| release.version == version)
            .ok_or_else(|| InstallError::RequestedVersionUnavailable(version.to_owned()));
    }
    releases
        .iter()
        .filter_map(|release| {
            Version::parse(&release.version)
                .ok()
                .map(|version| (version, release))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, release)| release)
        .ok_or_else(|| {
            InstallError::RequestedVersionUnavailable("no compatible release".to_owned())
        })
}

fn download_verify(
    source: &ArtifactSource,
    destination: &Path,
    limit: u64,
) -> Result<(u64, Sha256Checksum), InstallError> {
    let mut reader: Box<dyn Read> = if let Some(path) = source.as_str().strip_prefix("file://") {
        Box::new(File::open(path).map_err(InstallError::Download)?)
    } else {
        Box::new(
            ureq::get(source.as_str())
                .call()
                .map_err(|error| InstallError::RemoteDownload(Box::new(error)))?
                .into_reader(),
        )
    };
    let mut output = File::create(destination).map_err(InstallError::WriteArtifact)?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).map_err(InstallError::Download)?;
        if count == 0 {
            break;
        }
        size = size.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        if size > limit {
            return Err(InstallError::ArtifactTooLarge {
                limit,
                actual: size,
            });
        }
        hasher.update(&buffer[..count]);
        output
            .write_all(&buffer[..count])
            .map_err(InstallError::WriteArtifact)?;
    }
    let checksum = Sha256Checksum::new(format!("{:x}", hasher.finalize()))
        .expect("SHA-256 formatting is always a valid digest");
    Ok((size, checksum))
}

fn make_executable(path: &Path) -> Result<(), InstallError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(InstallError::SetExecutable)?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).map_err(InstallError::SetExecutable)?;
    }
    Ok(())
}
