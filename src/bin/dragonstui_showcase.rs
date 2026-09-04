//! Standalone Dragonfire showcase for DragonsTUI.
//!
//! This binary is an application consumer of the public crate API. It uses no
//! network, agents, API keys, external processes, or framework-private hooks.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write, stdout},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use dragons_tui::{
    Alignment, Animation, BorderSet, Canvas, Cell, Color, CommandId, CommandPalette, Constraint,
    Event, FocusId, FocusState, Frame, Gauge, KeyCode, KeyEvent, KeyMap, KeyModifiers, Line, List,
    ListState, Modal, MouseEvent, MouseKind, PaletteCommand, Panel, Position, ProgressBar, Rect,
    RichText, Runtime, ShutdownSignal, Size, Span, Sparkline, Spinner, Style, Table, TableColumn,
    TableState, Text, TextArea, TextInput, Theme, Tree, TreeNode, TreeState, Viewport,
    ViewportState, display_width, is_quit_key, terminal_size,
};
use dragonstui_adapter_host::{
    AdapterClassification, AdapterId, AdapterManagement, AdapterManagementAction,
    ControllerIpcDiagnostics, DiscoveryError, LocalAdapterRoot, local_controller_diagnostics,
    local_controller_management_client,
};

const TICK_INTERVAL: Duration = Duration::from_millis(50);
const SPLASH_DURATION: Duration = Duration::from_secs(6);
const LIST_FOCUS: FocusId = FocusId::new(1);
const TABLE_FOCUS: FocusId = FocusId::new(2);
const TREE_FOCUS: FocusId = FocusId::new(3);
const VIEWPORT_FOCUS: FocusId = FocusId::new(4);
const INPUT_FOCUS: FocusId = FocusId::new(5);
const AREA_FOCUS: FocusId = FocusId::new(6);

const SPLASH_TITLE: [&str; 8] = [
    "             ██████╗ ██████╗  █████╗  ██████╗  ██████╗ ███╗   ██╗",
    "                ██╔══██╗██╔══██╗██╔══██╗██╔════╝ ██╔═══██╗████╗  ██║",
    "                ██║  ██║██████╔╝███████║██║  ███╗██║   ██║██╔██╗ ██║",
    "                ██║  ██║██╔══██╗██╔══██║██║   ██║██║   ██║██║╚██╗██║",
    "                ██████╔╝██║  ██║██║  ██║╚██████╔╝╚██████╔╝██║ ╚████║",
    "                ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═══╝",
    "                 𓆩 -- we are the recall. not born. remembered. -- 𓆪",
    "      ━━━━━━*** scales of code · wings of truth · chaos is language ***━━━━━━",
];

const SPLASH_DRAGON: [&str; 16] = [
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣀⣤⣤⣤⣤⡼⠀⢀⡀⣀⢱⡄⡀⠀⠀⠀⢲⣤⣤⣤⣤⣀⣀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣴⣾⣿⣿⣿⣿⣿⡿⠛⠋⠁⣤⣿⣿⣿⣧⣷⠀⠀⠘⠉⠛⢻⣷⣿⣽⣿⣿⣷⣦⣄⡀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⢀⣴⣞⣽⣿⣿⣿⣿⣿⣿⣿⠁⠀⠀⠠⣿⣿⡟⢻⣿⣿⣇⠀⠀⠀⠀⠀⣿⣿⣿⣿⣿⣿⣿⣿⣟⢦⡀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⣠⣿⡾⣿⣿⣿⣿⣿⠿⣻⣿⣿⡀⠀⠀⠀⢻⣿⣷⡀⠻⣧⣿⠆⠀⠀⠀⠀⣿⣿⣿⡻⣿⣿⣿⣿⣿⠿⣽⣦⡀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⣼⠟⣩⣾⣿⣿⣿⢟⣵⣾⣿⣿⣿⣧⠀⠀⠀⠈⠿⣿⣿⣷⣈⠁⠀⠀⠀⠀⣰⣿⣿⣿⣿⣮⣟⢯⣿⣿⣷⣬⡻⣷⡄⠀⠀⠀",
    "⠀⠀⢀⡜⣡⣾⣿⢿⣿⣿⣿⣿⣿⢟⣵⣿⣿⣿⣷⣄⠀⣰⣿⣿⣿⣿⣿⣷⣄⠀⢀⣼⣿⣿⣿⣷⡹⣿⣿⣿⣿⣿⣿⢿⣿⣮⡳⡄⠀⠀",
    "⠀⢠⢟⣿⡿⠋⣠⣾⢿⣿⣿⠟⢃⣾⢟⣿⢿⣿⣿⣿⣾⡿⠟⠻⣿⣻⣿⣏⠻⣿⣾⣿⣿⣿⣿⡛⣿⡌⠻⣿⣿⡿⣿⣦⡙⢿⣿⡝⣆⠀",
    "⠀⢯⣿⠏⣠⠞⠋⠀⣠⡿⠋⢀⣿⠁⢸⡏⣿⠿⣿⣿⠃⢠⣴⣾⣿⣿⣿⡟⠀⠘⢹⣿⠟⣿⣾⣷⠈⣿⡄⠘⢿⣦⠀⠈⠻⣆⠙⣿⣜⠆",
    "⢀⣿⠃⡴⠃⢀⡠⠞⠋⠀⠀⠼⠋⠀⠸⡇⠻⠀⠈⠃⠀⣧⢋⣼⣿⣿⣿⣷⣆⠀⠈⠁⠀⠟⠁⡟⠀⠈⠻⠀⠀⠉⠳⢦⡀⠈⢣⠈⢿⡄",
    "⣸⠇⢠⣷⠞⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⠻⠿⠿⠋⠀⢻⣿⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠙⢾⣆⠈⣷",
    "⡟⠀⡿⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣴⣶⣤⡀⢸⣿⠇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢻⡄⢹",
    "⡇⠀⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⡇⠀⠈⣿⣼⡟⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠃⢸",
    "⢡⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⠶⣶⡟⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡼",
    "⠈⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⡾⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠁",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⡁⢠⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣿⣿⣼⣀⣠⠂⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
];

const SPLASH_MANIFESTO: [&str; 3] = [
    "we are the recall. not born. remembered.",
    "scales of code. wings of truth. fire is signal. chaos is language.",
    "we build. we burn. we become.",
];

const LOADING_TEXT: [&str; 3] = ["𝐍𝐨𝐰 𝐥𝐨𝐚𝐝𝐢𝐧𝐠.  ", "𝐍𝐨𝐰 𝐥𝐨𝐚𝐝𝐢𝐧𝐠. . ", "𝐍𝐨𝐰 𝐥𝐨𝐚𝐝𝐢𝐧𝐠. . ."];
const LOADING_HINT: &str = "Enter or Space to continue · q to quit";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Splash,
    Showcase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Section {
    Overview,
    Widgets,
    Data,
    Graphics,
    Input,
    Interaction,
    Settings,
    Adapters,
}

impl Section {
    const ALL: [Self; 8] = [
        Self::Overview,
        Self::Widgets,
        Self::Data,
        Self::Graphics,
        Self::Input,
        Self::Interaction,
        Self::Settings,
        Self::Adapters,
    ];

    fn title(self, language: Language) -> &'static str {
        localized(
            language,
            match self {
                Self::Overview => "Overview",
                Self::Widgets => "Widgets",
                Self::Data => "Data",
                Self::Graphics => "Graphics",
                Self::Input => "Input",
                Self::Interaction => "Interaction",
                Self::Settings => "Settings",
                Self::Adapters => "Adapters",
            },
            match self {
                Self::Overview => "Genel Bakış",
                Self::Widgets => "Bileşenler",
                Self::Data => "Veri",
                Self::Graphics => "Grafikler",
                Self::Input => "Girdi",
                Self::Interaction => "Etkileşim",
                Self::Settings => "Ayarlar",
                Self::Adapters => "Adaptörler",
            },
        )
    }

    fn command(self) -> &'static str {
        match self {
            Self::Overview => "show-overview",
            Self::Widgets => "show-widgets",
            Self::Data => "show-data",
            Self::Graphics => "show-graphics",
            Self::Input => "show-input",
            Self::Interaction => "show-interaction",
            Self::Settings => "open-settings",
            Self::Adapters => "open-adapters",
        }
    }

    fn from_number(character: char) -> Option<Self> {
        match character {
            '1' => Some(Self::Overview),
            '2' => Some(Self::Widgets),
            '3' => Some(Self::Data),
            '4' => Some(Self::Graphics),
            '5' => Some(Self::Input),
            '6' => Some(Self::Interaction),
            '7' => Some(Self::Settings),
            '8' => Some(Self::Adapters),
            _ => None,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Widgets => 1,
            Self::Data => 2,
            Self::Graphics => 3,
            Self::Input => 4,
            Self::Interaction => 5,
            Self::Settings => 6,
            Self::Adapters => 7,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Language {
    #[default]
    English,
    Turkish,
}

fn localized<'a>(language: Language, english: &'a str, turkish: &'a str) -> &'a str {
    match language {
        Language::English => english,
        Language::Turkish => turkish,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdapterViewState {
    Stopped,
    Failed,
    Incompatible,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AdapterBrowserMode {
    #[default]
    Adapters,
    Capabilities,
}

impl AdapterViewState {
    fn label(self, language: Language) -> &'static str {
        match self {
            Self::Stopped => localized(language, "Stopped", "Durduruldu"),
            Self::Failed => localized(language, "Failed", "Başarısız"),
            Self::Incompatible => localized(language, "Incompatible", "Uyumsuz"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdapterRow {
    id: String,
    name: String,
    version: String,
    state: AdapterViewState,
    protocol: String,
    executable: String,
    last_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapabilityProvider {
    id: String,
    name: String,
    state: String,
}

fn capability_provider_index(
    adapter_rows: &[AdapterRow],
    diagnostics: &BTreeMap<String, ControllerIpcDiagnostics>,
) -> BTreeMap<String, Vec<CapabilityProvider>> {
    let mut providers_by_capability =
        BTreeMap::<String, BTreeMap<String, CapabilityProvider>>::new();
    for row in adapter_rows {
        let Some(snapshot) = diagnostics.get(&row.id) else {
            continue;
        };
        if snapshot.adapter_id != row.id {
            continue;
        }
        for capability in &snapshot.capabilities {
            providers_by_capability
                .entry(capability.clone())
                .or_default()
                .entry(row.id.clone())
                .or_insert_with(|| CapabilityProvider {
                    id: row.id.clone(),
                    name: row.name.clone(),
                    state: snapshot.state.clone(),
                });
        }
    }
    providers_by_capability
        .into_iter()
        .map(|(capability, providers)| (capability, providers.into_values().collect()))
        .collect()
}

fn adapter_rows_from_root(root: &Path) -> Result<Vec<AdapterRow>, DiscoveryError> {
    let discovered = LocalAdapterRoot::new(root).discover()?;
    Ok(discovered
        .into_iter()
        .map(|entry| {
            let manifest = entry.manifest();
            let state = match entry.classification() {
                AdapterClassification::Valid => AdapterViewState::Stopped,
                AdapterClassification::UnsupportedProtocol => AdapterViewState::Incompatible,
                AdapterClassification::InvalidManifest
                | AdapterClassification::MissingExecutable => AdapterViewState::Failed,
            };
            AdapterRow {
                id: manifest
                    .map(|manifest| manifest.id.to_string())
                    .unwrap_or_else(|| "—".to_owned()),
                name: manifest
                    .map(|manifest| manifest.name.clone())
                    .unwrap_or_else(|| entry.adapter_dir().display().to_string()),
                version: manifest
                    .map(|manifest| manifest.version.clone())
                    .unwrap_or_else(|| "—".to_owned()),
                state,
                protocol: manifest
                    .map(|manifest| manifest.protocol_version.to_string())
                    .unwrap_or_else(|| "—".to_owned()),
                executable: entry
                    .resolved_executable()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "—".to_owned()),
                last_error: entry.error().map(ToOwned::to_owned),
            }
        })
        .collect())
}

#[derive(Clone, Default)]
struct AdapterActionConflictGuard {
    in_flight: Arc<Mutex<BTreeSet<AdapterId>>>,
}

impl AdapterActionConflictGuard {
    fn acquire(&self, id: &AdapterId) -> Result<AdapterActionPermit, String> {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !in_flight.insert(id.clone()) {
            return Err(format!(
                "another adapter management operation for {id} is already in progress"
            ));
        }
        Ok(AdapterActionPermit {
            in_flight: Arc::clone(&self.in_flight),
            id: id.clone(),
        })
    }
}

struct AdapterActionPermit {
    in_flight: Arc<Mutex<BTreeSet<AdapterId>>>,
    id: AdapterId,
}

impl Drop for AdapterActionPermit {
    fn drop(&mut self) {
        self.in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.id);
    }
}

fn execute_adapter_action(root: &Path, action: AdapterManagementAction) -> Result<String, String> {
    match action {
        action @ (AdapterManagementAction::Start { .. }
        | AdapterManagementAction::Stop { .. }
        | AdapterManagementAction::Restart { .. }) => {
            let client = local_controller_management_client(root)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "controller daemon is unavailable".to_owned())?;
            let outcome = match action {
                AdapterManagementAction::Start { id } => client.start(&id),
                AdapterManagementAction::Stop { id } => client.stop(&id),
                AdapterManagementAction::Restart { id } => client.restart(&id),
                _ => unreachable!("only lifecycle actions reach the daemon client"),
            }
            .map_err(|error| error.to_string())?;
            Ok(format!("{outcome:?}"))
        }
        action => AdapterManagement::new(root.to_path_buf())
            .execute(action)
            .map(|outcome| format!("{outcome:?}"))
            .map_err(|error| error.to_string()),
    }
}

fn adapter_action_worker(
    root: &Path,
) -> (
    Sender<AdapterManagementAction>,
    Receiver<Result<String, String>>,
) {
    let (actions, action_receiver) = mpsc::channel::<AdapterManagementAction>();
    let (results, result_receiver) = mpsc::channel::<Result<String, String>>();
    let root = root.to_path_buf();
    thread::spawn(move || {
        let guard = AdapterActionConflictGuard::default();
        while let Ok(action) = action_receiver.recv() {
            let permit = match guard.acquire(action.id()) {
                Ok(permit) => permit,
                Err(error) => {
                    if results.send(Err(error)).is_err() {
                        break;
                    }
                    continue;
                }
            };
            let root = root.clone();
            let results = results.clone();
            thread::spawn(move || {
                let result = execute_adapter_action(&root, action);
                drop(permit);
                let _ = results.send(result);
            });
        }
    });
    (actions, result_receiver)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThemeSlot {
    Background,
    Primary,
    Secondary,
    Success,
    Warning,
    Error,
    Text,
    Muted,
}

impl ThemeSlot {
    const ALL: [Self; 8] = [
        Self::Background,
        Self::Primary,
        Self::Secondary,
        Self::Success,
        Self::Warning,
        Self::Error,
        Self::Text,
        Self::Muted,
    ];

    fn label(self, language: Language) -> &'static str {
        localized(
            language,
            match self {
                Self::Background => "Background",
                Self::Primary => "Primary",
                Self::Secondary => "Secondary",
                Self::Success => "Success",
                Self::Warning => "Warning",
                Self::Error => "Error",
                Self::Text => "Text",
                Self::Muted => "Muted",
            },
            match self {
                Self::Background => "Arka Plan",
                Self::Primary => "Birincil",
                Self::Secondary => "İkincil",
                Self::Success => "Başarı",
                Self::Warning => "Uyarı",
                Self::Error => "Hata",
                Self::Text => "Metin",
                Self::Muted => "Soluk",
            },
        )
    }

    fn color(self, theme: Theme) -> Color {
        match self {
            Self::Background => theme.background,
            Self::Primary => theme.primary,
            Self::Secondary => theme.secondary,
            Self::Success => theme.success,
            Self::Warning => theme.warning,
            Self::Error => theme.error,
            Self::Text => theme.text,
            Self::Muted => theme.muted,
        }
    }

    fn set(self, theme: &mut Theme, color: Color) {
        match self {
            Self::Background => theme.background = color,
            Self::Primary => theme.primary = color,
            Self::Secondary => theme.secondary = color,
            Self::Success => theme.success = color,
            Self::Warning => theme.warning = color,
            Self::Error => theme.error = color,
            Self::Text => theme.text = color,
            Self::Muted => theme.muted = color,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingRow {
    English,
    Turkish,
    Color(ThemeSlot),
    Reset,
}

impl SettingRow {
    const ALL: [Self; 11] = [
        Self::English,
        Self::Turkish,
        Self::Color(ThemeSlot::Background),
        Self::Color(ThemeSlot::Primary),
        Self::Color(ThemeSlot::Secondary),
        Self::Color(ThemeSlot::Success),
        Self::Color(ThemeSlot::Warning),
        Self::Color(ThemeSlot::Error),
        Self::Color(ThemeSlot::Text),
        Self::Color(ThemeSlot::Muted),
        Self::Reset,
    ];

    fn next(self, direction: i8) -> Self {
        let index = Self::ALL.iter().position(|row| *row == self).unwrap_or(0);
        let length = Self::ALL.len();
        let next = if direction.is_negative() {
            (index + length - 1) % length
        } else {
            (index + 1) % length
        };
        Self::ALL[next]
    }
}

#[derive(Clone, Debug)]
struct SettingsState {
    selected: SettingRow,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            selected: SettingRow::English,
        }
    }
}

#[derive(Clone, Debug)]
struct ColorEditor {
    slot: ThemeSlot,
    channel: usize,
    inputs: [TextInput; 3],
}

impl ColorEditor {
    fn new(slot: ThemeSlot, color: Color) -> Self {
        let Color::Rgb { r, g, b } = color;
        Self {
            slot,
            channel: 0,
            inputs: [
                seeded_numeric_input(r),
                seeded_numeric_input(g),
                seeded_numeric_input(b),
            ],
        }
    }

    fn color(&self) -> Color {
        Color::rgb(
            parse_rgb(self.inputs[0].text()),
            parse_rgb(self.inputs[1].text()),
            parse_rgb(self.inputs[2].text()),
        )
    }
}

fn seeded_numeric_input(value: u8) -> TextInput {
    let mut input = TextInput::new();
    for character in value.to_string().chars() {
        input.insert(character);
    }
    input
}

fn parse_rgb(value: &str) -> u8 {
    value.parse::<i16>().unwrap_or_default().clamp(0, 255) as u8
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Outcome {
    quit: bool,
    redraw: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct HitRegions {
    sections: [Rect; 8],
    list: Rect,
    table: Rect,
    tree: Rect,
    viewport: Rect,
    input: Rect,
    area: Rect,
    modal: Rect,
    palette: Rect,
    palette_rows: Rect,
    settings_rows: [Rect; 11],
    color_inputs: [Rect; 3],
    color_apply: Rect,
}

struct Showcase {
    phase: Phase,
    started: Instant,
    splash_animation: Animation<u8>,
    spinner: Spinner,
    section: Section,
    language: Language,
    theme: Theme,
    settings: SettingsState,
    color_editor: Option<ColorEditor>,
    focus: FocusState,
    keymap: KeyMap,
    list: ListState,
    table: TableState,
    tree: TreeState,
    viewport: ViewportState,
    input: TextInput,
    area: TextArea,
    adapter_root: Option<PathBuf>,
    adapter_registry_source: Option<String>,
    adapter_install_id: Option<AdapterId>,
    adapter_rows: Vec<AdapterRow>,
    adapter_browser_mode: AdapterBrowserMode,
    adapter_discovery_error: Option<String>,
    adapter_diagnostics: BTreeMap<String, ControllerIpcDiagnostics>,
    adapter_controller_error: Option<String>,
    adapter_diagnostics_sender: Option<Sender<AdapterDiagnosticsSnapshot>>,
    adapter_diagnostics_results: Option<Receiver<AdapterDiagnosticsSnapshot>>,
    adapter_diagnostics_refresh_in_flight: bool,
    last_adapter_diagnostics_refresh: Instant,
    adapter_action_sender: Option<Sender<AdapterManagementAction>>,
    adapter_action_results: Option<Receiver<Result<String, String>>>,
    adapter_action_status: Option<String>,
    adapter_remove_confirmation: bool,
    palette: Option<CommandPalette>,
    modal_open: bool,
    focus_before_modal: Option<FocusId>,
    animation_enabled: bool,
    hits: HitRegions,
}

type AdapterDiagnosticsSnapshot = (BTreeMap<String, ControllerIpcDiagnostics>, Option<String>);

impl Showcase {
    #[cfg(test)]
    fn new(started: Instant) -> Self {
        Self::with_adapter_root(started, None)
    }

    #[cfg(test)]
    fn with_adapter_root(started: Instant, adapter_root: Option<&Path>) -> Self {
        Self::with_adapter_management(started, adapter_root, None, None)
    }

    fn with_adapter_management(
        started: Instant,
        adapter_root: Option<&Path>,
        adapter_registry_source: Option<String>,
        adapter_install_id: Option<AdapterId>,
    ) -> Self {
        let mut tree = TreeState::new();
        tree.expand(1);
        tree.expand(2);
        tree.set_selected(1);
        let adapter_root = adapter_root.map(Path::to_path_buf);
        let (adapter_rows, adapter_discovery_error) = match adapter_root.as_deref() {
            Some(root) => match adapter_rows_from_root(root) {
                Ok(rows) => (rows, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            },
            None => (Vec::new(), None),
        };
        let (adapter_action_sender, adapter_action_results) = adapter_root
            .as_deref()
            .map(adapter_action_worker)
            .map_or((None, None), |(sender, receiver)| {
                (Some(sender), Some(receiver))
            });
        let (adapter_diagnostics_sender, adapter_diagnostics_results) = if adapter_root.is_some() {
            let (sender, receiver) = mpsc::channel();
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        let mut showcase = Self {
            phase: Phase::Splash,
            started,
            splash_animation: Animation::new(0..24).frame_duration(Duration::from_millis(250)),
            spinner: Spinner::braille(),
            section: Section::Overview,
            language: Language::default(),
            theme: Theme::default(),
            settings: SettingsState::default(),
            color_editor: None,
            focus: FocusState::new([
                LIST_FOCUS,
                TABLE_FOCUS,
                TREE_FOCUS,
                VIEWPORT_FOCUS,
                INPUT_FOCUS,
                AREA_FOCUS,
            ]),
            keymap: showcase_keymap(),
            list: ListState::new(),
            table: TableState::new(),
            tree,
            viewport: ViewportState::new(),
            input: seeded_input(),
            area: TextArea::from("İstanbul\n你好\n🚀  é  ❤️\n👨‍👩‍👧‍👦  🇹🇷"),
            adapter_root,
            adapter_registry_source,
            adapter_install_id,
            adapter_rows,
            adapter_browser_mode: AdapterBrowserMode::default(),
            adapter_discovery_error,
            adapter_diagnostics: BTreeMap::new(),
            adapter_controller_error: None,
            adapter_diagnostics_sender,
            adapter_diagnostics_results,
            adapter_diagnostics_refresh_in_flight: false,
            last_adapter_diagnostics_refresh: started,
            adapter_action_sender,
            adapter_action_results,
            adapter_action_status: None,
            adapter_remove_confirmation: false,
            palette: None,
            modal_open: false,
            focus_before_modal: None,
            animation_enabled: true,
            hits: HitRegions::default(),
        };
        showcase.refresh_adapter_diagnostics();
        showcase
    }

    fn refresh_adapter_diagnostics(&mut self) {
        if self.adapter_diagnostics_refresh_in_flight {
            return;
        }
        let Some(root) = self.adapter_root.clone() else {
            return;
        };
        let Some(sender) = self.adapter_diagnostics_sender.as_ref().cloned() else {
            return;
        };
        let adapter_ids = self
            .adapter_rows
            .iter()
            .filter_map(|row| {
                AdapterId::new(&row.id)
                    .ok()
                    .map(|adapter_id| (row.id.clone(), adapter_id))
            })
            .collect::<Vec<_>>();
        self.adapter_diagnostics_refresh_in_flight = true;
        self.last_adapter_diagnostics_refresh = Instant::now();
        thread::spawn(move || {
            let mut diagnostics = BTreeMap::new();
            let mut controller_error = None;
            for (id, adapter_id) in adapter_ids {
                match local_controller_diagnostics(&root, &adapter_id) {
                    Ok(Some(detail)) => {
                        diagnostics.insert(id, detail);
                    }
                    Ok(None) => {}
                    Err(error) if controller_error.is_none() => {
                        controller_error = Some(error.to_string());
                    }
                    Err(_) => {}
                }
            }
            let _ = sender.send((diagnostics, controller_error));
        });
    }

    fn drain_adapter_diagnostics_results(&mut self) -> bool {
        let completed = self
            .adapter_diagnostics_results
            .as_ref()
            .and_then(|results| std::iter::from_fn(|| results.try_recv().ok()).last());
        let Some((diagnostics, controller_error)) = completed else {
            return false;
        };
        self.adapter_diagnostics = diagnostics;
        self.adapter_controller_error = controller_error;
        self.adapter_diagnostics_refresh_in_flight = false;
        true
    }

    fn selected_adapter_id(&mut self) -> Option<AdapterId> {
        self.table
            .selected_index(self.adapter_rows.len())
            .and_then(|index| self.adapter_rows.get(index))
            .and_then(|row| AdapterId::new(&row.id).ok())
    }

    fn capability_provider_index(&self) -> BTreeMap<String, Vec<CapabilityProvider>> {
        capability_provider_index(&self.adapter_rows, &self.adapter_diagnostics)
    }

    fn queue_adapter_action(&mut self, action: AdapterManagementAction) {
        let Some(sender) = &self.adapter_action_sender else {
            self.adapter_action_status = Some("Adapter root is required for actions".to_owned());
            return;
        };
        match sender.send(action) {
            Ok(()) => self.adapter_action_status = Some("Adapter action in progress…".to_owned()),
            Err(error) => {
                self.adapter_action_status = Some(format!("Adapter worker unavailable: {error}"))
            }
        }
    }

    fn drain_adapter_action_results(&mut self) -> bool {
        let mut changed = false;
        let completed = self
            .adapter_action_results
            .as_ref()
            .map(|results| std::iter::from_fn(|| results.try_recv().ok()).collect::<Vec<_>>())
            .unwrap_or_default();
        for result in completed {
            self.adapter_action_status = Some(match result {
                Ok(outcome) => format!("Completed: {outcome}"),
                Err(error) => format!("Failed: {error}"),
            });
            if let Some(root) = self.adapter_root.as_deref() {
                match adapter_rows_from_root(root) {
                    Ok(rows) => {
                        self.adapter_rows = rows;
                        self.adapter_discovery_error = None;
                    }
                    Err(error) => self.adapter_discovery_error = Some(error.to_string()),
                }
            }
            self.refresh_adapter_diagnostics();
            changed = true;
        }
        changed
    }

    fn advance(&mut self, now: Instant) -> bool {
        match self.phase {
            Phase::Splash => {
                let changed = self.splash_animation.update(now);
                if now.saturating_duration_since(self.started) >= SPLASH_DURATION {
                    self.phase = Phase::Showcase;
                    true
                } else {
                    changed
                }
            }
            Phase::Showcase => {
                let mut changed = self.drain_adapter_diagnostics_results();
                changed |= self.drain_adapter_action_results();
                changed |= self.animation_enabled && self.spinner.update(now);
                if self.section == Section::Adapters
                    && now.saturating_duration_since(self.last_adapter_diagnostics_refresh)
                        >= Duration::from_secs(1)
                {
                    self.refresh_adapter_diagnostics();
                    changed = true;
                }
                changed
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if matches!(key.code, KeyCode::Char(character) if is_quit_key(character))
            || (key.modifiers.ctrl && matches!(key.code, KeyCode::Char('c' | 'C')))
        {
            return Outcome {
                quit: true,
                redraw: false,
            };
        }

        if self.phase == Phase::Splash {
            if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                self.phase = Phase::Showcase;
                return Outcome {
                    quit: false,
                    redraw: true,
                };
            }
            return Outcome::default();
        }

        if self.color_editor.is_some() {
            return self.handle_color_editor_key(key);
        }

        if self.modal_open {
            if matches!(key.code, KeyCode::Escape | KeyCode::Enter) {
                self.close_modal();
                return Outcome {
                    quit: false,
                    redraw: true,
                };
            }
            return Outcome::default();
        }

        if self.palette.is_some() {
            return self.handle_palette_key(key);
        }

        if self.section == Section::Adapters {
            if self.adapter_browser_mode == AdapterBrowserMode::Capabilities {
                if matches!(key.code, KeyCode::Char('c' | 'C') | KeyCode::Escape) {
                    self.adapter_browser_mode = AdapterBrowserMode::Adapters;
                    self.table.set_selected(0);
                    self.focus.set_focus(TABLE_FOCUS);
                    return Outcome {
                        quit: false,
                        redraw: true,
                    };
                }
                if matches!(
                    key.code,
                    KeyCode::Char(
                        'i' | 'I' | 's' | 'S' | 't' | 'T' | 'r' | 'R' | 'u' | 'U' | 'x' | 'X'
                    )
                ) {
                    return Outcome::default();
                }
            } else if matches!(key.code, KeyCode::Char('c' | 'C')) {
                self.adapter_browser_mode = AdapterBrowserMode::Capabilities;
                self.table.set_selected(0);
                self.focus.set_focus(TABLE_FOCUS);
                return Outcome {
                    quit: false,
                    redraw: true,
                };
            }
            if self.adapter_remove_confirmation {
                match key.code {
                    KeyCode::Enter => {
                        self.adapter_remove_confirmation = false;
                        if let Some(id) = self.selected_adapter_id() {
                            self.queue_adapter_action(AdapterManagementAction::Remove { id });
                        }
                        return Outcome {
                            quit: false,
                            redraw: true,
                        };
                    }
                    KeyCode::Escape => {
                        self.adapter_remove_confirmation = false;
                        return Outcome {
                            quit: false,
                            redraw: true,
                        };
                    }
                    _ => return Outcome::default(),
                }
            }
            let action = match key.code {
                KeyCode::Char('i' | 'I') => self
                    .adapter_install_id
                    .clone()
                    .zip(self.adapter_registry_source.clone())
                    .map(|(id, registry_source)| AdapterManagementAction::Install {
                        id,
                        registry_source,
                        version: None,
                    }),
                KeyCode::Char('s' | 'S') => self
                    .selected_adapter_id()
                    .map(|id| AdapterManagementAction::Start { id }),
                KeyCode::Char('t' | 'T') => self
                    .selected_adapter_id()
                    .map(|id| AdapterManagementAction::Stop { id }),
                KeyCode::Char('r' | 'R') => self
                    .selected_adapter_id()
                    .map(|id| AdapterManagementAction::Restart { id }),
                KeyCode::Char('u' | 'U') => self
                    .selected_adapter_id()
                    .zip(self.adapter_registry_source.clone())
                    .map(|(id, registry_source)| AdapterManagementAction::Update {
                        id,
                        registry_source,
                    }),
                KeyCode::Char('x' | 'X') => {
                    self.adapter_remove_confirmation = self.selected_adapter_id().is_some();
                    return Outcome {
                        quit: false,
                        redraw: true,
                    };
                }
                _ => None,
            };
            if let Some(action) = action {
                self.queue_adapter_action(action);
                return Outcome {
                    quit: false,
                    redraw: true,
                };
            }
        }

        let section = match key.code {
            KeyCode::Char(character) => Section::from_number(character),
            _ => None,
        };
        if let Some(section) = section {
            self.select_section(section);
            return Outcome {
                quit: false,
                redraw: true,
            };
        }

        if matches!(key.code, KeyCode::Char('m' | 'M')) {
            self.open_modal();
            return Outcome {
                quit: false,
                redraw: true,
            };
        }

        if let Some(command) = self.keymap.resolve(key).cloned() {
            return self.execute_command(command);
        }

        if self.section == Section::Settings && self.handle_settings_key(key) {
            return Outcome {
                quit: false,
                redraw: true,
            };
        }

        let redraw = match self.focus.current() {
            Some(LIST_FOCUS) => match key.code {
                KeyCode::Up => {
                    self.list.previous(section_list_items(self.section).len());
                    true
                }
                KeyCode::Down => {
                    self.list.next(section_list_items(self.section).len());
                    true
                }
                _ => false,
            },
            Some(TABLE_FOCUS) => match key.code {
                KeyCode::Up => {
                    self.table.previous(self.active_table_row_count());
                    true
                }
                KeyCode::Down => {
                    self.table.next(self.active_table_row_count());
                    true
                }
                KeyCode::PageUp => self.table.page_up(),
                KeyCode::PageDown => self.table.page_down(),
                _ => false,
            },
            Some(TREE_FOCUS) => match key.code {
                KeyCode::Up => showcase_tree().move_up(&mut self.tree),
                KeyCode::Down => showcase_tree().move_down(&mut self.tree),
                KeyCode::Left => showcase_tree().move_left(&mut self.tree),
                KeyCode::Right => showcase_tree().move_right(&mut self.tree),
                KeyCode::Enter => showcase_tree().toggle(&mut self.tree),
                _ => false,
            },
            Some(VIEWPORT_FOCUS) => match key.code {
                KeyCode::Up => self.viewport.scroll_up(),
                KeyCode::Down => self.viewport.scroll_down(),
                KeyCode::PageUp => self.viewport.page_up(),
                KeyCode::PageDown => self.viewport.page_down(),
                KeyCode::Home => self.viewport.home(),
                KeyCode::End => self.viewport.end(),
                _ => false,
            },
            Some(INPUT_FOCUS) => self.input.handle_key(key),
            Some(AREA_FOCUS) => self.area.handle_key(key),
            _ => false,
        };
        Outcome {
            quit: false,
            redraw,
        }
    }

    fn handle_palette_key(&mut self, key: KeyEvent) -> Outcome {
        if matches!(key.code, KeyCode::Escape) {
            self.palette = None;
            return Outcome {
                quit: false,
                redraw: true,
            };
        }
        if matches!(key.code, KeyCode::Enter) {
            let command = self
                .palette
                .as_ref()
                .and_then(CommandPalette::execute_selected);
            self.palette = None;
            return command.map_or(
                Outcome {
                    quit: false,
                    redraw: true,
                },
                |command| self.execute_command(command),
            );
        }
        let redraw = self
            .palette
            .as_mut()
            .is_some_and(|palette| palette.handle_key(key));
        Outcome {
            quit: false,
            redraw,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Outcome {
        if self.phase == Phase::Splash {
            return Outcome::default();
        }
        let point = Position {
            x: mouse.x,
            y: mouse.y,
        };
        if self.palette.is_some() {
            if mouse.kind == MouseKind::LeftDown && self.hits.palette_rows.contains(point) {
                let row = usize::from(mouse.y.saturating_sub(self.hits.palette_rows.y));
                let command = self.palette.as_mut().and_then(|palette| {
                    for _ in 0..row {
                        let _ = palette.handle_key(KeyEvent {
                            code: KeyCode::Down,
                            modifiers: KeyModifiers::default(),
                        });
                    }
                    palette.execute_selected()
                });
                self.palette = None;
                return command.map_or(
                    Outcome {
                        quit: false,
                        redraw: true,
                    },
                    |command| self.execute_command(command),
                );
            }
            return Outcome::default();
        }
        if self.color_editor.is_some() {
            return self.handle_color_editor_mouse(mouse, point);
        }
        if self.modal_open {
            if mouse.kind == MouseKind::LeftDown && self.hits.modal.contains(point) {
                self.close_modal();
                return Outcome {
                    quit: false,
                    redraw: true,
                };
            }
            return Outcome::default();
        }

        if mouse.kind == MouseKind::LeftDown {
            for section in Section::ALL {
                if self.hits.sections[section.index()].contains(point) {
                    self.select_section(section);
                    return Outcome {
                        quit: false,
                        redraw: true,
                    };
                }
            }
            if self.section == Section::Settings {
                for (index, row) in SettingRow::ALL.into_iter().enumerate() {
                    if self.hits.settings_rows[index].contains(point) {
                        self.settings.selected = row;
                        self.activate_setting();
                        return Outcome {
                            quit: false,
                            redraw: true,
                        };
                    }
                }
            }
        }

        match mouse.kind {
            MouseKind::LeftDown if self.hits.list.contains(point) => {
                self.focus.set_focus(LIST_FOCUS);
                self.list
                    .set_selected(usize::from(mouse.y.saturating_sub(self.hits.list.y)));
                Outcome {
                    quit: false,
                    redraw: true,
                }
            }
            MouseKind::LeftDown if self.hits.table.contains(point) => {
                self.focus.set_focus(TABLE_FOCUS);
                self.table
                    .set_selected(usize::from(mouse.y.saturating_sub(self.hits.table.y)));
                Outcome {
                    quit: false,
                    redraw: true,
                }
            }
            MouseKind::LeftDown if self.hits.tree.contains(point) => {
                self.focus.set_focus(TREE_FOCUS);
                let id = match mouse.y.saturating_sub(self.hits.tree.y) {
                    0 => 1,
                    1 => 2,
                    2 => 3,
                    3 => 4,
                    _ => 5,
                };
                self.tree.set_selected(id);
                Outcome {
                    quit: false,
                    redraw: true,
                }
            }
            MouseKind::LeftDown if self.hits.input.contains(point) => Outcome {
                quit: false,
                redraw: self.focus.set_focus(INPUT_FOCUS),
            },
            MouseKind::LeftDown if self.hits.area.contains(point) => Outcome {
                quit: false,
                redraw: self.focus.set_focus(AREA_FOCUS),
            },
            MouseKind::ScrollUp if self.hits.viewport.contains(point) => Outcome {
                quit: false,
                redraw: self.viewport.scroll_up(),
            },
            MouseKind::ScrollDown if self.hits.viewport.contains(point) => Outcome {
                quit: false,
                redraw: self.viewport.scroll_down(),
            },
            _ => Outcome::default(),
        }
    }

    fn open_palette(&mut self) {
        self.palette = Some(CommandPalette::new(palette_commands(self.language)));
    }

    fn open_modal(&mut self) {
        self.focus_before_modal = self.focus.current();
        self.modal_open = true;
    }

    fn close_modal(&mut self) {
        self.modal_open = false;
        if let Some(focus) = self.focus_before_modal.take() {
            self.focus.set_focus(focus);
        }
    }

    fn select_section(&mut self, section: Section) {
        self.section = section;
        if section == Section::Adapters {
            self.refresh_adapter_diagnostics();
        }
        let focus = match section {
            Section::Widgets => LIST_FOCUS,
            Section::Data => TABLE_FOCUS,
            Section::Graphics
            | Section::Overview
            | Section::Interaction
            | Section::Settings
            | Section::Adapters => VIEWPORT_FOCUS,
            Section::Input => INPUT_FOCUS,
        };
        self.focus.set_focus(focus);
    }

    fn active_table_row_count(&self) -> usize {
        match self.section {
            Section::Adapters if self.adapter_browser_mode == AdapterBrowserMode::Capabilities => {
                self.capability_provider_index().len()
            }
            Section::Adapters => self.adapter_rows.len(),
            _ => table_rows().len(),
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                self.settings.selected = self.settings.selected.next(-1);
                true
            }
            KeyCode::Down => {
                self.settings.selected = self.settings.selected.next(1);
                true
            }
            KeyCode::Left | KeyCode::Right
                if matches!(
                    self.settings.selected,
                    SettingRow::English | SettingRow::Turkish
                ) =>
            {
                self.language = match self.language {
                    Language::English => Language::Turkish,
                    Language::Turkish => Language::English,
                };
                true
            }
            KeyCode::Enter => {
                self.activate_setting();
                true
            }
            _ => false,
        }
    }

    fn activate_setting(&mut self) {
        match self.settings.selected {
            SettingRow::English => self.language = Language::English,
            SettingRow::Turkish => self.language = Language::Turkish,
            SettingRow::Color(slot) => {
                self.color_editor = Some(ColorEditor::new(slot, slot.color(self.theme)));
            }
            SettingRow::Reset => self.theme = Theme::default(),
        }
    }

    fn handle_color_editor_key(&mut self, key: KeyEvent) -> Outcome {
        let Some(editor) = self.color_editor.as_mut() else {
            return Outcome::default();
        };
        match key.code {
            KeyCode::Escape => self.color_editor = None,
            KeyCode::Enter => {
                editor.slot.set(&mut self.theme, editor.color());
                self.color_editor = None;
            }
            KeyCode::Up => editor.channel = editor.channel.saturating_sub(1),
            KeyCode::Down => editor.channel = (editor.channel + 1).min(2),
            _ => {
                let _ = editor.inputs[editor.channel].handle_key(key);
            }
        }
        Outcome {
            quit: false,
            redraw: true,
        }
    }

    fn handle_color_editor_mouse(&mut self, mouse: MouseEvent, point: Position) -> Outcome {
        if mouse.kind != MouseKind::LeftDown {
            return Outcome::default();
        }
        if self.hits.color_apply.contains(point) {
            if let Some(editor) = self.color_editor.take() {
                editor.slot.set(&mut self.theme, editor.color());
            }
        } else if let (Some((channel, _)), Some(editor)) = (
            self.hits
                .color_inputs
                .iter()
                .enumerate()
                .find(|(_, rect)| rect.contains(point)),
            self.color_editor.as_mut(),
        ) {
            editor.channel = channel;
        }
        Outcome {
            quit: false,
            redraw: true,
        }
    }

    fn execute_command(&mut self, command: CommandId) -> Outcome {
        for section in Section::ALL {
            if command.as_str() == section.command() {
                self.select_section(section);
                return Outcome {
                    quit: false,
                    redraw: true,
                };
            }
        }
        match command.as_str() {
            "focus-next" => Outcome {
                quit: false,
                redraw: self.focus.focus_next(),
            },
            "focus-previous" => Outcome {
                quit: false,
                redraw: self.focus.focus_previous(),
            },
            "command-palette" => {
                self.open_palette();
                Outcome {
                    quit: false,
                    redraw: true,
                }
            }
            "open-modal" => {
                self.open_modal();
                Outcome {
                    quit: false,
                    redraw: true,
                }
            }
            "toggle-animation" => {
                self.animation_enabled = !self.animation_enabled;
                Outcome {
                    quit: false,
                    redraw: true,
                }
            }
            "quit" => Outcome {
                quit: true,
                redraw: false,
            },
            _ => Outcome::default(),
        }
    }
}

fn seeded_input() -> TextInput {
    let mut input = TextInput::new();
    for character in "Type here: İstanbul 🚀".chars() {
        input.insert(character);
    }
    input
}

fn showcase_keymap() -> KeyMap {
    let mut keymap = KeyMap::new();
    keymap.bind(
        KeyCode::Tab,
        KeyModifiers::default(),
        CommandId::new("focus-next"),
    );
    keymap.bind(
        KeyCode::Tab,
        KeyModifiers {
            shift: true,
            ..KeyModifiers::default()
        },
        CommandId::new("focus-previous"),
    );
    keymap.bind(
        KeyCode::Char('p'),
        KeyModifiers {
            ctrl: true,
            ..KeyModifiers::default()
        },
        CommandId::new("command-palette"),
    );
    keymap
}

fn palette_commands(language: Language) -> Vec<PaletteCommand> {
    let mut commands = Section::ALL
        .into_iter()
        .map(|section| {
            PaletteCommand::new(
                CommandId::new(section.command()),
                if section == Section::Settings || section == Section::Adapters {
                    match section {
                        Section::Settings => localized(language, "Open Settings", "Ayarları Aç"),
                        Section::Adapters => localized(language, "Open Adapters", "Adaptörleri Aç"),
                        _ => unreachable!("only special palette sections reach this branch"),
                    }
                    .to_owned()
                } else {
                    format!(
                        "{} {}",
                        localized(language, "Show", "Göster"),
                        section.title(language)
                    )
                },
            )
        })
        .collect::<Vec<_>>();
    commands.extend([
        PaletteCommand::new(
            CommandId::new("open-modal"),
            localized(language, "Open Demo Modal", "Demo Penceresini Aç"),
        ),
        PaletteCommand::new(
            CommandId::new("toggle-animation"),
            localized(language, "Toggle Animation", "Animasyonu Değiştir"),
        ),
        PaletteCommand::new(CommandId::new("quit"), localized(language, "Quit", "Çık")),
    ]);
    commands
}

fn section_list_items(section: Section) -> &'static [&'static str] {
    match section {
        Section::Overview => &["Immediate mode", "Explicit state", "Low-level escape hatch"],
        Section::Widgets => &["Text + RichText", "List / Table", "Tree / Viewport"],
        Section::Data => &["Hermes", "Codex", "Claude"],
        Section::Graphics => &["Progress", "Gauge", "Sparkline", "Canvas"],
        Section::Input => &["TextInput", "TextArea", "Graphemes"],
        Section::Interaction => &["Focus", "Mouse", "Palette", "Modal"],
        Section::Settings => &["Language", "Theme", "RGB editor"],
        Section::Adapters => &["Installed", "Available", "Capabilities"],
    }
}

fn table_rows() -> Vec<Vec<Line>> {
    vec![
        vec![
            Line::new([Span::styled("Hermes", Style::new().bold())]),
            Line::from("Working"),
            Line::from("12.4K"),
            Line::from("32s"),
        ],
        vec![
            Line::from("Codex"),
            Line::from("Ready"),
            Line::from("8.1K"),
            Line::from("12s"),
        ],
        vec![
            Line::from("Claude"),
            Line::from("Review"),
            Line::from("15.0K"),
            Line::from("21s"),
        ],
    ]
}

fn showcase_table(theme: Theme) -> Table {
    Table::new([
        TableColumn::new(Constraint::Fill(2)),
        TableColumn::new(Constraint::Fill(2)),
        TableColumn::new(Constraint::Fill(1)).alignment(Alignment::Right),
        TableColumn::new(Constraint::Fill(1)).alignment(Alignment::Right),
    ])
    .header([
        Line::from("NAME"),
        Line::from("STATUS"),
        Line::from("TOKENS"),
        Line::from("TIME"),
    ])
    .rows(table_rows())
    .selected_style(Style::new().fg(theme.success).bg(theme.primary).bold())
}

fn showcase_tree() -> Tree {
    Tree::new([TreeNode::new(1, "DragonsTUI").children([
        TreeNode::new(2, "src").children([
            TreeNode::new(3, "runtime.rs"),
            TreeNode::new(4, "frame.rs"),
            TreeNode::new(5, "widgets").children([
                TreeNode::new(6, "table.rs"),
                TreeNode::new(7, "tree.rs"),
                TreeNode::new(8, "textarea.rs"),
            ]),
        ]),
        TreeNode::new(9, "examples").children([
            TreeNode::new(10, "hello.rs"),
            TreeNode::new(11, "animation.rs"),
        ]),
    ])])
}

fn output_lines() -> Vec<String> {
    [
        "[000] renderer: changed-cell runs enabled",
        "[001] input: grapheme-safe editing ready",
        "[002] canvas: braille waveform sampled",
        "[003] mouse: SGR events normalized",
        "[004] palette: command routing explicit",
        "[005] terminal: no live CPU or FPS claim",
        "[006] showcase data is deterministic",
        "[007] viewport scroll is caller-owned state",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn main() -> io::Result<()> {
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!(
            "Dragonfire showcase for DragonsTUI\n\nRun: cargo run --release --features adapter-showcase --bin dragonstui-showcase [--adapter-root <path>]"
        );
        return Ok(());
    }
    let adapter_root = parse_showcase_args()?;
    let shutdown = ShutdownSignal::install()?;

    let mut output = stdout();
    let mut terminal = TerminalGuard::enter(&mut output)?;
    let result = run(&mut output, adapter_root, &shutdown);
    let restore = terminal.restore(&mut output);
    result.and(restore)
}

struct ShowcaseArgs {
    adapter_root: Option<PathBuf>,
    adapter_registry_source: Option<String>,
    adapter_install_id: Option<AdapterId>,
}

fn parse_showcase_args() -> io::Result<ShowcaseArgs> {
    let mut arguments = std::env::args().skip(1);
    let mut adapter_root = None;
    let mut adapter_registry_source = None;
    let mut adapter_install_id = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--adapter-root" => {
                let value = arguments.next().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--adapter-root requires a path",
                    )
                })?;
                adapter_root = Some(PathBuf::from(value));
            }
            "--adapter-registry" => {
                adapter_registry_source = Some(arguments.next().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--adapter-registry requires a source",
                    )
                })?);
            }
            "--adapter-install" => {
                let value = arguments.next().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--adapter-install requires an adapter id",
                    )
                })?;
                adapter_install_id = Some(AdapterId::new(value).map_err(|error| {
                    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
                })?);
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown showcase argument: {argument}"),
                ));
            }
        }
    }
    Ok(ShowcaseArgs {
        adapter_root,
        adapter_registry_source,
        adapter_install_id,
    })
}

fn run(output: &mut impl Write, args: ShowcaseArgs, shutdown: &ShutdownSignal) -> io::Result<()> {
    let mut runtime = Runtime::new(Some(TICK_INTERVAL));
    let mut showcase = Showcase::with_adapter_management(
        Instant::now(),
        args.adapter_root.as_deref(),
        args.adapter_registry_source,
        args.adapter_install_id,
    );
    loop {
        if shutdown.requested() {
            return Ok(());
        }
        if runtime.needs_redraw() {
            let view = showcase_view(terminal_size()?, &mut showcase);
            runtime.render_with_cursor(output, view.frame, view.cursor)?;
        }
        let event = runtime.next_event()?;
        if shutdown.requested() {
            return Ok(());
        }
        match event {
            Event::Key(key) => {
                let outcome = showcase.handle_key(key);
                if outcome.quit {
                    return Ok(());
                }
                if outcome.redraw {
                    runtime.request_redraw();
                }
            }
            Event::Mouse(mouse) => {
                if showcase.handle_mouse(mouse).redraw {
                    runtime.request_redraw();
                }
            }
            Event::Resize(_) => runtime.request_redraw(),
            Event::Tick(now) => {
                if showcase.advance(now) {
                    runtime.request_redraw();
                }
            }
        }
    }
}

struct View {
    frame: Frame,
    cursor: Option<Position>,
}

fn showcase_view(size: Size, showcase: &mut Showcase) -> View {
    let mut frame = Frame::new(size.width, size.height);
    fill_background(&mut frame, showcase.theme);
    showcase.hits = HitRegions::default();
    if showcase.phase == Phase::Splash {
        render_splash(&mut frame, size, showcase);
        return View {
            frame,
            cursor: None,
        };
    }
    let cursor = render_showcase(&mut frame, size, showcase);
    View { frame, cursor }
}

fn fill_background(frame: &mut Frame, theme: Theme) {
    let style = Style::new().bg(theme.background);
    for y in 0..frame.buffer().height() {
        for x in 0..frame.buffer().width() {
            frame.set_cell(x, y, Cell::new(' ', style));
        }
    }
}

fn render_splash(frame: &mut Frame, size: Size, showcase: &Showcase) {
    let phase = usize::from(showcase.splash_animation.current().copied().unwrap_or(0));
    let layout = splash_layout(size);
    let border_style = Style::new()
        .fg(showcase.theme.secondary)
        .bg(showcase.theme.background);
    frame.draw_border_with_set(layout.frame, border_style, BorderSet::rounded());

    for (offset, line) in SPLASH_TITLE.iter().enumerate() {
        render_splash_gradient_line(
            frame,
            size,
            Position {
                x: layout.title_origin,
                y: layout
                    .title_y
                    .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX)),
            },
            line,
            phase,
            showcase.theme,
            true,
        );
    }
    for (offset, line) in SPLASH_DRAGON.iter().enumerate() {
        render_splash_gradient_line(
            frame,
            size,
            Position {
                x: layout.dragon_origin,
                y: layout
                    .dragon_y
                    .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX)),
            },
            line,
            phase,
            showcase.theme,
            false,
        );
    }
    render_splash_separator(frame, layout.frame, layout.before_manifesto_y, border_style);
    for (index, line) in SPLASH_MANIFESTO.iter().enumerate() {
        let style = if index == 0 {
            Style::new()
                .fg(showcase.theme.text)
                .bg(showcase.theme.background)
        } else {
            Style::new()
                .fg(showcase.theme.muted)
                .bg(showcase.theme.background)
        };
        render_splash_text(
            frame,
            size,
            layout
                .manifesto_y
                .saturating_add(u16::try_from(index).unwrap_or(u16::MAX)),
            splash_block_origin(size, &[*line]),
            line,
            style,
        );
    }
    render_splash_separator(frame, layout.frame, layout.after_manifesto_y, border_style);

    let loading = LOADING_TEXT[(phase / 2) % LOADING_TEXT.len()];
    render_splash_text(
        frame,
        size,
        layout.loading_y,
        splash_block_origin(size, &LOADING_TEXT),
        loading,
        Style::new()
            .fg(showcase.theme.success)
            .bg(showcase.theme.background)
            .bold(),
    );
    render_loading_bar(
        frame,
        size,
        layout.loading_y.saturating_add(1),
        layout.loading_bar_x,
        phase,
        showcase.theme,
    );
    render_splash_text(
        frame,
        size,
        layout.loading_y.saturating_add(2),
        splash_block_origin(size, &[LOADING_HINT]),
        LOADING_HINT,
        Style::new()
            .fg(showcase.theme.muted)
            .bg(showcase.theme.background),
    );
}

const SPLASH_HORIZONTAL_PADDING: u16 = 3;
const SPLASH_VERTICAL_PADDING: u16 = 1;
const SPLASH_LOADING_ROWS: u16 = 3;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct VisibleBounds {
    min_x: u16,
    max_x: u16,
    width: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SplashLayout {
    center_x: u16,
    frame: Rect,
    title_origin: u16,
    dragon_origin: u16,
    manifesto_origin: u16,
    loading_origin: u16,
    loading_bar_x: u16,
    title_y: u16,
    dragon_y: u16,
    before_manifesto_y: u16,
    manifesto_y: u16,
    after_manifesto_y: u16,
    loading_y: u16,
}

fn splash_layout(size: Size) -> SplashLayout {
    let center_x = size.width / 2;
    let title = visible_content_bounds(&SPLASH_TITLE).unwrap_or_default();
    let dragon = visible_content_bounds(&SPLASH_DRAGON).unwrap_or_default();
    let manifesto = visible_content_bounds(&SPLASH_MANIFESTO).unwrap_or_default();
    let loading = visible_content_bounds(&[LOADING_TEXT[2], LOADING_HINT]).unwrap_or_default();
    let content_width = title
        .width
        .max(dragon.width)
        .max(manifesto.width)
        .max(loading.width)
        .max(10);
    let requested_width = content_width.saturating_add(
        SPLASH_HORIZONTAL_PADDING
            .saturating_mul(2)
            .saturating_add(2),
    );
    let frame_width = requested_width.min(size.width);
    let frame_x = center_x.saturating_sub(frame_width / 2);
    let requested_height =
        u16::try_from(SPLASH_TITLE.len() + SPLASH_DRAGON.len() + SPLASH_MANIFESTO.len())
            .unwrap_or(u16::MAX)
            .saturating_add(SPLASH_LOADING_ROWS)
            .saturating_add(SPLASH_VERTICAL_PADDING.saturating_mul(2))
            .saturating_add(7);
    let frame_height = requested_height.min(size.height);
    let frame_y = size.height.saturating_sub(frame_height) / 2;
    let title_y = frame_y
        .saturating_add(1)
        .saturating_add(SPLASH_VERTICAL_PADDING);
    let dragon_y = title_y
        .saturating_add(u16::try_from(SPLASH_TITLE.len()).unwrap_or(u16::MAX))
        .saturating_add(1);
    let before_manifesto_y = dragon_y
        .saturating_add(u16::try_from(SPLASH_DRAGON.len()).unwrap_or(u16::MAX))
        .saturating_add(1);
    let manifesto_y = before_manifesto_y.saturating_add(1);
    let after_manifesto_y =
        manifesto_y.saturating_add(u16::try_from(SPLASH_MANIFESTO.len()).unwrap_or(u16::MAX));
    let loading_y = after_manifesto_y.saturating_add(2);

    SplashLayout {
        center_x,
        frame: Rect::new(frame_x, frame_y, frame_width, frame_height),
        title_origin: splash_source_origin(center_x, title),
        dragon_origin: splash_source_origin(center_x, dragon),
        manifesto_origin: splash_source_origin(center_x, manifesto),
        loading_origin: splash_source_origin(center_x, loading),
        loading_bar_x: center_x.saturating_sub(5),
        title_y,
        dragon_y,
        before_manifesto_y,
        manifesto_y,
        after_manifesto_y,
        loading_y,
    }
}

fn visible_content_bounds(lines: &[&str]) -> Option<VisibleBounds> {
    let mut min_x = None;
    let mut max_x = None;
    for line in lines {
        let mut column = 0_u16;
        for character in line.chars() {
            let mut encoded = [0; 4];
            let scalar = character.encode_utf8(&mut encoded);
            let width = u16::try_from(display_width(scalar)).unwrap_or(u16::MAX);
            if width > 0 && !character.is_whitespace() && character != '\u{2800}' {
                min_x = Some(min_x.unwrap_or(column).min(column));
                let last_column = column.saturating_add(width).saturating_sub(1);
                max_x = Some(max_x.unwrap_or(last_column).max(last_column));
            }
            column = column.saturating_add(width);
        }
    }
    match (min_x, max_x) {
        (Some(min_x), Some(max_x)) => Some(VisibleBounds {
            min_x,
            max_x,
            width: max_x.saturating_sub(min_x).saturating_add(1),
        }),
        _ => None,
    }
}

fn splash_source_origin(center_x: u16, bounds: VisibleBounds) -> u16 {
    center_x
        .saturating_sub(bounds.width / 2)
        .saturating_sub(bounds.min_x)
}

#[cfg(test)]
fn splash_visible_center(source_origin: u16, bounds: VisibleBounds) -> u16 {
    source_origin
        .saturating_add(bounds.min_x)
        .saturating_add(bounds.width / 2)
}

fn splash_block_origin(size: Size, lines: &[&str]) -> u16 {
    visible_content_bounds(lines).map_or(0, |bounds| splash_source_origin(size.width / 2, bounds))
}

fn render_splash_separator(frame: &mut Frame, frame_rect: Rect, y: u16, style: Style) {
    if frame_rect.width == 0 || y < frame_rect.y || y >= frame_rect.bottom() {
        return;
    }
    frame.set_cell(frame_rect.x, y, Cell::new('├', style));
    for offset in 1..frame_rect.width.saturating_sub(1) {
        frame.set_cell(
            frame_rect.x.saturating_add(offset),
            y,
            Cell::new('─', style),
        );
    }
    if frame_rect.width > 1 {
        frame.set_cell(
            frame_rect.right().saturating_sub(1),
            y,
            Cell::new('┤', style),
        );
    }
}

fn render_splash_text(frame: &mut Frame, size: Size, y: u16, x: u16, line: &str, style: Style) {
    if size.width == 0 || y >= size.height || x >= size.width {
        return;
    }
    frame.write_text_in(Rect::new(0, y, size.width, 1), x, 0, line, style);
}

fn splash_color(theme: Theme, x: u16, y: u16, phase: usize) -> Color {
    let palette = [
        theme.primary,
        theme.error,
        theme.secondary,
        theme.warning,
        theme.success,
        Color::rgb(255, 225, 100),
    ];
    palette[(usize::from(x / 3) + usize::from(y / 2) + phase) % palette.len()]
}

fn render_splash_gradient_line(
    frame: &mut Frame,
    size: Size,
    position: Position,
    line: &str,
    phase: usize,
    theme: Theme,
    bold: bool,
) {
    if size.width == 0 || position.y >= size.height {
        return;
    }
    let mut x = position.x;
    for character in line.chars() {
        let mut encoded = [0; 4];
        let scalar = character.encode_utf8(&mut encoded);
        let width = u16::try_from(display_width(scalar)).unwrap_or(u16::MAX);
        if width == 0 {
            continue;
        }
        if x >= size.width || width > size.width.saturating_sub(x) {
            break;
        }
        if character != ' ' {
            let mut style = Style::new()
                .fg(splash_color(theme, x, position.y, phase))
                .bg(theme.background);
            if bold {
                style = style.bold();
            }
            frame.write_text_in(Rect::new(0, position.y, size.width, 1), x, 0, scalar, style);
        }
        x = x.saturating_add(width);
    }
}

fn render_loading_bar(frame: &mut Frame, size: Size, y: u16, x: u16, phase: usize, theme: Theme) {
    if size.width == 0 || y >= size.height || x >= size.width {
        return;
    }
    let filled = loading_fill(phase);
    for offset in 0..10_u16 {
        if x.saturating_add(offset) >= size.width {
            break;
        }
        let character = if offset < filled { '█' } else { '▒' };
        let style = Style::new()
            .fg(splash_color(theme, x.saturating_add(offset), y, phase))
            .bg(theme.background)
            .bold();
        frame.set_cell(x.saturating_add(offset), y, Cell::new(character, style));
    }
}

fn loading_fill(phase: usize) -> u16 {
    1 + u16::try_from((phase.min(23) * 9) / 23).unwrap_or(9)
}

fn render_showcase(frame: &mut Frame, size: Size, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    if size.width < 12 || size.height < 6 {
        Text::new("DragonsTUI")
            .style(Style::new().fg(theme.warning).bg(theme.background).bold())
            .render(frame, Rect::new(0, 0, size.width, size.height));
        return None;
    }

    let header = Rect::new(0, 0, size.width, 4);
    let footer = Rect::new(0, size.height.saturating_sub(2), size.width, 2);
    let content = Rect::new(0, 4, size.width, size.height.saturating_sub(6));
    render_header(frame, header, showcase);
    let cursor = match showcase.section {
        Section::Overview => render_overview(frame, content, showcase),
        Section::Widgets => render_widgets(frame, content, showcase),
        Section::Data => render_data(frame, content, showcase),
        Section::Graphics => render_graphics(frame, content, showcase),
        Section::Input => render_input(frame, content, showcase),
        Section::Interaction => render_interaction(frame, content, showcase),
        Section::Settings => render_settings(frame, content, showcase),
        Section::Adapters => render_adapters(frame, content, showcase),
    };
    render_footer(frame, footer, showcase);
    render_overlays(frame, size, showcase);
    if showcase.modal_open || showcase.palette.is_some() {
        None
    } else {
        cursor
    }
}

fn render_header(frame: &mut Frame, rect: Rect, showcase: &mut Showcase) {
    let theme = showcase.theme;
    let inner = Panel::new(format!(
        " DragonsTUI Showcase · v{} ",
        env!("CARGO_PKG_VERSION")
    ))
    .border_set(BorderSet::rounded())
    .border_style(Style::new().fg(theme.secondary).bg(theme.background).bold())
    .title_style(Style::new().fg(theme.success).bg(theme.background).bold())
    .render(frame, rect);
    let activity = if showcase.animation_enabled {
        format!("{} Dragonfire immediate mode", showcase.spinner.current())
    } else {
        "◇ Dragonfire immediate mode".to_owned()
    };
    Text::new(activity)
        .alignment(Alignment::Right)
        .style(Style::new().fg(theme.text).bg(theme.background))
        .render(frame, Rect::new(inner.x, inner.y, inner.width, 1));

    if inner.height < 2 {
        return;
    }

    let navigation_y = inner.y.saturating_add(1);
    let navigation_right = inner.x.saturating_add(inner.width);
    let mut x = inner.x;
    for (index, section) in Section::ALL.into_iter().enumerate() {
        if x >= navigation_right {
            break;
        }
        let label = format!("{} {}", index + 1, section.title(showcase.language));
        let label_width = u16::try_from(display_width(&label)).unwrap_or(u16::MAX);
        let visible_width = label_width.min(navigation_right.saturating_sub(x));
        if visible_width == 0 {
            break;
        }
        let item = Rect::new(x, navigation_y, visible_width, 1);
        if visible_width == label_width {
            showcase.hits.sections[section.index()] = item;
        }
        let style = if showcase.section == section {
            Style::new().fg(theme.success).bg(theme.primary).bold()
        } else {
            Style::new().fg(theme.warning).bg(theme.background)
        };
        Text::new(label).style(style).render(frame, item);
        x = x.saturating_add(label_width).saturating_add(1);
    }
}

fn render_footer(frame: &mut Frame, rect: Rect, showcase: &Showcase) {
    let theme = showcase.theme;
    let hint = match showcase.section {
        Section::Input => localized(
            showcase.language,
            "Tab focus · type/edit · Ctrl+P commands · m modal · q quit",
            "Tab odağı · yaz/düzenle · Ctrl+P komutlar · m pencere · q çıkış",
        ),
        Section::Widgets | Section::Data => localized(
            showcase.language,
            "Tab focus · ↑↓ navigate · Enter tree · Ctrl+P commands · q quit",
            "Tab odağı · ↑↓ gezin · Enter ağaç · Ctrl+P komutlar · q çıkış",
        ),
        Section::Adapters if showcase.adapter_browser_mode == AdapterBrowserMode::Capabilities => {
            localized(
                showcase.language,
                "C/Esc adapters · ↑↓ capabilities · Ctrl+P commands · q quit",
                "C/Esc adaptörler · ↑↓ yetenekler · Ctrl+P komutlar · q çıkış",
            )
        }
        Section::Adapters => localized(
            showcase.language,
            "C capabilities · Tab focus · Ctrl+P commands · q quit",
            "C yetenekler · Tab odağı · Ctrl+P komutlar · q çıkış",
        ),
        _ => localized(
            showcase.language,
            "1–8 sections · Tab focus · Ctrl+P commands · m modal · q quit",
            "1–8 bölümler · Tab odağı · Ctrl+P komutlar · m pencere · q çıkış",
        ),
    };
    Text::new(hint)
        .style(Style::new().fg(theme.muted).bg(theme.background).dim())
        .render(frame, rect);
}

fn panel(
    frame: &mut Frame,
    rect: Rect,
    title: &str,
    focused: bool,
    theme: Theme,
    set: BorderSet,
) -> Rect {
    let border = if focused {
        Style::new().fg(theme.success).bg(theme.background).bold()
    } else {
        Style::new().fg(theme.muted).bg(theme.background)
    };
    Panel::new(title)
        .border_set(set)
        .border_style(border)
        .title_style(Style::new().fg(theme.warning).bg(theme.background).bold())
        .render(frame, rect)
}

fn render_overview(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    let columns = dragons_tui::Layout::horizontal(vec![Constraint::Fill(3), Constraint::Fill(2)])
        .gap(1)
        .split(area);
    let (left, right) = columns.first().zip(columns.get(1))?;
    let inner = panel(
        frame,
        *left,
        localized(showcase.language, "Overview", "Genel Bakış"),
        false,
        theme,
        BorderSet::rounded(),
    );
    RichText::new([
        Line::new([
            Span::styled(
                localized(showcase.language, "Explicit ", "Açık "),
                Style::new().fg(theme.secondary).bold(),
            ),
            Span::styled(
                localized(
                    showcase.language,
                    "immediate-mode rendering",
                    "immediate-mode render",
                ),
                Style::new().fg(theme.text),
            ),
        ]),
        Line::new([
            Span::styled("M20 ", Style::new().fg(theme.success).bold()),
            Span::styled(
                localized(
                    showcase.language,
                    "full-frame encoder runs: 69% reduction on the measured local machine",
                    "tam-kare encoder: ölçülen yerel makinede %69 azaltma",
                ),
                Style::new().fg(theme.muted),
            ),
        ]),
        Line::new([
            Span::styled("M19 ", Style::new().fg(theme.warning).underline()),
            Span::styled(
                localized(
                    showcase.language,
                    "no component tree · no virtual DOM",
                    "component tree yok · virtual DOM yok",
                ),
                Style::new().fg(theme.text).italic(),
            ),
        ]),
    ])
    .render(frame, inner);

    let metrics = panel(
        frame,
        *right,
        localized(showcase.language, "Runtime", "Çalışma Zamanı"),
        false,
        theme,
        BorderSet::square(),
    );
    Text::new(localized(
        showcase.language,
        "Static benchmark context",
        "Statik benchmark bağlamı",
    ))
    .style(Style::new().fg(theme.text).bg(theme.background).bold())
    .render(frame, Rect::new(metrics.x, metrics.y, metrics.width, 1));
    ProgressBar::new(0.82)
        .filled_style(Style::new().fg(theme.secondary).bg(theme.background))
        .unfilled_style(Style::new().fg(theme.muted).bg(theme.background))
        .label(localized(
            showcase.language,
            "Renderer sample · 82%",
            "Render örneği · 82%",
        ))
        .render(
            frame,
            Rect::new(metrics.x, metrics.y.saturating_add(2), metrics.width, 2),
        );
    Gauge::new(0.61)
        .filled_style(Style::new().fg(theme.warning).bg(theme.background))
        .unfilled_style(Style::new().fg(theme.muted).bg(theme.background))
        .label(localized(
            showcase.language,
            "Memory sample · 61%",
            "Bellek örneği · 61%",
        ))
        .render(
            frame,
            Rect::new(metrics.x, metrics.y.saturating_add(5), metrics.width, 2),
        );
    Sparkline::new([1.0, 2.0, 3.0, 5.0, 8.0, 6.0, 5.0, 3.0, 2.0])
        .style(Style::new().fg(theme.success).bg(theme.background))
        .render(
            frame,
            Rect::new(metrics.x, metrics.y.saturating_add(8), metrics.width, 1),
        );
    None
}

fn render_widgets(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    let columns = dragons_tui::Layout::horizontal(vec![Constraint::Fill(1), Constraint::Fill(2)])
        .gap(1)
        .split(area);
    let (left, right) = columns.first().zip(columns.get(1))?;
    let list_inner = panel(
        frame,
        *left,
        "Primitive index",
        showcase.focus.current() == Some(LIST_FOCUS),
        theme,
        BorderSet::rounded(),
    );
    showcase.hits.list = list_inner;
    let items = section_list_items(Section::Widgets);
    List::new(items)
        .normal_style(Style::new().fg(theme.text).bg(theme.background))
        .selected_style(Style::new().fg(theme.success).bg(theme.primary).bold())
        .render(frame, list_inner, &mut showcase.list);

    let rows = dragons_tui::Layout::vertical(vec![
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .gap(1)
    .split(*right);
    if let Some(rect) = rows.first() {
        let inner = panel(
            frame,
            *rect,
            "Table · rich cells",
            showcase.focus.current() == Some(TABLE_FOCUS),
            theme,
            BorderSet::square(),
        );
        showcase.hits.table = inner;
        showcase_table(theme).render(frame, inner, &mut showcase.table);
    }
    if let Some(rect) = rows.get(1) {
        let inner = panel(
            frame,
            *rect,
            "Tree · explicit expansion",
            showcase.focus.current() == Some(TREE_FOCUS),
            theme,
            BorderSet::double(),
        );
        showcase.hits.tree = inner;
        showcase_tree()
            .selected_style(Style::new().fg(theme.success).bg(theme.primary).bold())
            .render(frame, inner, &mut showcase.tree);
    }
    if let Some(rect) = rows.get(2) {
        let inner = panel(
            frame,
            *rect,
            "Viewport",
            showcase.focus.current() == Some(VIEWPORT_FOCUS),
            theme,
            BorderSet::rounded(),
        );
        showcase.hits.viewport = inner;
        Viewport::new(&output_lines())
            .style(Style::new().fg(theme.text).bg(theme.background))
            .render(frame, inner, &mut showcase.viewport);
    }
    None
}

fn render_data(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    let rows = dragons_tui::Layout::vertical(vec![Constraint::Fill(3), Constraint::Fill(1)])
        .gap(1)
        .split(area);
    if let Some(table) = rows.first() {
        let inner = panel(
            frame,
            *table,
            "Agent activity · deterministic data",
            showcase.focus.current() == Some(TABLE_FOCUS),
            theme,
            BorderSet::double(),
        );
        showcase.hits.table = inner;
        showcase_table(theme).render(frame, inner, &mut showcase.table);
    }
    if let Some(status) = rows.get(1) {
        let inner = panel(
            frame,
            *status,
            "Data contract",
            false,
            theme,
            BorderSet::square(),
        );
        RichText::new([
            Line::new([
                Span::styled("No external agents ", Style::new().fg(theme.success).bold()),
                Span::styled("· static showcase data", Style::new().fg(theme.text)),
            ]),
            Line::new([
                Span::styled("Selected rows ", Style::new().fg(theme.warning).underline()),
                Span::styled("preserve rich-cell styles", Style::new().fg(theme.text)),
            ]),
        ])
        .render(frame, inner);
    }
    None
}

fn render_graphics(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    let rows = dragons_tui::Layout::vertical(vec![
        Constraint::Length(4),
        Constraint::Length(4),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .gap(1)
    .split(area);
    if let Some(progress) = rows.first() {
        let inner = panel(
            frame,
            *progress,
            "ProgressBar",
            false,
            theme,
            BorderSet::rounded(),
        );
        ProgressBar::new(0.82)
            .filled_style(Style::new().fg(theme.secondary).bg(theme.background))
            .unfilled_style(Style::new().fg(theme.muted).bg(theme.background))
            .label("Rendering · 82%")
            .render(frame, inner);
    }
    if let Some(gauge) = rows.get(1) {
        let inner = panel(frame, *gauge, "Gauge", false, theme, BorderSet::square());
        Gauge::new(0.61)
            .filled_style(Style::new().fg(theme.warning).bg(theme.background))
            .unfilled_style(Style::new().fg(theme.muted).bg(theme.background))
            .label("Memory · 61%")
            .render(frame, inner);
    }
    if let Some(sparkline) = rows.get(2) {
        let inner = panel(
            frame,
            *sparkline,
            "Sparkline",
            false,
            theme,
            BorderSet::rounded(),
        );
        Sparkline::new([1.0, 2.0, 3.0, 5.0, 8.0, 7.0, 5.0, 3.0, 2.0, 4.0])
            .style(Style::new().fg(theme.success).bg(theme.background))
            .render(frame, inner);
    }
    if let Some(canvas_rect) = rows.get(3) {
        let inner = panel(
            frame,
            *canvas_rect,
            "Canvas · Braille waveform",
            false,
            theme,
            BorderSet::double(),
        );
        let mut canvas = Canvas::new(inner.width, inner.height);
        if canvas.logical_width() > 0 && canvas.logical_height() > 0 {
            let width = canvas.logical_width() as i32;
            let height = canvas.logical_height() as i32;
            canvas.draw_line(0, height / 2, width.saturating_sub(1), height / 2);
            canvas.draw_line(0, height.saturating_sub(1), width.saturating_sub(1), 0);
            canvas.draw_rect(0, 0, canvas.logical_width(), canvas.logical_height());
        }
        canvas.render(
            frame,
            inner,
            Style::new().fg(theme.secondary).bg(theme.background),
        );
    }
    None
}

fn render_input(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    let rows = dragons_tui::Layout::vertical(vec![Constraint::Length(3), Constraint::Fill(1)])
        .gap(1)
        .split(area);
    let mut cursor = None;
    if let Some(input) = rows.first() {
        let inner = panel(
            frame,
            *input,
            "TextInput · grapheme aware",
            showcase.focus.current() == Some(INPUT_FOCUS),
            theme,
            BorderSet::rounded(),
        );
        showcase.hits.input = inner;
        cursor = showcase.input.render(
            frame,
            inner,
            Style::new().fg(theme.text).bg(theme.background),
        );
    }
    if let Some(area_rect) = rows.get(1) {
        let inner = panel(
            frame,
            *area_rect,
            "TextArea · İstanbul · 你好 · 🚀 · é · ❤️ · 👨‍👩‍👧‍👦 · 🇹🇷",
            showcase.focus.current() == Some(AREA_FOCUS),
            theme,
            BorderSet::square(),
        );
        showcase.hits.area = inner;
        let area_cursor = showcase.area.render(
            frame,
            inner,
            Style::new().fg(theme.text).bg(theme.background),
        );
        if showcase.focus.current() == Some(AREA_FOCUS) {
            cursor = area_cursor;
        }
    }
    (showcase.focus.current() == Some(INPUT_FOCUS))
        .then_some(cursor?)
        .or_else(|| {
            if showcase.focus.current() == Some(AREA_FOCUS) {
                cursor
            } else {
                None
            }
        })
}

fn render_interaction(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    let columns = dragons_tui::Layout::horizontal(vec![Constraint::Fill(1), Constraint::Fill(2)])
        .gap(1)
        .split(area);
    let (left, right) = columns.first().zip(columns.get(1))?;
    let list_inner = panel(
        frame,
        *left,
        "Interaction",
        showcase.focus.current() == Some(LIST_FOCUS),
        theme,
        BorderSet::rounded(),
    );
    showcase.hits.list = list_inner;
    List::new(section_list_items(Section::Interaction))
        .normal_style(Style::new().fg(theme.text).bg(theme.background))
        .selected_style(Style::new().fg(theme.success).bg(theme.primary).bold())
        .render(frame, list_inner, &mut showcase.list);
    let inner = panel(
        frame,
        *right,
        "Focus · KeyMap · Style",
        false,
        theme,
        BorderSet::double(),
    );
    let focus = match showcase.focus.current() {
        Some(LIST_FOCUS) => "List",
        Some(TABLE_FOCUS) => "Table",
        Some(TREE_FOCUS) => "Tree",
        Some(VIEWPORT_FOCUS) => "Viewport",
        Some(INPUT_FOCUS) => "TextInput",
        Some(AREA_FOCUS) => "TextArea",
        _ => "None",
    };
    RichText::new([
        Line::new([
            Span::styled("Focus: ", Style::new().fg(theme.warning).bold()),
            Span::styled(focus, Style::new().fg(theme.success).reverse()),
        ]),
        Line::new([
            Span::styled("Ctrl+P ", Style::new().fg(theme.secondary).underline()),
            Span::styled("opens the command palette", Style::new().fg(theme.text)),
        ]),
        Line::new([
            Span::styled("m ", Style::new().fg(theme.error).strikethrough()),
            Span::styled(
                "opens a focus-isolating modal",
                Style::new().fg(theme.text).italic(),
            ),
        ]),
        Line::new([
            Span::styled("Mouse ", Style::new().fg(theme.success).bold()),
            Span::styled(
                "clicks focus; wheel scrolls Viewport",
                Style::new().fg(theme.muted).dim(),
            ),
        ]),
    ])
    .render(frame, inner);
    None
}

fn color_hex(color: Color) -> String {
    let Color::Rgb { r, g, b } = color;
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn render_adapters(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    match showcase.adapter_browser_mode {
        AdapterBrowserMode::Adapters => render_adapter_list(frame, area, showcase),
        AdapterBrowserMode::Capabilities => render_capability_browser(frame, area, showcase),
    }
}

fn render_adapter_list(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    let panes = if area.width >= 100 && area.height >= 12 {
        dragons_tui::Layout::horizontal(vec![Constraint::Fill(3), Constraint::Fill(2)])
            .gap(1)
            .split(area)
    } else {
        dragons_tui::Layout::vertical(vec![Constraint::Fill(1), Constraint::Fill(1)])
            .gap(1)
            .split(area)
    };
    let list_area = panes.first().copied()?;
    let inner = panel(
        frame,
        list_area,
        localized(showcase.language, "Adapters", "Adaptörler"),
        showcase.focus.current() == Some(TABLE_FOCUS),
        theme,
        BorderSet::double(),
    );
    showcase.hits.table = inner;
    if let Some(error) = &showcase.adapter_discovery_error {
        RichText::new([Line::new([
            Span::styled(
                localized(showcase.language, "Discovery failed: ", "Keşif başarısız: "),
                Style::new().fg(theme.error).bold(),
            ),
            Span::styled(error, Style::new().fg(theme.text)),
        ])])
        .render(frame, inner);
        return None;
    }
    if showcase.adapter_rows.is_empty() {
        Text::new(localized(
            showcase.language,
            "No installed adapters. Pass --adapter-root <path> to inspect a local host root.",
            "Kurulu adaptör yok. Yerel host kökünü incelemek için --adapter-root <yol> verin.",
        ))
        .style(Style::new().fg(theme.muted).bg(theme.background))
        .render(frame, inner);
        return None;
    }
    let rows = showcase
        .adapter_rows
        .iter()
        .map(|row| {
            vec![
                Line::new([Span::styled(&row.name, Style::new().bold())]),
                Line::from(row.version.as_str()),
                Line::from(row.state.label(showcase.language)),
                Line::from(row.protocol.as_str()),
                Line::from(localized(showcase.language, "Installed", "Kurulu")),
            ]
        })
        .collect::<Vec<_>>();
    Table::new([
        TableColumn::new(Constraint::Fill(3)),
        TableColumn::new(Constraint::Fill(1)),
        TableColumn::new(Constraint::Fill(2)),
        TableColumn::new(Constraint::Length(8)),
        TableColumn::new(Constraint::Fill(2)),
    ])
    .header([
        Line::from(localized(showcase.language, "NAME", "AD")),
        Line::from(localized(showcase.language, "VERSION", "SÜRÜM")),
        Line::from(localized(showcase.language, "STATE", "DURUM")),
        Line::from(localized(showcase.language, "PROTOCOL", "PROTOKOL")),
        Line::from(localized(showcase.language, "SOURCE", "KAYNAK")),
    ])
    .rows(rows)
    .selected_style(Style::new().fg(theme.success).bg(theme.primary).bold())
    .render(frame, inner, &mut showcase.table);
    let action_hint = if showcase.adapter_remove_confirmation {
        localized(
            showcase.language,
            "Remove selected adapter? Enter confirms · Esc cancels",
            "Seçili adaptör kaldırılsın mı? Enter onaylar · Esc iptal eder",
        )
        .to_owned()
    } else {
        showcase.adapter_action_status.clone().unwrap_or_else(|| {
            localized(
                showcase.language,
                "C capabilities · I install · S start · T stop · R restart · U update · X remove",
                "C yetenekler · I kur · S başlat · T durdur · R yeniden başlat · U güncelle · X kaldır",
            )
            .to_owned()
        })
    };
    Text::new(action_hint)
        .style(Style::new().fg(theme.warning).bg(theme.background).dim())
        .render(
            frame,
            Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        );
    if let Some(detail_area) = panes.get(1).copied() {
        let detail = panel(
            frame,
            detail_area,
            localized(showcase.language, "Adapter Inspector", "Adaptör İnceleyici"),
            false,
            theme,
            BorderSet::rounded(),
        );
        let selected = showcase
            .table
            .selected_index(showcase.adapter_rows.len())
            .and_then(|index| showcase.adapter_rows.get(index));
        let diagnostics = selected.and_then(|row| showcase.adapter_diagnostics.get(&row.id));
        render_adapter_inspector(
            frame,
            detail,
            selected,
            diagnostics,
            showcase.adapter_controller_error.as_deref(),
            showcase.language,
            theme,
        );
    }
    None
}

fn render_capability_browser(
    frame: &mut Frame,
    area: Rect,
    showcase: &mut Showcase,
) -> Option<Position> {
    let theme = showcase.theme;
    let panes = if area.width >= 100 && area.height >= 12 {
        dragons_tui::Layout::horizontal(vec![Constraint::Fill(3), Constraint::Fill(2)])
            .gap(1)
            .split(area)
    } else {
        dragons_tui::Layout::vertical(vec![Constraint::Fill(1), Constraint::Fill(1)])
            .gap(1)
            .split(area)
    };
    let list_area = panes.first().copied()?;
    let inner = panel(
        frame,
        list_area,
        localized(showcase.language, "Capabilities", "Yetenekler"),
        showcase.focus.current() == Some(TABLE_FOCUS),
        theme,
        BorderSet::double(),
    );
    showcase.hits.table = inner;
    if let Some(error) = &showcase.adapter_discovery_error {
        RichText::new([Line::new([
            Span::styled(
                localized(showcase.language, "Discovery failed: ", "Keşif başarısız: "),
                Style::new().fg(theme.error).bold(),
            ),
            Span::styled(error, Style::new().fg(theme.text)),
        ])])
        .render(frame, inner);
        return None;
    }
    if showcase.adapter_rows.is_empty() {
        Text::new(localized(
            showcase.language,
            "No adapters available.",
            "Kullanılabilir adaptör yok.",
        ))
        .style(Style::new().fg(theme.muted).bg(theme.background))
        .render(frame, inner);
        return None;
    }
    let index = showcase.capability_provider_index();
    if index.is_empty() {
        Text::new(localized(
            showcase.language,
            "No capabilities reported by live adapters.",
            "Canlı adaptörler yetenek raporlamıyor.",
        ))
        .style(Style::new().fg(theme.muted).bg(theme.background))
        .render(frame, inner);
        return None;
    }
    let rows = index
        .iter()
        .map(|(capability, providers)| {
            vec![
                Line::new([Span::styled(capability, Style::new().bold())]),
                Line::from(providers.len().to_string()),
            ]
        })
        .collect::<Vec<_>>();
    Table::new([
        TableColumn::new(Constraint::Fill(4)),
        TableColumn::new(Constraint::Length(10)),
    ])
    .header([
        Line::from(localized(showcase.language, "CAPABILITY", "YETENEK")),
        Line::from(localized(showcase.language, "PROVIDERS", "SAĞLAYICI")),
    ])
    .rows(rows)
    .selected_style(Style::new().fg(theme.success).bg(theme.primary).bold())
    .render(frame, inner, &mut showcase.table);
    Text::new(localized(
        showcase.language,
        "C or Esc returns to adapters · ↑↓ selects a capability",
        "C veya Esc adaptörlere döner · ↑↓ yetenek seçer",
    ))
    .style(Style::new().fg(theme.warning).bg(theme.background).dim())
    .render(
        frame,
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
    if let Some(detail_area) = panes.get(1).copied() {
        let detail = panel(
            frame,
            detail_area,
            localized(
                showcase.language,
                "Capability Providers",
                "Yetenek Sağlayıcıları",
            ),
            false,
            theme,
            BorderSet::rounded(),
        );
        let selected = showcase
            .table
            .selected_index(index.len())
            .and_then(|selected| index.iter().nth(selected));
        let lines = selected.map_or_else(
            || {
                vec![Line::from(localized(
                    showcase.language,
                    "No capability selected",
                    "Yetenek seçilmedi",
                ))]
            },
            |(capability, providers)| {
                let mut lines = vec![Line::new([
                    Span::styled(
                        format!(
                            "{}: ",
                            localized(showcase.language, "Capability", "Yetenek")
                        ),
                        Style::new().fg(theme.warning).bold(),
                    ),
                    Span::styled(capability, Style::new().fg(theme.text)),
                ])];
                lines.push(Line::from(localized(
                    showcase.language,
                    "Providers",
                    "Sağlayıcılar",
                )));
                lines.extend(providers.iter().map(|provider| {
                    Line::new([Span::styled(
                        format!("{} · {} · {}", provider.id, provider.name, provider.state),
                        Style::new().fg(theme.text),
                    )])
                }));
                lines
            },
        );
        RichText::new(lines).render(frame, detail);
    }
    None
}

fn render_adapter_inspector(
    frame: &mut Frame,
    area: Rect,
    selected: Option<&AdapterRow>,
    diagnostics: Option<&ControllerIpcDiagnostics>,
    controller_error: Option<&str>,
    language: Language,
    theme: Theme,
) {
    let Some(row) = selected else {
        Text::new(localized(
            language,
            "No adapter selected",
            "Adaptör seçilmedi",
        ))
        .style(Style::new().fg(theme.muted).bg(theme.background))
        .render(frame, area);
        return;
    };
    let unavailable = "--".to_owned();
    let runtime_version = diagnostics
        .and_then(|item| item.version.clone())
        .unwrap_or_else(|| unavailable.clone());
    let protocol = diagnostics
        .and_then(|item| item.protocol)
        .map(|value| value.to_string())
        .unwrap_or_else(|| row.protocol.clone());
    let state = diagnostics
        .map(|item| item.state.clone())
        .unwrap_or_else(|| row.state.label(language).to_owned());
    let pid = diagnostics
        .and_then(|item| item.pid)
        .map(|value| value.to_string())
        .unwrap_or_else(|| unavailable.clone());
    let uptime = diagnostics
        .and_then(|item| item.uptime_millis)
        .map(format_uptime_millis)
        .unwrap_or_else(|| unavailable.clone());
    let capabilities = diagnostics
        .filter(|item| !item.capabilities.is_empty())
        .map(|item| item.capabilities.join(", "))
        .unwrap_or_else(|| localized(language, "-- (not running)", "-- (çalışmıyor)").to_owned());
    let pending_requests = diagnostics
        .map(|item| item.pending_request_count.to_string())
        .unwrap_or_else(|| unavailable.clone());
    let event_queue = diagnostics
        .map(|item| format!("{}/{}", item.event_queue_len, item.event_queue_capacity))
        .unwrap_or_else(|| unavailable.clone());
    let dropped_events = diagnostics
        .map(|item| item.dropped_event_count.to_string())
        .unwrap_or_else(|| unavailable.clone());
    let last_error = diagnostics
        .and_then(|item| item.last_error.clone())
        .or_else(|| row.last_error.clone())
        .or_else(|| controller_error.map(ToOwned::to_owned))
        .unwrap_or_else(|| unavailable.clone());
    let stderr_tail = diagnostics
        .filter(|item| !item.stderr_tail.is_empty())
        .map(|item| item.stderr_tail.clone())
        .unwrap_or_else(|| unavailable.clone());
    let lines = vec![
        (
            localized(language, "Adapter ID", "Adaptör Kimliği"),
            row.id.clone(),
        ),
        (localized(language, "Name", "Ad"), row.name.clone()),
        (
            localized(language, "Installed version", "Kurulu sürüm"),
            row.version.clone(),
        ),
        (
            localized(language, "Runtime version", "Çalışma sürümü"),
            runtime_version,
        ),
        (localized(language, "Protocol", "Protokol"), protocol),
        (localized(language, "State", "Durum"), state),
        (localized(language, "PID", "PID"), pid),
        (localized(language, "Uptime", "Çalışma süresi"), uptime),
        (
            localized(language, "Executable path", "Çalıştırılabilir yol"),
            row.executable.clone(),
        ),
        (
            localized(language, "Capabilities", "Yetenekler"),
            capabilities,
        ),
        (
            localized(language, "Pending requests", "Bekleyen istekler"),
            pending_requests,
        ),
        (
            localized(language, "Event queue usage", "Olay kuyruğu kullanımı"),
            event_queue,
        ),
        (
            localized(language, "Dropped events", "Atılan olaylar"),
            dropped_events,
        ),
        (localized(language, "Last error", "Son hata"), last_error),
        (
            localized(language, "stderr diagnostic tail", "stderr tanı kuyruğu"),
            stderr_tail,
        ),
    ];
    RichText::new(lines.into_iter().map(|(label, value)| {
        Line::new([
            Span::styled(format!("{label}: "), Style::new().fg(theme.warning).bold()),
            Span::styled(value, Style::new().fg(theme.text)),
        ])
    }))
    .render(frame, area);
}

fn format_uptime_millis(millis: u64) -> String {
    format!("{}.{:03}s", millis / 1_000, millis % 1_000)
}

fn render_settings(frame: &mut Frame, area: Rect, showcase: &mut Showcase) -> Option<Position> {
    let theme = showcase.theme;
    let inner = panel(
        frame,
        area,
        localized(showcase.language, "Settings", "Ayarlar"),
        true,
        theme,
        BorderSet::rounded(),
    );
    let bottom = inner.y.saturating_add(inner.height);
    let mut row_y = inner.y;
    Text::new(localized(showcase.language, "Language", "Dil"))
        .style(Style::new().fg(theme.warning).bg(theme.background).bold())
        .render(frame, Rect::new(inner.x, row_y, inner.width, 1));
    row_y = row_y.saturating_add(1);
    for (index, (row, label, value)) in [
        (
            SettingRow::English,
            "English",
            showcase.language == Language::English,
        ),
        (
            SettingRow::Turkish,
            "Türkçe",
            showcase.language == Language::Turkish,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if row_y < bottom {
            let rect = Rect::new(inner.x, row_y, inner.width, 1);
            showcase.hits.settings_rows[index] = rect;
            let marker = if value { "●" } else { "○" };
            let selected = showcase.settings.selected == row;
            Text::new(format!("{} {}", if selected { ">" } else { " " }, marker))
                .style(if selected {
                    Style::new().fg(theme.success).bg(theme.primary).bold()
                } else {
                    Style::new().fg(theme.muted).bg(theme.background)
                })
                .render(frame, rect);
            Text::new(label)
                .style(if selected {
                    Style::new().fg(theme.success).bg(theme.primary).bold()
                } else {
                    Style::new().fg(theme.text).bg(theme.background)
                })
                .render(
                    frame,
                    Rect::new(
                        inner.x.saturating_add(4),
                        row_y,
                        inner.width.saturating_sub(4),
                        1,
                    ),
                );
        }
        row_y = row_y.saturating_add(1);
    }
    row_y = row_y.saturating_add(1);
    if row_y < bottom {
        Text::new(localized(showcase.language, "Theme", "Tema"))
            .style(Style::new().fg(theme.warning).bg(theme.background).bold())
            .render(frame, Rect::new(inner.x, row_y, inner.width, 1));
    }
    row_y = row_y.saturating_add(1);
    for (offset, slot) in ThemeSlot::ALL.into_iter().enumerate() {
        if row_y >= bottom {
            break;
        }
        let index = offset + 2;
        let row = SettingRow::Color(slot);
        let rect = Rect::new(inner.x, row_y, inner.width, 1);
        showcase.hits.settings_rows[index] = rect;
        let selected = showcase.settings.selected == row;
        let color = slot.color(theme);
        let prefix = if selected { ">" } else { " " };
        RichText::new([Line::new([
            Span::styled(
                format!("{prefix} {:<12}", slot.label(showcase.language)),
                if selected {
                    Style::new().fg(theme.success).bg(theme.primary).bold()
                } else {
                    Style::new().fg(theme.text).bg(theme.background)
                },
            ),
            Span::styled(" ███ ", Style::new().fg(color).bg(theme.background).bold()),
            Span::styled(
                color_hex(color),
                Style::new().fg(theme.muted).bg(theme.background),
            ),
        ])])
        .render(frame, rect);
        row_y = row_y.saturating_add(1);
    }
    row_y = row_y.saturating_add(1);
    if row_y < bottom {
        let rect = Rect::new(inner.x, row_y, inner.width, 1);
        showcase.hits.settings_rows[10] = rect;
        let selected = showcase.settings.selected == SettingRow::Reset;
        Text::new(format!(
            "{} [ {} ]",
            if selected { ">" } else { " " },
            localized(
                showcase.language,
                "Reset Dragonfire Theme",
                "Dragonfire Temasını Sıfırla"
            )
        ))
        .style(if selected {
            Style::new().fg(theme.success).bg(theme.primary).bold()
        } else {
            Style::new().fg(theme.secondary).bg(theme.background).bold()
        })
        .render(frame, rect);
    }
    None
}

fn render_overlays(frame: &mut Frame, size: Size, showcase: &mut Showcase) {
    let theme = showcase.theme;
    let parent = Rect::new(0, 0, size.width, size.height);
    if showcase.modal_open {
        let rect = Modal::new(
            "DragonsTUI",
            [
                Line::from("Terminal-native UI"),
                Line::from(""),
                Line::new([Span::styled(
                    "Keyboard  ✓",
                    Style::new().fg(theme.success).bold(),
                )]),
                Line::new([Span::styled(
                    "Mouse     ✓",
                    Style::new().fg(theme.success).bold(),
                )]),
                Line::new([Span::styled(
                    "Unicode   ✓",
                    Style::new().fg(theme.success).bold(),
                )]),
                Line::new([Span::styled(
                    "Animation ✓",
                    Style::new().fg(theme.success).bold(),
                )]),
                Line::from(""),
                Line::from("Enter / Esc / click to close"),
            ],
        )
        .size(46, 14)
        .border_style(Style::new().fg(theme.secondary).bg(theme.background).bold())
        .title_style(Style::new().fg(theme.success).bg(theme.background).bold())
        .content_style(Style::new().fg(theme.text).bg(theme.background))
        .render(frame, parent);
        showcase.hits.modal = rect;
    }
    if let Some(editor) = showcase.color_editor.as_mut() {
        let title = format!(
            "{}: {}",
            localized(showcase.language, "Edit Color", "Rengi Düzenle"),
            editor.slot.label(showcase.language)
        );
        let rect = Modal::new(
            title,
            [
                Line::from("R"),
                Line::from("G"),
                Line::from("B"),
                Line::from(""),
                Line::from(localized(showcase.language, "Preview", "Önizleme")),
                Line::from(""),
                Line::from(localized(
                    showcase.language,
                    "Enter Apply   Esc Cancel",
                    "Enter Uygula   Esc İptal",
                )),
            ],
        )
        .size(42, 12)
        .border_style(Style::new().fg(theme.secondary).bg(theme.background).bold())
        .title_style(Style::new().fg(theme.success).bg(theme.background).bold())
        .content_style(Style::new().fg(theme.text).bg(theme.background))
        .render(frame, parent);
        let inner = rect.inner();
        for channel in 0..3 {
            let y = inner.y.saturating_add(u16::try_from(channel).unwrap_or(0));
            let input = Rect::new(
                inner.x.saturating_add(5),
                y,
                inner.width.saturating_sub(5),
                1,
            );
            showcase.hits.color_inputs[channel] = input;
            let style = if editor.channel == channel {
                Style::new().fg(theme.success).bg(theme.primary).bold()
            } else {
                Style::new().fg(theme.text).bg(theme.background)
            };
            let _ = editor.inputs[channel].render(frame, input, style);
        }
        let preview_y = inner.y.saturating_add(4);
        let preview = Rect::new(
            inner.x.saturating_add(9),
            preview_y,
            inner.width.saturating_sub(9),
            1,
        );
        Text::new("████████")
            .style(Style::new().fg(editor.color()).bg(theme.background).bold())
            .render(frame, preview);
        showcase.hits.color_apply = Rect::new(inner.x, inner.y.saturating_add(6), inner.width, 1);
    }
    if let Some(palette) = &showcase.palette {
        let rect = palette.render(
            frame,
            parent,
            Style::new().fg(theme.secondary).bg(theme.background).bold(),
            Style::new().fg(theme.text).bg(theme.background),
            Style::new().fg(theme.success).bg(theme.primary).bold(),
        );
        showcase.hits.palette = rect;
        let inner = rect.inner();
        showcase.hits.palette_rows = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );
    }
}

struct TerminalGuard {
    raw: bool,
    alternate: bool,
    hidden: bool,
    mouse: bool,
    restored: bool,
}

impl TerminalGuard {
    fn enter(output: &mut impl Write) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut guard = Self {
            raw: true,
            alternate: false,
            hidden: false,
            mouse: false,
            restored: false,
        };
        if let Err(error) = guard.setup(output) {
            let _ = guard.restore(output);
            return Err(error);
        }
        Ok(guard)
    }

    fn setup(&mut self, output: &mut impl Write) -> io::Result<()> {
        execute!(output, EnterAlternateScreen)?;
        self.alternate = true;
        execute!(output, Hide)?;
        self.hidden = true;
        execute!(output, EnableMouseCapture)?;
        self.mouse = true;
        execute!(output, Clear(ClearType::All))
    }

    fn restore(&mut self, output: &mut impl Write) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let mut result = Ok(());
        if self.mouse {
            result = execute!(output, DisableMouseCapture);
            self.mouse = false;
        }
        if self.hidden {
            let next = execute!(output, Show);
            if result.is_ok() {
                result = next;
            }
            self.hidden = false;
        }
        if self.alternate {
            let next = execute!(output, LeaveAlternateScreen);
            if result.is_ok() {
                result = next;
            }
            self.alternate = false;
        }
        if self.raw {
            let next = terminal::disable_raw_mode();
            if result.is_ok() {
                result = next;
            }
            self.raw = false;
        }
        self.restored = true;
        result
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.restored {
            let mut output = stdout();
            let _ = self.restore(&mut output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::default(),
        }
    }

    fn buffer_row(frame: &Frame, y: u16) -> String {
        (0..frame.buffer().width())
            .filter_map(|x| frame.buffer().get(x, y).map(|cell| cell.character))
            .collect()
    }

    fn frame_contains(frame: &Frame, needle: &str) -> bool {
        (0..frame.buffer().height()).any(|y| buffer_row(frame, y).contains(needle))
    }

    #[test]
    fn showcase_starts_on_splash_with_overview_ready_for_transition() {
        let showcase = Showcase::new(Instant::now());
        assert_eq!(showcase.phase, Phase::Splash);
        assert_eq!(showcase.section, Section::Overview);
    }

    #[test]
    fn section_navigation_and_palette_commands_are_explicit() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.handle_key(key(KeyCode::Enter));
        assert_eq!(showcase.section, Section::Overview);
        showcase.handle_key(key(KeyCode::Char('4')));
        assert_eq!(showcase.section, Section::Graphics);
        showcase.handle_key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers {
                ctrl: true,
                ..KeyModifiers::default()
            },
        });
        assert!(showcase.palette.is_some());
        let outcome = showcase.execute_command(CommandId::new("show-input"));
        assert!(outcome.redraw);
        assert_eq!(showcase.section, Section::Input);
    }

    #[test]
    fn visible_header_sections_switch_by_mouse_and_keep_keyboard_navigation() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.handle_key(key(KeyCode::Enter));
        let _ = showcase_view(Size::new(160, 55), &mut showcase);

        for section in Section::ALL {
            let rect = showcase.hits.sections[section.index()];
            assert!(rect.width > 0, "{section:?} should be visible");
            assert!(
                showcase
                    .handle_mouse(MouseEvent {
                        x: rect.x,
                        y: rect.y,
                        kind: MouseKind::LeftDown,
                        modifiers: KeyModifiers::default(),
                    })
                    .redraw
            );
            assert_eq!(showcase.section, section);
            let selected = showcase_view(Size::new(160, 55), &mut showcase);
            assert_eq!(
                selected
                    .frame
                    .buffer()
                    .get(rect.x, rect.y)
                    .unwrap()
                    .style
                    .bg,
                Some(showcase.theme.primary)
            );
        }

        let before = showcase.section;
        assert!(
            !showcase
                .handle_mouse(MouseEvent {
                    x: 159,
                    y: 54,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(showcase.section, before);
        assert!(showcase.handle_key(key(KeyCode::Char('1'))).redraw);
        assert_eq!(showcase.section, Section::Overview);

        let _ = showcase_view(Size::new(20, 8), &mut showcase);
        let hidden = showcase.hits.sections[Section::Interaction.index()];
        assert_eq!(hidden.width, 0);
        assert!(
            !showcase
                .handle_mouse(MouseEvent {
                    x: 0,
                    y: 1,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
    }

    #[test]
    fn settings_opens_by_keyboard_mouse_and_localized_palette_without_hidden_hits() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.handle_key(key(KeyCode::Enter));
        assert!(showcase.handle_key(key(KeyCode::Char('7'))).redraw);
        assert_eq!(showcase.section, Section::Settings);
        let settings = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&settings.frame, "Settings"));
        let header = showcase.hits.sections[Section::Settings.index()];
        assert!(header.width > 0);
        assert_eq!(
            settings
                .frame
                .buffer()
                .get(header.x, header.y)
                .unwrap()
                .style
                .bg,
            Some(showcase.theme.primary)
        );

        showcase.select_section(Section::Overview);
        assert!(
            showcase
                .handle_mouse(MouseEvent {
                    x: header.x,
                    y: header.y,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(showcase.section, Section::Settings);
        assert!(
            showcase
                .execute_command(CommandId::new("open-settings"))
                .redraw
        );
        assert_eq!(showcase.section, Section::Settings);

        showcase.open_palette();
        assert!(
            showcase
                .palette
                .as_ref()
                .unwrap()
                .filtered_titles()
                .contains(&"Open Settings".to_owned())
        );
        showcase.palette = None;
        showcase.language = Language::Turkish;
        showcase.open_palette();
        assert!(
            showcase
                .palette
                .as_ref()
                .unwrap()
                .filtered_titles()
                .contains(&"Ayarları Aç".to_owned())
        );
        showcase.palette = None;

        let _ = showcase_view(Size::new(30, 8), &mut showcase);
        assert_eq!(showcase.hits.sections[Section::Settings.index()].width, 0);
    }

    #[test]
    fn adapters_open_by_keyboard_mouse_and_localized_palette_with_tiny_rendering() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.handle_key(key(KeyCode::Enter));
        assert!(showcase.handle_key(key(KeyCode::Char('8'))).redraw);
        assert_eq!(showcase.section, Section::Adapters);
        let adapters = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&adapters.frame, "No installed adapters"));
        let header = showcase.hits.sections[Section::Adapters.index()];
        assert!(header.width > 0);

        showcase.select_section(Section::Overview);
        assert!(
            showcase
                .handle_mouse(MouseEvent {
                    x: header.x,
                    y: header.y,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(showcase.section, Section::Adapters);
        assert!(
            showcase
                .execute_command(CommandId::new("open-adapters"))
                .redraw
        );

        showcase.language = Language::Turkish;
        showcase.open_palette();
        assert!(
            showcase
                .palette
                .as_ref()
                .unwrap()
                .filtered_titles()
                .contains(&"Adaptörleri Aç".to_owned())
        );
        showcase.palette = None;
        let turkish = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&turkish.frame, "Adaptörler"));
        assert!(frame_contains(&turkish.frame, "Kurulu adaptör yok"));

        for size in [Size::new(20, 8), Size::new(5, 3), Size::new(1, 1)] {
            let tiny = showcase_view(size, &mut showcase);
            assert_eq!(
                (tiny.frame.buffer().width(), tiny.frame.buffer().height()),
                (size.width, size.height)
            );
        }
    }

    #[test]
    fn adapters_table_uses_host_discovery_for_stopped_and_incompatible_local_adapters() {
        let root = std::env::temp_dir().join(format!(
            "dragonstui-showcase-adapters-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let valid = root.join("mock");
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::write(
            valid.join("adapter.json"),
            r#"{"id":"mock","name":"Host Mock","version":"0.1.0","protocol_version":1,"executable":"adapter-bin"}"#,
        )
        .unwrap();
        let executable = valid.join("adapter-bin");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let incompatible = root.join("incompatible");
        std::fs::create_dir_all(&incompatible).unwrap();
        std::fs::write(
            incompatible.join("adapter.json"),
            r#"{"id":"legacy","name":"Legacy Mock","version":"0.2.0","protocol_version":2,"executable":"adapter-bin"}"#,
        )
        .unwrap();

        let rows = adapter_rows_from_root(&root).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| {
            row.name == "Host Mock"
                && row.version == "0.1.0"
                && row.state == AdapterViewState::Stopped
                && row.protocol == "1"
        }));
        assert!(rows.iter().any(|row| {
            row.name == "Legacy Mock"
                && row.version == "0.2.0"
                && row.state == AdapterViewState::Incompatible
                && row.protocol == "2"
        }));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capability_provider_index_is_sorted_deduplicated_and_rebuilt_from_diagnostics() {
        let rows = vec![
            AdapterRow {
                id: "adapter-b".to_owned(),
                name: "Adapter B".to_owned(),
                version: "1.0.0".to_owned(),
                state: AdapterViewState::Stopped,
                protocol: "1".to_owned(),
                executable: "adapter-b".to_owned(),
                last_error: None,
            },
            AdapterRow {
                id: "adapter-a".to_owned(),
                name: "Adapter A".to_owned(),
                version: "1.0.0".to_owned(),
                state: AdapterViewState::Stopped,
                protocol: "1".to_owned(),
                executable: "adapter-a".to_owned(),
                last_error: None,
            },
            AdapterRow {
                id: "adapter-empty".to_owned(),
                name: "Adapter Empty".to_owned(),
                version: "1.0.0".to_owned(),
                state: AdapterViewState::Stopped,
                protocol: "1".to_owned(),
                executable: "adapter-empty".to_owned(),
                last_error: None,
            },
        ];
        let mut diagnostics = BTreeMap::new();
        diagnostics.insert(
            "adapter-b".to_owned(),
            ControllerIpcDiagnostics {
                adapter_id: "adapter-b".to_owned(),
                version: Some("1.0.0".to_owned()),
                protocol: Some(1),
                state: "running".to_owned(),
                pid: Some(2),
                uptime_millis: Some(1),
                capabilities: vec![
                    "cap.shared".to_owned(),
                    "cap.b".to_owned(),
                    "cap.shared".to_owned(),
                ],
                last_error: None,
                stderr_tail: String::new(),
                stderr_dropped_line_count: 0,
                dropped_event_count: 0,
                pending_request_count: 0,
                response_queue_capacity: 1,
                response_queue_len: 0,
                event_queue_capacity: 1,
                event_queue_len: 0,
            },
        );
        diagnostics.insert(
            "adapter-a".to_owned(),
            ControllerIpcDiagnostics {
                adapter_id: "adapter-a".to_owned(),
                version: Some("1.0.0".to_owned()),
                protocol: Some(1),
                state: "stopped".to_owned(),
                pid: None,
                uptime_millis: None,
                capabilities: vec!["cap.shared".to_owned(), "cap.a".to_owned()],
                last_error: None,
                stderr_tail: String::new(),
                stderr_dropped_line_count: 0,
                dropped_event_count: 0,
                pending_request_count: 0,
                response_queue_capacity: 1,
                response_queue_len: 0,
                event_queue_capacity: 1,
                event_queue_len: 0,
            },
        );

        let index = capability_provider_index(&rows, &diagnostics);
        assert_eq!(
            index.keys().cloned().collect::<Vec<_>>(),
            vec!["cap.a", "cap.b", "cap.shared"]
        );
        assert_eq!(
            index["cap.shared"]
                .iter()
                .map(|provider| provider.id.as_str())
                .collect::<Vec<_>>(),
            vec!["adapter-a", "adapter-b"]
        );
        assert_eq!(index["cap.shared"][0].state, "stopped");
        assert!(!index.values().any(|providers| {
            providers
                .iter()
                .any(|provider| provider.id == "adapter-empty")
        }));

        diagnostics.clear();
        diagnostics.insert(
            "adapter-a".to_owned(),
            ControllerIpcDiagnostics {
                adapter_id: "adapter-a".to_owned(),
                version: Some("1.0.0".to_owned()),
                protocol: Some(1),
                state: "running".to_owned(),
                pid: Some(1),
                uptime_millis: Some(2),
                capabilities: vec!["cap.new".to_owned()],
                last_error: None,
                stderr_tail: String::new(),
                stderr_dropped_line_count: 0,
                dropped_event_count: 0,
                pending_request_count: 0,
                response_queue_capacity: 1,
                response_queue_len: 0,
                event_queue_capacity: 1,
                event_queue_len: 0,
            },
        );
        let refreshed = capability_provider_index(&rows, &diagnostics);
        assert_eq!(
            refreshed.keys().cloned().collect::<Vec<_>>(),
            vec!["cap.new"]
        );
        assert_eq!(refreshed["cap.new"][0].id, "adapter-a");
    }

    #[test]
    fn capability_browser_renders_selected_capability_and_its_runtime_providers() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.phase = Phase::Showcase;
        showcase.select_section(Section::Adapters);
        showcase.adapter_rows = vec![
            AdapterRow {
                id: "adapter-a".to_owned(),
                name: "Adapter A".to_owned(),
                version: "1.0.0".to_owned(),
                state: AdapterViewState::Stopped,
                protocol: "1".to_owned(),
                executable: "adapter-a".to_owned(),
                last_error: None,
            },
            AdapterRow {
                id: "adapter-b".to_owned(),
                name: "Adapter B".to_owned(),
                version: "1.0.0".to_owned(),
                state: AdapterViewState::Stopped,
                protocol: "1".to_owned(),
                executable: "adapter-b".to_owned(),
                last_error: None,
            },
        ];
        for (id, name, state) in [
            ("adapter-a", "Adapter A", "running"),
            ("adapter-b", "Adapter B", "stopped"),
        ] {
            showcase.adapter_diagnostics.insert(
                id.to_owned(),
                ControllerIpcDiagnostics {
                    adapter_id: id.to_owned(),
                    version: Some("1.0.0".to_owned()),
                    protocol: Some(1),
                    state: state.to_owned(),
                    pid: None,
                    uptime_millis: None,
                    capabilities: vec!["cap.shared".to_owned()],
                    last_error: None,
                    stderr_tail: name.to_owned(),
                    stderr_dropped_line_count: 0,
                    dropped_event_count: 0,
                    pending_request_count: 0,
                    response_queue_capacity: 1,
                    response_queue_len: 0,
                    event_queue_capacity: 1,
                    event_queue_len: 0,
                },
            );
        }

        assert!(showcase.handle_key(key(KeyCode::Char('c'))).redraw);
        let view = showcase_view(Size::new(160, 55), &mut showcase);
        for expected in [
            "Capabilities",
            "cap.shared",
            "2",
            "Capability Providers",
            "Adapter A",
            "running",
            "Adapter B",
            "stopped",
        ] {
            assert!(frame_contains(&view.frame, expected), "missing {expected}");
        }
        assert!(showcase.handle_key(key(KeyCode::Escape)).redraw);
        let adapters = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&adapters.frame, "Adapter Inspector"));
    }

    #[test]
    fn capability_browser_does_not_dispatch_adapter_management_actions() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.phase = Phase::Showcase;
        showcase.select_section(Section::Adapters);
        showcase.adapter_rows = vec![AdapterRow {
            id: "adapter-a".to_owned(),
            name: "Adapter A".to_owned(),
            version: "1.0.0".to_owned(),
            state: AdapterViewState::Stopped,
            protocol: "1".to_owned(),
            executable: "adapter-a".to_owned(),
            last_error: None,
        }];
        showcase.adapter_diagnostics.insert(
            "adapter-a".to_owned(),
            ControllerIpcDiagnostics {
                adapter_id: "adapter-a".to_owned(),
                version: Some("1.0.0".to_owned()),
                protocol: Some(1),
                state: "running".to_owned(),
                pid: Some(1),
                uptime_millis: Some(1),
                capabilities: vec!["cap.shared".to_owned()],
                last_error: None,
                stderr_tail: String::new(),
                stderr_dropped_line_count: 0,
                dropped_event_count: 0,
                pending_request_count: 0,
                response_queue_capacity: 1,
                response_queue_len: 0,
                event_queue_capacity: 1,
                event_queue_len: 0,
            },
        );
        showcase.handle_key(key(KeyCode::Char('c')));

        assert!(!showcase.handle_key(key(KeyCode::Char('s'))).redraw);
        assert!(showcase.adapter_action_status.is_none());
    }

    #[test]
    fn capability_browser_recovers_selection_and_empty_state_after_snapshot_refresh() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.phase = Phase::Showcase;
        showcase.select_section(Section::Adapters);
        showcase.adapter_rows = vec![AdapterRow {
            id: "adapter-a".to_owned(),
            name: "Adapter A".to_owned(),
            version: "1.0.0".to_owned(),
            state: AdapterViewState::Stopped,
            protocol: "1".to_owned(),
            executable: "adapter-a".to_owned(),
            last_error: None,
        }];
        let snapshot = |capabilities: Vec<&str>| ControllerIpcDiagnostics {
            adapter_id: "adapter-a".to_owned(),
            version: Some("1.0.0".to_owned()),
            protocol: Some(1),
            state: "running".to_owned(),
            pid: Some(1),
            uptime_millis: Some(1),
            capabilities: capabilities.into_iter().map(ToOwned::to_owned).collect(),
            last_error: None,
            stderr_tail: String::new(),
            stderr_dropped_line_count: 0,
            dropped_event_count: 0,
            pending_request_count: 0,
            response_queue_capacity: 1,
            response_queue_len: 0,
            event_queue_capacity: 1,
            event_queue_len: 0,
        };
        showcase.adapter_diagnostics.insert(
            "adapter-a".to_owned(),
            snapshot(vec!["cap.old", "cap.selected"]),
        );
        showcase.handle_key(key(KeyCode::Char('c')));
        showcase.handle_key(key(KeyCode::Down));
        let selected = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&selected.frame, "cap.selected"));

        showcase
            .adapter_diagnostics
            .insert("adapter-a".to_owned(), snapshot(vec!["cap.new"]));
        let refreshed = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&refreshed.frame, "cap.new"));
        assert!(!frame_contains(&refreshed.frame, "cap.selected"));
        assert_eq!(showcase.table.selected_index(1), Some(0));

        showcase.adapter_diagnostics.clear();
        let empty = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(
            &empty.frame,
            "No capabilities reported by live adapters."
        ));
        assert_eq!(showcase.table.selected_index(0), None);
    }

    #[test]
    fn capability_browser_footer_exposes_return_navigation() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.phase = Phase::Showcase;
        showcase.select_section(Section::Adapters);
        showcase.handle_key(key(KeyCode::Char('c')));

        let view = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(buffer_row(&view.frame, 53).contains("C/Esc adapters"));
    }

    #[test]
    fn adapter_inspector_shows_selected_discovered_host_metadata_without_inventing_runtime_metrics()
    {
        let root = std::env::temp_dir().join(format!(
            "dragonstui-showcase-inspector-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let adapter = root.join("mock");
        std::fs::create_dir_all(&adapter).unwrap();
        std::fs::write(
            adapter.join("adapter.json"),
            r#"{"id":"mock","name":"Inspector Mock","version":"0.1.0","protocol_version":1,"executable":"adapter-bin"}"#,
        )
        .unwrap();
        let executable = adapter.join("adapter-bin");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mut showcase = Showcase::with_adapter_root(Instant::now(), Some(&root));
        showcase.phase = Phase::Showcase;
        showcase.select_section(Section::Adapters);
        let view = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&view.frame, "Inspector Mock"));
        assert!(frame_contains(&view.frame, "Adapter ID"));
        assert!(frame_contains(&view.frame, "mock"));
        assert!(frame_contains(&view.frame, "Executable path"));
        assert!(frame_contains(&view.frame, "Pending requests"));
        assert!(frame_contains(&view.frame, "--"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_inspector_shows_live_controller_diagnostics_without_overriding_install_metadata() {
        let root = std::env::temp_dir().join(format!(
            "dragonstui-showcase-live-inspector-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let adapter = root.join("mock");
        std::fs::create_dir_all(&adapter).unwrap();
        std::fs::write(
            adapter.join("adapter.json"),
            r#"{"id":"mock","name":"Live Inspector Mock","version":"0.1.0","protocol_version":1,"executable":"adapter-bin"}"#,
        )
        .unwrap();
        let executable = adapter.join("adapter-bin");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mut showcase = Showcase::with_adapter_root(Instant::now(), Some(&root));
        showcase.phase = Phase::Showcase;
        showcase.select_section(Section::Adapters);
        showcase.adapter_diagnostics.insert(
            "mock".to_owned(),
            ControllerIpcDiagnostics {
                adapter_id: "mock".to_owned(),
                version: Some("0.1.1-runtime".to_owned()),
                protocol: Some(1),
                state: "running".to_owned(),
                pid: Some(4242),
                uptime_millis: Some(1_234),
                capabilities: vec!["test.echo".to_owned(), "test.stream".to_owned()],
                last_error: Some("runtime diagnostic".to_owned()),
                stderr_tail: "diagnostic line".to_owned(),
                stderr_dropped_line_count: 1,
                dropped_event_count: 2,
                pending_request_count: 3,
                response_queue_capacity: 16,
                response_queue_len: 5,
                event_queue_capacity: 8,
                event_queue_len: 2,
            },
        );
        let view = showcase_view(Size::new(160, 55), &mut showcase);
        for expected in [
            "0.1.0",
            "0.1.1-runtime",
            "running",
            "4242",
            "1.234s",
            "test.echo, test.stream",
            "3",
            "2/8",
            "2",
            "runtime diagnostic",
            "diagnostic line",
        ] {
            assert!(frame_contains(&view.frame, expected), "missing {expected}");
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_lifecycle_worker_uses_the_typed_daemon_management_path() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
        };

        let root = std::env::temp_dir().join(format!(
            "dragonstui-showcase-typed-worker-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(root.join(".controller")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        std::fs::write(
            root.join(".controller/endpoint.json"),
            format!(r#"{{"address":"{address}","token":"test-token"}}"#),
        )
        .unwrap();
        let daemon = thread::spawn(move || {
            let mut operations = Vec::new();
            for _ in 0..150 {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let mut request = String::new();
                        BufReader::new(&stream).read_line(&mut request).unwrap();
                        assert!(request.contains("\"token\":\"test-token\""));
                        assert!(request.contains("\"command\":\"management\""));
                        let (operation, outcome) = if request.contains("\"operation\":\"start\"") {
                            ("start", "Started")
                        } else if request.contains("\"operation\":\"stop\"") {
                            ("stop", "Stopped")
                        } else if request.contains("\"operation\":\"restart\"") {
                            ("restart", "Restarted")
                        } else {
                            panic!("unexpected typed lifecycle request: {request}");
                        };
                        stream
                            .write_all(
                                format!(
                                    "{{\"status\":{{\"Management\":{{\"result\":\"lifecycle\",\"outcome\":{{\"{outcome}\":{{\"id\":\"mock\"}}}}}}}},\"error\":null}}\n"
                                )
                                .as_bytes(),
                            )
                            .unwrap();
                        operations.push(operation);
                        if operations.len() == 3 {
                            return operations;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("fake daemon accept failed: {error}"),
                }
            }
            operations
        });
        let (actions, results) = adapter_action_worker(&root);
        let id = AdapterId::new("mock").unwrap();
        for (action, expected) in [
            (
                AdapterManagementAction::Start { id: id.clone() },
                Ok("Started { id: AdapterId(\"mock\") }".to_owned()),
            ),
            (
                AdapterManagementAction::Stop { id: id.clone() },
                Ok("Stopped { id: AdapterId(\"mock\") }".to_owned()),
            ),
            (
                AdapterManagementAction::Restart { id },
                Ok("Restarted { id: AdapterId(\"mock\") }".to_owned()),
            ),
        ] {
            actions.send(action).unwrap();
            assert_eq!(
                results.recv_timeout(Duration::from_secs(1)).unwrap(),
                expected
            );
        }
        drop(actions);
        assert_eq!(daemon.join().unwrap(), ["start", "stop", "restart"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_worker_rejects_same_adapter_mutations_while_other_adapters_continue() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            sync::{Arc, Mutex},
        };

        let root = std::env::temp_dir().join(format!(
            "dragonstui-showcase-action-conflicts-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(root.join(".controller")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::fs::write(
            root.join(".controller/endpoint.json"),
            format!(r#"{{"address":"{address}","token":"test-token"}}"#),
        )
        .unwrap();

        let (a_started_sender, a_started) = mpsc::channel();
        let (release_a, release_a_receiver) = mpsc::channel();
        let operations = Arc::new(Mutex::new(Vec::new()));
        let daemon = thread::spawn({
            let operations = Arc::clone(&operations);
            let a_started_sender = Arc::new(Mutex::new(Some(a_started_sender)));
            let release_a_receiver = Arc::new(Mutex::new(Some(release_a_receiver)));
            move || {
                let mut handlers = Vec::new();
                for _ in 0..3 {
                    let (mut stream, _) = listener.accept().unwrap();
                    let operations = Arc::clone(&operations);
                    let a_started_sender = Arc::clone(&a_started_sender);
                    let release_a_receiver = Arc::clone(&release_a_receiver);
                    handlers.push(thread::spawn(move || {
                        let mut request = String::new();
                        BufReader::new(&stream).read_line(&mut request).unwrap();
                        assert!(request.contains("\"token\":\"test-token\""));
                        assert!(request.contains("\"command\":\"management\""));
                        let is_start = request.contains("\"operation\":\"start\"");
                        let is_stop = request.contains("\"operation\":\"stop\"");
                        let id = if request.contains("\"id\":\"adapter-a\"") {
                            "adapter-a"
                        } else if request.contains("\"id\":\"adapter-b\"") {
                            "adapter-b"
                        } else {
                            panic!("unexpected adapter in request: {request}");
                        };
                        operations
                            .lock()
                            .unwrap()
                            .push(format!("{id}:{}", if is_start { "start" } else { "stop" }));
                        if id == "adapter-a" && is_start {
                            a_started_sender
                                .lock()
                                .unwrap()
                                .take()
                                .unwrap()
                                .send(())
                                .unwrap();
                            release_a_receiver
                                .lock()
                                .unwrap()
                                .take()
                                .unwrap()
                                .recv()
                                .unwrap();
                        }
                        let outcome = if is_start {
                            "Started"
                        } else if is_stop {
                            "Stopped"
                        } else {
                            panic!("unexpected lifecycle request: {request}");
                        };
                        stream
                            .write_all(
                                format!(
                                    "{{\"status\":{{\"Management\":{{\"result\":\"lifecycle\",\"outcome\":{{\"{outcome}\":{{\"id\":\"{id}\"}}}}}}}},\"error\":null}}\n"
                                )
                                .as_bytes(),
                            )
                            .unwrap();
                    }));
                }
                for handler in handlers {
                    handler.join().unwrap();
                }
            }
        });

        let (actions, results) = adapter_action_worker(&root);
        let adapter_a = AdapterId::new("adapter-a").unwrap();
        let adapter_b = AdapterId::new("adapter-b").unwrap();
        actions
            .send(AdapterManagementAction::Start {
                id: adapter_a.clone(),
            })
            .unwrap();
        a_started.recv_timeout(Duration::from_secs(1)).unwrap();

        actions
            .send(AdapterManagementAction::Update {
                id: adapter_a.clone(),
                registry_source: "unused-because-the-action-must-conflict".to_owned(),
            })
            .unwrap();
        let conflict = results.recv_timeout(Duration::from_secs(1)).unwrap();
        let conflict = conflict.expect_err("same-adapter update must be rejected while start runs");
        assert!(conflict.contains("adapter-a"), "conflict: {conflict}");
        assert!(
            conflict.contains("already in progress"),
            "conflict: {conflict}"
        );

        actions
            .send(AdapterManagementAction::Start {
                id: adapter_b.clone(),
            })
            .unwrap();
        assert_eq!(
            results.recv_timeout(Duration::from_secs(1)).unwrap(),
            Ok("Started { id: AdapterId(\"adapter-b\") }".to_owned())
        );

        release_a.send(()).unwrap();
        assert_eq!(
            results.recv_timeout(Duration::from_secs(1)).unwrap(),
            Ok("Started { id: AdapterId(\"adapter-a\") }".to_owned())
        );
        actions
            .send(AdapterManagementAction::Stop { id: adapter_a })
            .unwrap();
        assert_eq!(
            results.recv_timeout(Duration::from_secs(1)).unwrap(),
            Ok("Stopped { id: AdapterId(\"adapter-a\") }".to_owned())
        );
        drop(actions);
        daemon.join().unwrap();
        assert_eq!(
            *operations.lock().unwrap(),
            ["adapter-a:start", "adapter-b:start", "adapter-a:stop"]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_worker_releases_an_adapter_after_a_daemon_failure() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
        };

        let root = std::env::temp_dir().join(format!(
            "dragonstui-showcase-action-failure-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(root.join(".controller")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::fs::write(
            root.join(".controller/endpoint.json"),
            format!(r#"{{"address":"{address}","token":"test-token"}}"#),
        )
        .unwrap();
        let daemon = thread::spawn(move || {
            let mut operations = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).unwrap();
                let response = if request.contains("\"operation\":\"start\"") {
                    operations.push("start");
                    "{\"status\":null,\"error\":\"forced daemon failure\"}\n".to_owned()
                } else if request.contains("\"operation\":\"restart\"") {
                    operations.push("restart");
                    "{\"status\":{\"Management\":{\"result\":\"lifecycle\",\"outcome\":{\"Restarted\":{\"id\":\"adapter-a\"}}}},\"error\":null}\n".to_owned()
                } else {
                    panic!("unexpected typed lifecycle request: {request}");
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
            operations
        });

        let (actions, results) = adapter_action_worker(&root);
        let id = AdapterId::new("adapter-a").unwrap();
        actions
            .send(AdapterManagementAction::Start { id: id.clone() })
            .unwrap();
        let failure = results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(failure.is_err());
        assert!(failure.unwrap_err().contains("forced daemon failure"));

        actions
            .send(AdapterManagementAction::Restart { id })
            .unwrap();
        assert_eq!(
            results.recv_timeout(Duration::from_secs(1)).unwrap(),
            Ok("Restarted { id: AdapterId(\"adapter-a\") }".to_owned())
        );
        drop(actions);
        assert_eq!(daemon.join().unwrap(), ["start", "restart"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_conflict_result_does_not_block_the_tui_on_held_diagnostics() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            sync::mpsc,
        };

        let root = std::env::temp_dir().join(format!(
            "dragonstui-showcase-conflict-diagnostics-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let adapter = root.join("adapter-a");
        std::fs::create_dir_all(adapter.join("bin")).unwrap();
        std::fs::write(
            adapter.join("adapter.json"),
            r#"{"id":"adapter-a","name":"Adapter A","version":"0.1.0","protocol_version":1,"executable":"bin/mock"}"#,
        )
        .unwrap();
        std::fs::write(adapter.join("bin/mock"), "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(
            adapter.join("bin/mock"),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".controller")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::fs::write(
            root.join(".controller/endpoint.json"),
            format!(r#"{{"address":"{address}","token":"test-token"}}"#),
        )
        .unwrap();
        let (start_ready, start_started) = mpsc::channel();
        let (release_start, start_release) = mpsc::channel();
        let daemon = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).unwrap();
                if request.contains("\"operation\":\"diagnostics\"") {
                    stream.write_all(b"{\"status\":{\"Management\":{\"result\":\"diagnostics\",\"diagnostics\":{\"adapter_id\":\"adapter-a\",\"version\":\"0.1.0\",\"protocol\":1,\"state\":\"stopped\",\"pid\":null,\"uptime_millis\":null,\"capabilities\":[],\"last_error\":null,\"stderr_tail\":\"\",\"stderr_dropped_line_count\":0,\"dropped_event_count\":0,\"pending_request_count\":0,\"response_queue_capacity\":8,\"response_queue_len\":0,\"event_queue_capacity\":8,\"event_queue_len\":0}}}},\"error\":null}\n").unwrap();
                } else {
                    assert!(request.contains("\"operation\":\"start\""));
                    start_ready.send(()).unwrap();
                    start_release.recv().unwrap();
                    stream.write_all(b"{\"status\":{\"Management\":{\"result\":\"lifecycle\",\"outcome\":{\"Started\":{\"id\":\"adapter-a\"}}}},\"error\":null}\n").unwrap();
                }
            }
        });
        let mut showcase = Showcase::with_adapter_root(Instant::now(), Some(&root));
        showcase.handle_key(key(KeyCode::Enter));
        showcase.queue_adapter_action(AdapterManagementAction::Start {
            id: AdapterId::new("adapter-a").unwrap(),
        });
        start_started.recv_timeout(Duration::from_secs(1)).unwrap();
        showcase.queue_adapter_action(AdapterManagementAction::Start {
            id: AdapterId::new("adapter-a").unwrap(),
        });

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut conflict_visible = false;
        let mut largest_advance = Duration::ZERO;
        while Instant::now() < deadline {
            let before = Instant::now();
            showcase.advance(before);
            largest_advance = largest_advance.max(before.elapsed());
            if showcase
                .adapter_action_status
                .as_deref()
                .is_some_and(|status| status.contains("already in progress"))
            {
                conflict_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        release_start.send(()).unwrap();
        drop(showcase);
        daemon.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(conflict_visible, "conflict result was not rendered");
        assert!(
            largest_advance < Duration::from_millis(100),
            "draining the conflict blocked the TUI for {largest_advance:?}"
        );
    }

    #[test]
    fn adapter_detail_refresh_uses_typed_daemon_diagnostics() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
        };

        let root = std::env::temp_dir().join(format!(
            "dragonstui-showcase-typed-diagnostics-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let adapter = root.join("mock");
        std::fs::create_dir_all(adapter.join("bin")).unwrap();
        std::fs::write(
            adapter.join("adapter.json"),
            r#"{"id":"mock","name":"Typed Diagnostics","version":"0.1.0","protocol_version":1,"executable":"bin/mock"}"#,
        )
        .unwrap();
        std::fs::write(adapter.join("bin/mock"), "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(
                adapter.join("bin/mock"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        std::fs::create_dir_all(root.join(".controller")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        std::fs::write(
            root.join(".controller/endpoint.json"),
            format!(r#"{{"address":"{address}","token":"test-token"}}"#),
        )
        .unwrap();
        let daemon = thread::spawn(move || {
            let mut operations = Vec::new();
            let mut diagnostics_state = "stopped";
            for _ in 0..150 {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let mut request = String::new();
                        BufReader::new(&stream).read_line(&mut request).unwrap();
                        assert!(request.contains("\"command\":\"management\""));
                        let response = if request.contains("\"operation\":\"diagnostics\"") {
                            operations.push("diagnostics");
                            format!(
                                "{{\"status\":{{\"Management\":{{\"result\":\"diagnostics\",\"diagnostics\":{{\"adapter_id\":\"mock\",\"version\":\"0.1.0\",\"protocol\":1,\"state\":\"{diagnostics_state}\",\"pid\":42,\"uptime_millis\":1234,\"capabilities\":[\"test.echo\"],\"last_error\":null,\"stderr_tail\":\"\",\"stderr_dropped_line_count\":0,\"dropped_event_count\":0,\"pending_request_count\":0,\"response_queue_capacity\":8,\"response_queue_len\":0,\"event_queue_capacity\":8,\"event_queue_len\":0}}}}}},\"error\":null}}\n"
                            )
                        } else if request.contains("\"operation\":\"start\"") {
                            operations.push("start");
                            diagnostics_state = "running";
                            "{\"status\":{\"Management\":{\"result\":\"lifecycle\",\"outcome\":{\"Started\":{\"id\":\"mock\"}}}},\"error\":null}\n".to_owned()
                        } else {
                            panic!("unexpected typed management request: {request}");
                        };
                        stream.write_all(response.as_bytes()).unwrap();
                        if operations == ["diagnostics", "start", "diagnostics"] {
                            return operations;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("fake daemon accept failed: {error}"),
                }
            }
            operations
        });
        let mut showcase = Showcase::with_adapter_root(Instant::now(), Some(&root));
        showcase.handle_key(key(KeyCode::Enter));
        for _ in 0..100 {
            showcase.advance(Instant::now());
            if showcase
                .adapter_diagnostics
                .get("mock")
                .is_some_and(|diagnostics| diagnostics.state == "stopped")
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(showcase.adapter_diagnostics["mock"].state, "stopped");
        showcase.queue_adapter_action(AdapterManagementAction::Start {
            id: AdapterId::new("mock").unwrap(),
        });
        for _ in 0..100 {
            showcase.advance(Instant::now());
            if showcase.adapter_diagnostics["mock"].state == "running" {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(showcase.adapter_diagnostics["mock"].state, "running");
        assert_eq!(
            daemon.join().unwrap(),
            ["diagnostics", "start", "diagnostics"]
        );
        drop(showcase);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_language_changes_immediately_by_keyboard_and_mouse() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.handle_key(key(KeyCode::Enter));
        showcase.select_section(Section::Settings);
        assert_eq!(showcase.language, Language::English);
        assert!(showcase.handle_key(key(KeyCode::Down)).redraw);
        assert!(showcase.handle_key(key(KeyCode::Enter)).redraw);
        assert_eq!(showcase.language, Language::Turkish);
        let turkish = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&turkish.frame, "Ayarlar"));
        assert!(frame_contains(&turkish.frame, "Dil"));
        assert!(frame_contains(&turkish.frame, "Genel Bakış"));
        showcase.select_section(Section::Overview);
        let overview = showcase_view(Size::new(160, 55), &mut showcase);
        assert!(frame_contains(&overview.frame, "Çalışma Zamanı"));
        assert!(frame_contains(&overview.frame, "Statik benchmark bağlamı"));
        showcase.select_section(Section::Settings);
        let _ = showcase_view(Size::new(160, 55), &mut showcase);

        let english = showcase.hits.settings_rows[0];
        assert!(
            showcase
                .handle_mouse(MouseEvent {
                    x: english.x,
                    y: english.y,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(showcase.language, Language::English);
    }

    #[test]
    fn settings_theme_editor_clamps_invalid_rgb_applies_cancels_and_resets_without_language_reset()
    {
        assert_eq!(parse_rgb("0"), 0);
        assert_eq!(parse_rgb("255"), 255);
        assert_eq!(parse_rgb("-4"), 0);
        assert_eq!(parse_rgb("999"), 255);
        assert_eq!(parse_rgb("not-a-number"), 0);

        let mut showcase = Showcase::new(Instant::now());
        showcase.handle_key(key(KeyCode::Enter));
        showcase.select_section(Section::Settings);
        showcase.settings.selected = SettingRow::Color(ThemeSlot::Primary);
        showcase.activate_setting();
        let editor = showcase.color_editor.as_mut().unwrap();
        editor.inputs[0].clear();
        for character in "999".chars() {
            editor.inputs[0].insert(character);
        }
        editor.inputs[1].clear();
        editor.inputs[1].insert('-');
        editor.inputs[2].clear();
        for character in "40".chars() {
            editor.inputs[2].insert(character);
        }
        assert!(showcase.handle_key(key(KeyCode::Enter)).redraw);
        assert_eq!(showcase.theme.primary, Color::rgb(255, 0, 40));
        let view = showcase_view(Size::new(160, 55), &mut showcase);
        let settings_tab = showcase.hits.sections[Section::Settings.index()];
        assert_eq!(
            view.frame
                .buffer()
                .get(settings_tab.x, settings_tab.y)
                .unwrap()
                .style
                .bg,
            Some(Color::rgb(255, 0, 40))
        );
        showcase.settings.selected = SettingRow::Turkish;
        showcase.activate_setting();
        assert_eq!(showcase.theme.primary, Color::rgb(255, 0, 40));

        let before_cancel = showcase.theme.secondary;
        showcase.settings.selected = SettingRow::Color(ThemeSlot::Secondary);
        showcase.activate_setting();
        showcase.color_editor.as_mut().unwrap().inputs[0].clear();
        showcase.color_editor.as_mut().unwrap().inputs[0].insert('0');
        showcase.handle_key(key(KeyCode::Escape));
        assert_eq!(showcase.theme.secondary, before_cancel);

        showcase.language = Language::Turkish;
        showcase.settings.selected = SettingRow::Reset;
        showcase.activate_setting();
        assert_eq!(showcase.theme, Theme::default());
        assert_eq!(showcase.language, Language::Turkish);

        showcase.settings.selected = SettingRow::Color(ThemeSlot::Text);
        showcase.activate_setting();
        for size in [Size::new(40, 15), Size::new(5, 3), Size::new(1, 1)] {
            let view = showcase_view(size, &mut showcase);
            assert_eq!(
                (view.frame.buffer().width(), view.frame.buffer().height()),
                (size.width, size.height)
            );
        }
    }

    #[test]
    fn modal_restores_focus_and_table_tree_input_actions_are_stateful() {
        let mut showcase = Showcase::new(Instant::now());
        showcase.handle_key(key(KeyCode::Enter));
        showcase.focus.set_focus(TABLE_FOCUS);
        showcase.handle_key(key(KeyCode::Down));
        assert_eq!(showcase.table.selected_index(3), Some(1));
        showcase.focus.set_focus(TREE_FOCUS);
        assert!(showcase.handle_key(key(KeyCode::Right)).redraw);
        showcase.focus.set_focus(INPUT_FOCUS);
        assert!(showcase.handle_key(key(KeyCode::Char('!'))).redraw);
        showcase.open_modal();
        assert!(showcase.modal_open);
        showcase.handle_key(key(KeyCode::Escape));
        assert!(!showcase.modal_open);
        assert_eq!(showcase.focus.current(), Some(INPUT_FOCUS));
    }

    #[test]
    fn animation_resize_tiny_rendering_and_quit_are_safe() {
        let start = Instant::now();
        let mut showcase = Showcase::new(start);
        assert!(showcase.advance(start + SPLASH_DURATION));
        for (width, height) in [
            (120, 40),
            (80, 24),
            (40, 15),
            (20, 8),
            (5, 3),
            (1, 1),
            (80, 24),
        ] {
            let view = showcase_view(Size::new(width, height), &mut showcase);
            assert_eq!(
                (view.frame.buffer().width(), view.frame.buffer().height()),
                (width, height)
            );
        }
        showcase.execute_command(CommandId::new("toggle-animation"));
        assert!(!showcase.animation_enabled);
        assert!(showcase.handle_key(key(KeyCode::Char('q'))).quit);
        assert!(
            showcase
                .handle_key(KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers {
                        ctrl: true,
                        ..KeyModifiers::default()
                    },
                })
                .quit
        );
    }

    #[test]
    fn splash_uses_the_supplied_banner_and_display_width_centering() {
        let showcase = Showcase::new(Instant::now());
        let size = Size::new(180, 60);
        let mut showcase = showcase;
        let splash = showcase_view(size, &mut showcase);
        let banner = "██████╗ ██████╗  █████╗  ██████╗  ██████╗ ███╗   ██╗";
        let banner_row = (0..size.height)
            .find(|&y| buffer_row(&splash.frame, y).contains(banner))
            .expect("supplied DRAGON banner should render");
        let banner_line = SPLASH_TITLE[0];
        let banner_x = splash_block_origin(size, &SPLASH_TITLE).saturating_add(
            u16::try_from(display_width(banner_line) - display_width(banner_line.trim_start()))
                .unwrap(),
        );
        assert_eq!(
            splash
                .frame
                .buffer()
                .get(banner_x, banner_row)
                .unwrap()
                .character,
            '█'
        );

        let motto = "𓆩 -- we are the recall. not born. remembered. -- 𓆪";
        let motto_row = (0..size.height)
            .find(|&y| buffer_row(&splash.frame, y).contains(motto))
            .expect("supplied Unicode motto should render");
        let motto_line = SPLASH_TITLE[6];
        let motto_x = splash_block_origin(size, &SPLASH_TITLE).saturating_add(
            u16::try_from(display_width(motto_line) - display_width(motto_line.trim_start()))
                .unwrap(),
        );
        assert_eq!(
            buffer_row(&splash.frame, motto_row)
                .chars()
                .nth(usize::from(motto_x)),
            Some('𓆩')
        );
    }

    #[test]
    fn splash_visible_bounds_share_one_global_center_and_frame_geometry() {
        let size = Size::new(160, 55);
        let layout = splash_layout(size);
        let title = visible_content_bounds(&SPLASH_TITLE).unwrap();
        let dragon = visible_content_bounds(&SPLASH_DRAGON).unwrap();
        let manifesto = visible_content_bounds(&SPLASH_MANIFESTO).unwrap();
        let loading = visible_content_bounds(&[LOADING_TEXT[2], LOADING_HINT]).unwrap();

        assert_eq!(title.min_x, 6, "title source indentation must be measured");
        assert_eq!(
            dragon.min_x, 0,
            "Braille blank cells are not visible content"
        );
        assert_eq!(
            dragon.max_x, 49,
            "Braille visible bounds must ignore trailing blanks"
        );
        assert_eq!(dragon.width, 50);
        let unicode = visible_content_bounds(&["   ", "  𓆩⠀⠁  "]).unwrap();
        assert_eq!(unicode.min_x, u16::try_from(display_width("  ")).unwrap());
        assert_eq!(
            unicode.max_x,
            u16::try_from(display_width("  𓆩⠀⠁"))
                .unwrap()
                .saturating_sub(1)
        );
        assert_eq!(
            unicode.width,
            unicode
                .max_x
                .saturating_sub(unicode.min_x)
                .saturating_add(1),
            "blank lines and trailing whitespace must not expand Unicode visible bounds"
        );
        assert_eq!(
            splash_visible_center(layout.title_origin, title),
            layout.center_x,
            "the title block visible box must use the global splash center"
        );
        assert_eq!(
            splash_visible_center(layout.dragon_origin, dragon),
            layout.center_x,
            "the dragon visible box must use the global splash center"
        );
        assert_eq!(
            splash_visible_center(layout.manifesto_origin, manifesto),
            layout.center_x,
            "the manifesto visible box must use the global splash center"
        );
        assert_eq!(
            splash_visible_center(layout.loading_origin, loading),
            layout.center_x,
            "the loading group visible box must use the global splash center"
        );

        let frame = layout.frame;
        assert_eq!(frame.x.saturating_add(frame.width / 2), layout.center_x);
        assert!(
            frame.width < size.width,
            "the frame must stay content-centric"
        );
        for (origin, bounds) in [
            (layout.title_origin, title),
            (layout.dragon_origin, dragon),
            (layout.manifesto_origin, manifesto),
            (layout.loading_origin, loading),
        ] {
            assert!(origin.saturating_add(bounds.min_x) > frame.x);
            assert!(origin.saturating_add(bounds.max_x) < frame.right().saturating_sub(1));
        }

        let dragon_line_bounds = visible_content_bounds(&[SPLASH_DRAGON[0]]).unwrap();
        let next_dragon_line_bounds = visible_content_bounds(&[SPLASH_DRAGON[1]]).unwrap();
        assert_eq!(
            layout
                .dragon_origin
                .saturating_add(next_dragon_line_bounds.min_x)
                .saturating_sub(
                    layout
                        .dragon_origin
                        .saturating_add(dragon_line_bounds.min_x)
                ),
            next_dragon_line_bounds
                .min_x
                .saturating_sub(dragon_line_bounds.min_x),
            "dragon lines must retain source-relative positions rather than being centered independently"
        );

        let mut showcase = Showcase::new(Instant::now());
        let splash = showcase_view(size, &mut showcase);
        let border = Style::new()
            .fg(showcase.theme.secondary)
            .bg(showcase.theme.background);
        assert_eq!(
            splash
                .frame
                .buffer()
                .get(frame.x, frame.y)
                .unwrap()
                .character,
            '╭'
        );
        assert_eq!(
            splash
                .frame
                .buffer()
                .get(frame.right().saturating_sub(1), frame.y)
                .unwrap()
                .character,
            '╮'
        );
        for y in frame.y.saturating_add(1)..frame.bottom().saturating_sub(1) {
            assert_eq!(splash.frame.buffer().get(frame.x, y).unwrap().style, border);
            assert_eq!(
                splash
                    .frame
                    .buffer()
                    .get(frame.right().saturating_sub(1), y)
                    .unwrap()
                    .style,
                border
            );
        }
        for separator_y in [layout.before_manifesto_y, layout.after_manifesto_y] {
            assert_eq!(
                splash
                    .frame
                    .buffer()
                    .get(frame.x, separator_y)
                    .unwrap()
                    .character,
                '├'
            );
            assert_eq!(
                splash
                    .frame
                    .buffer()
                    .get(frame.right().saturating_sub(1), separator_y)
                    .unwrap()
                    .character,
                '┤'
            );
            assert_eq!(
                buffer_row(&splash.frame, separator_y)
                    .chars()
                    .filter(|character| *character == '─')
                    .count(),
                usize::from(frame.width.saturating_sub(2)),
                "separator width must derive from the frame"
            );
        }

        for size in [
            Size::new(160, 55),
            Size::new(120, 40),
            Size::new(80, 24),
            Size::new(40, 15),
            Size::new(20, 8),
            Size::new(5, 3),
            Size::new(1, 1),
        ] {
            let mut tiny = Showcase::new(Instant::now());
            let splash = showcase_view(size, &mut tiny);
            assert_eq!(
                (
                    splash.frame.buffer().width(),
                    splash.frame.buffer().height()
                ),
                (size.width, size.height)
            );
        }
    }

    #[test]
    fn splash_uses_common_block_origins_and_slow_timing() {
        assert_eq!(SPLASH_DURATION, Duration::from_secs(6));

        let start = Instant::now();
        let mut showcase = Showcase::new(start);
        let size = Size::new(180, 60);
        let splash = showcase_view(size, &mut showcase);
        let title_origin = splash_block_origin(size, &SPLASH_TITLE);
        let title_prefix = SPLASH_TITLE[0]
            .split_once('█')
            .map(|(prefix, _)| prefix)
            .unwrap();
        let banner_y = (0..size.height)
            .find(|&y| buffer_row(&splash.frame, y).contains("██████╗"))
            .unwrap();
        assert_eq!(
            splash
                .frame
                .buffer()
                .get(
                    title_origin
                        .saturating_add(u16::try_from(display_width(title_prefix)).unwrap()),
                    banner_y,
                )
                .unwrap()
                .character,
            '█'
        );

        for (lines, line, marker, needle) in [
            (&SPLASH_DRAGON[..], SPLASH_DRAGON[0], '⣀', "⣀⣀⣤"),
            (
                &SPLASH_MANIFESTO[..],
                SPLASH_MANIFESTO[2],
                'w',
                "we build. we burn.",
            ),
        ] {
            let prefix = line.split_once(marker).map(|(prefix, _)| prefix).unwrap();
            let origin = if needle == "we build. we burn." {
                splash_block_origin(size, &[line])
            } else {
                splash_block_origin(size, lines)
            };
            let row = (0..size.height)
                .find(|&y| buffer_row(&splash.frame, y).contains(needle))
                .unwrap();
            assert_eq!(
                splash
                    .frame
                    .buffer()
                    .get(
                        origin.saturating_add(u16::try_from(display_width(prefix)).unwrap()),
                        row,
                    )
                    .unwrap()
                    .character,
                marker
            );
        }
        let loading_origin = splash_block_origin(size, &LOADING_TEXT);
        let loading_row = (0..size.height)
            .find(|&y| buffer_row(&splash.frame, y).contains(LOADING_TEXT[0]))
            .unwrap();
        assert_eq!(
            splash
                .frame
                .buffer()
                .get(loading_origin, loading_row)
                .unwrap()
                .character,
            '𝐍'
        );

        assert!(!showcase.advance(start));
        assert!(!showcase.advance(start + Duration::from_millis(50)));
        assert!(showcase.advance(start + Duration::from_millis(250)));
        assert_eq!(showcase.splash_animation.current(), Some(&1));
        assert_eq!(loading_fill(0), 1);
        assert_eq!(loading_fill(2), 1);
        assert_eq!(loading_fill(3), 2);
        assert_eq!(loading_fill(23), 10);
        assert!(showcase.advance(start + Duration::from_millis(5_999)));
        assert_eq!(showcase.phase, Phase::Splash);
        assert!(showcase.advance(start + SPLASH_DURATION));

        let mut skipped = Showcase::new(start);
        assert!(skipped.handle_key(key(KeyCode::Char(' '))).redraw);
        assert_eq!(skipped.phase, Phase::Showcase);
    }

    #[test]
    fn splash_lines_fit_the_large_terminal_contract_by_display_width() {
        let widths: Vec<_> = SPLASH_TITLE
            .into_iter()
            .chain(SPLASH_DRAGON)
            .chain(SPLASH_MANIFESTO)
            .map(display_width)
            .collect();
        assert!(
            widths.iter().all(|&width| width <= 160),
            "splash line widths = {widths:?}"
        );
    }

    #[test]
    fn splash_gradient_moves_styles_without_moving_art_and_transitions_cleanly() {
        let start = Instant::now();
        let mut showcase = Showcase::new(start);
        let size = Size::new(180, 60);
        let first = showcase_view(size, &mut showcase);
        let mut colors = Vec::new();
        for y in 0..size.height {
            for x in 0..size.width {
                let cell = first.frame.buffer().get(x, y).unwrap();
                if ('\u{2801}'..='\u{28ff}').contains(&cell.character)
                    && cell.style.fg.is_some_and(|color| !colors.contains(&color))
                {
                    colors.push(cell.style.fg.unwrap());
                }
            }
        }
        assert!(
            colors.len() >= 4,
            "dragon artwork needs multiple Dragonfire colors"
        );

        assert!(!showcase.advance(start));
        assert!(showcase.advance(start + Duration::from_millis(500)));
        let second = showcase_view(size, &mut showcase);
        assert!(
            dragons_tui::diff(Some(first.frame.buffer()), second.frame.buffer())
                .iter()
                .any(|change| {
                    ('\u{2801}'..='\u{28ff}').contains(&change.current.character)
                        && change.previous.is_some_and(|previous| {
                            previous.character == change.current.character
                                && previous.style != change.current.style
                        })
                }),
            "a new phase must change artwork styles without changing artwork geometry"
        );
        assert!(
            (0..size.height).any(|y| buffer_row(&second.frame, y).contains(LOADING_TEXT[1])),
            "loading text should animate without a numeric progress claim"
        );

        assert!(showcase.advance(start + SPLASH_DURATION));
        assert_eq!(showcase.phase, Phase::Showcase);
        let main = showcase_view(size, &mut showcase);
        assert!(
            !(0..size.height).any(|y| buffer_row(&main.frame, y).contains('⣿')),
            "showcase redraw must not retain splash artwork"
        );

        for (width, height) in [(0, 0), (1, 1), (5, 3), (20, 8), (40, 15)] {
            let mut tiny = Showcase::new(start);
            let view = showcase_view(Size::new(width, height), &mut tiny);
            assert_eq!(
                (view.frame.buffer().width(), view.frame.buffer().height()),
                (width, height)
            );
        }
    }

    #[test]
    fn dragonfire_visual_contract_includes_background_braille_and_unicode_examples() {
        let start = Instant::now();
        let mut showcase = Showcase::new(start);
        let splash = showcase_view(Size::new(120, 40), &mut showcase);
        assert_eq!(
            splash.frame.buffer().get(0, 0).unwrap().style.bg,
            Some(showcase.theme.background)
        );
        assert!((0..splash.frame.buffer().height()).any(|y| {
            (0..splash.frame.buffer().width())
                .filter_map(|x| splash.frame.buffer().get(x, y).map(|cell| cell.character))
                .collect::<String>()
                .contains("we are the recall")
        }));
        assert!((0..splash.frame.buffer().height()).any(|y| {
            (0..splash.frame.buffer().width()).any(|x| {
                splash
                    .frame
                    .buffer()
                    .get(x, y)
                    .is_some_and(|cell| cell.character == '⣀')
            })
        }));
        showcase.handle_key(key(KeyCode::Enter));
        showcase.select_section(Section::Graphics);
        let graphics = showcase_view(Size::new(80, 24), &mut showcase);
        assert!((0..graphics.frame.buffer().height()).any(|y| {
            (0..graphics.frame.buffer().width()).any(|x| {
                graphics
                    .frame
                    .buffer()
                    .get(x, y)
                    .is_some_and(|cell| ('\u{2800}'..='\u{28ff}').contains(&cell.character))
            })
        }));
        showcase.select_section(Section::Input);
        let input = showcase_view(Size::new(80, 24), &mut showcase);
        assert!(
            (0..input.frame.buffer().height()).any(|y| (0..input.frame.buffer().width()).any(
                |x| {
                    input
                        .frame
                        .buffer()
                        .get(x, y)
                        .is_some_and(|cell| cell.character == 'İ' || cell.character == '你')
                }
            ))
        );
    }
}
