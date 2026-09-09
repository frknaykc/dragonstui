use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use clap::{Parser, Subcommand};
use dragonstui_adapter_host::{
    AdapterClassification, AdapterController, AdapterId, AdapterInstaller, ControllerClient,
    ControllerIpcServer, ControllerIpcStatus, ControllerManagementClient, DiscoveredAdapter,
    LocalAdapterRoot, Platform, Registry,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Parser)]
#[command(
    name = "dragonstui-adapter",
    version,
    about = "Manage DragonsTUI external adapters",
    long_about = "Manage DragonsTUI external adapters without entering alternate-screen TUI mode.\n\nThe adapter root defaults to ./adapters; override it with --root."
)]
struct Cli {
    /// Directory containing installed adapter directories.
    #[arg(long, global = true, value_name = "PATH")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Search a local or HTTPS adapter registry.
    Search {
        query: Option<String>,
        #[arg(long)]
        registry: String,
    },
    /// List locally installed adapters without starting them.
    List,
    /// Display locally installed adapter metadata without starting it.
    Info { id: String },
    /// Verify and install a registry adapter without starting it.
    Install {
        id: String,
        #[arg(long)]
        registry: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Verify and replace an installed adapter with a newer registry release.
    Update {
        id: String,
        #[arg(long)]
        registry: String,
    },
    /// Remove a locally installed adapter directory. Requires --yes.
    Remove {
        id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Explicitly start an installed adapter through the local controller daemon.
    Start { id: String },
    /// Stop a running adapter through the local controller daemon.
    Stop { id: String },
    /// Restart an installed adapter through the local controller daemon.
    Restart { id: String },
    #[command(hide = true)]
    ControllerDaemon,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dragonstui-adapter: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let root = cli.root.unwrap_or_else(|| PathBuf::from("adapters"));
    match cli.command {
        Command::ControllerDaemon => {
            let token = std::env::var("DRAGONSTUI_CONTROLLER_TOKEN")
                .map_err(|_| "controller daemon credential is missing".to_owned())?;
            return run_controller_daemon(&root, &token);
        }
        Command::Search { query, registry } => {
            let registry = load_registry(&registry)?;
            for entry in registry.search(query.as_deref().unwrap_or("")) {
                println!(
                    "{:<20} {}{}",
                    entry.id,
                    entry.name,
                    entry
                        .description
                        .as_deref()
                        .map(|description| format!(" — {description}"))
                        .unwrap_or_default()
                );
            }
        }
        Command::List => {
            println!("ID\tVERSION\tSTATE\tPROTOCOL");
            let entries = discover(&root)?;
            if entries.is_empty() {
                eprintln!(
                    "No installed adapters in {}.\nUse this same path with showcase --adapter-root.\nNext: dragonstui-adapter --root <path> install <id> --registry <trusted-path-or-https-url>\nInstall does not start an adapter; use start <id> explicitly. No registry is bundled.",
                    root.display()
                );
            }
            for entry in entries {
                let (id, version, protocol) = entry
                    .manifest()
                    .map(|manifest| {
                        (
                            manifest.id.to_string(),
                            manifest.version.clone(),
                            manifest.protocol_version.to_string(),
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            entry
                                .adapter_dir()
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("unknown")
                                .to_owned(),
                            "-".to_owned(),
                            "-".to_owned(),
                        )
                    });
                let state = entry
                    .manifest()
                    .and_then(|manifest| live_state(&root, &manifest.id))
                    .unwrap_or_else(|| display_state(&entry).to_owned());
                println!("{id}\t{version}\t{state}\t{protocol}");
            }
        }
        Command::Info { id } => {
            let id = parse_id(&id)?;
            let entry = discover(&root)?
                .into_iter()
                .find(|entry| entry.manifest().is_some_and(|manifest| manifest.id == id))
                .ok_or_else(|| format!("adapter is not installed: {id}"))?;
            let manifest = entry
                .manifest()
                .ok_or_else(|| format!("adapter has no valid manifest: {id}"))?;
            println!("ID: {}", manifest.id);
            println!("Name: {}", manifest.name);
            println!("Version: {}", manifest.version);
            println!("Protocol: {}", manifest.protocol_version);
            let state = live_state(&root, &id).unwrap_or_else(|| display_state(&entry).to_owned());
            println!("State: {state}");
            println!("Executable: {}", manifest.executable.as_path().display());
            if let Ok(metadata) = AdapterInstaller::new(&root).install_metadata(&id) {
                println!("SHA-256: {}", metadata.sha256);
                println!("Artifact source: {}", metadata.artifact_source);
                if let Some(source) = metadata.registry_source {
                    println!("Registry source: {source}");
                }
                println!(
                    "Platform: {}/{}",
                    metadata.platform_os, metadata.platform_architecture
                );
            }
        }
        Command::Install {
            id,
            registry,
            version,
        } => {
            let id = parse_id(&id)?;
            let receipt = AdapterInstaller::new(&root)
                .registry_source(&registry)
                .install(
                    &load_registry(&registry)?,
                    &id,
                    version.as_deref(),
                    &Platform::current().map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            println!("Installed {} {}", receipt.adapter_id, receipt.version);
        }
        Command::Update { id, registry } => {
            let id = parse_id(&id)?;
            let installer = AdapterInstaller::new(&root).registry_source(&registry);
            let prepared = installer
                .prepare_update(
                    &load_registry(&registry)?,
                    &id,
                    &Platform::current().map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            unregister_if_controlled(&root, &id)?;
            let receipt = installer
                .commit_update(prepared)
                .map_err(|error| error.to_string())?;
            println!("Updated {} {}", receipt.adapter_id, receipt.version);
        }
        Command::Remove { id, yes } => {
            if !yes {
                return Err("refusing to remove without --yes".to_owned());
            }
            let id = parse_id(&id)?;
            unregister_if_controlled(&root, &id)?;
            AdapterInstaller::new(&root)
                .remove(&id)
                .map_err(|error| error.to_string())?;
            println!("Removed {id}");
        }
        Command::Start { id } => {
            let id = parse_id(&id)?;
            controller_client(&root)?
                .start(&id)
                .map_err(|error| error.to_string())?;
            println!("Started {id}");
        }
        Command::Stop { id } => {
            let id = parse_id(&id)?;
            controller_client(&root)?
                .stop(&id)
                .map_err(|error| error.to_string())?;
            println!("Stopped {id}");
        }
        Command::Restart { id } => {
            let id = parse_id(&id)?;
            controller_client(&root)?
                .restart(&id)
                .map_err(|error| error.to_string())?;
            println!("Restarted {id}");
        }
    }
    Ok(())
}

const CONTROLLER_DIRECTORY: &str = ".controller";
const CONTROLLER_ENDPOINT: &str = "endpoint.json";

#[derive(Deserialize, Serialize)]
struct ControllerEndpoint {
    address: std::net::SocketAddr,
    token: String,
}

fn controller_endpoint_path(root: &Path) -> PathBuf {
    root.join(CONTROLLER_DIRECTORY).join(CONTROLLER_ENDPOINT)
}

fn controller_client(root: &Path) -> Result<ControllerManagementClient, String> {
    if let Some(endpoint) = read_controller_endpoint(root)? {
        let client = ControllerManagementClient::new(endpoint.address, endpoint.token);
        client
            .diagnostics(&AdapterId::new("controller-probe").expect("static valid ID"))
            .map_err(|error| {
                format!("existing controller is unavailable; endpoint preserved: {error}")
            })?;
        return Ok(client);
    }

    fs::create_dir_all(root.join(CONTROLLER_DIRECTORY)).map_err(|error| error.to_string())?;
    let token = generate_controller_token()?;
    ProcessCommand::new(std::env::current_exe().map_err(|error| error.to_string())?)
        .arg("--root")
        .arg(root)
        .arg("controller-daemon")
        .env("DRAGONSTUI_CONTROLLER_TOKEN", &token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start local controller daemon: {error}"))?;

    for _ in 0..100 {
        thread::sleep(Duration::from_millis(10));
        if let Some(endpoint) = read_controller_endpoint(root)? {
            let client = ControllerManagementClient::new(endpoint.address, endpoint.token);
            if client
                .diagnostics(&AdapterId::new("controller-probe").expect("static valid ID"))
                .is_ok()
            {
                return Ok(client);
            }
        }
    }
    Err("local controller daemon did not become ready".to_owned())
}

fn run_controller_daemon(root: &Path, token: &str) -> Result<(), String> {
    let stopping = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        signal_hook::flag::register(signal, Arc::clone(&stopping))
            .map_err(|error| format!("could not install controller shutdown handler: {error}"))?;
    }
    let sigpipe = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGPIPE, Arc::clone(&sigpipe))
        .map_err(|error| format!("could not install controller SIGPIPE handler: {error}"))?;
    fs::create_dir_all(root.join(CONTROLLER_DIRECTORY)).map_err(|error| error.to_string())?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let endpoint = ControllerEndpoint {
        address: listener.local_addr().map_err(|error| error.to_string())?,
        token: token.to_owned(),
    };
    let endpoint_path = controller_endpoint_path(root);
    publish_controller_endpoint(&endpoint_path, &endpoint)?;
    let result = ControllerIpcServer::new(
        listener,
        AdapterController::new(root, Duration::from_secs(2), 128),
        token,
    )
    .serve_until(&stopping);
    let _ = fs::remove_file(endpoint_path);
    if stopping.load(Ordering::Relaxed) {
        return Ok(());
    }
    result.map_err(|error| error.to_string())
}

fn publish_controller_endpoint(path: &Path, endpoint: &ControllerEndpoint) -> Result<(), String> {
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        serde_json::to_writer(&mut file, endpoint).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_controller_endpoint(root: &Path) -> Result<Option<ControllerEndpoint>, String> {
    let path = controller_endpoint_path(root);
    let body = match fs::read(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let endpoint: ControllerEndpoint = serde_json::from_slice(&body)
        .map_err(|_| "invalid local controller endpoint".to_owned())?;
    if !endpoint.address.ip().is_loopback() {
        return Err("local controller endpoint must use a loopback address".to_owned());
    }
    Ok(Some(endpoint))
}

fn live_state(root: &Path, id: &AdapterId) -> Option<String> {
    let endpoint = read_controller_endpoint(root).ok()??;
    match ControllerClient::new(endpoint.address, endpoint.token)
        .status(id)
        .ok()?
    {
        ControllerIpcStatus::State(state) => Some(state),
        ControllerIpcStatus::Missing
        | ControllerIpcStatus::Diagnostics(_)
        | ControllerIpcStatus::Management(_)
        | ControllerIpcStatus::Actions(_)
        | ControllerIpcStatus::Sessions(_)
        | ControllerIpcStatus::Action(_)
        | ControllerIpcStatus::SessionOpened { .. }
        | ControllerIpcStatus::SessionActive(_)
        | ControllerIpcStatus::SessionEvents(_)
        | ControllerIpcStatus::Operation(_)
        | ControllerIpcStatus::Operations(_)
        | ControllerIpcStatus::LiveData(_)
        | ControllerIpcStatus::Completed => None,
    }
}

fn unregister_if_controlled(root: &Path, id: &AdapterId) -> Result<(), String> {
    let Some(endpoint) = read_controller_endpoint(root)? else {
        return Ok(());
    };
    let client = ControllerClient::new(endpoint.address, endpoint.token);
    match client.status(id).map_err(|error| error.to_string())? {
        ControllerIpcStatus::State(_) => {
            match client.unregister(id).map_err(|error| error.to_string())? {
                ControllerIpcStatus::Completed => {}
                _ => return Err("unexpected controller unregister response".to_owned()),
            }
        }
        ControllerIpcStatus::Missing => {}
        _ => return Err("unexpected controller status response".to_owned()),
    }
    Ok(())
}

fn generate_controller_token() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("could not generate controller credential: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn parse_id(source: &str) -> Result<AdapterId, String> {
    AdapterId::new(source).map_err(|error| format!("invalid adapter id: {error}"))
}

fn discover(root: &Path) -> Result<Vec<DiscoveredAdapter>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    LocalAdapterRoot::new(root).discover().map_err(|error| {
        format!(
            "Cannot read adapter root {}: {error}. Check --root points to a readable directory.",
            root.display()
        )
    })
}

fn display_state(entry: &DiscoveredAdapter) -> &'static str {
    match entry.classification() {
        AdapterClassification::Valid => "stopped",
        AdapterClassification::InvalidManifest => "invalid-manifest",
        AdapterClassification::MissingExecutable => "missing-executable",
        AdapterClassification::UnsupportedProtocol => "incompatible",
    }
}

fn load_registry(source: &str) -> Result<Registry, String> {
    Registry::load(source).map_err(|error| error.to_string())
}
