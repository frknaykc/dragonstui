use std::{
    io::{self, Write, stdout},
    time::{Duration, Instant},
};

use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use dragons_tui::{
    Alignment, Animation, BorderSet, Canvas, Cell, CommandId, CommandPalette, Constraint, Event,
    FocusId, FocusState, Frame, Gauge, KeyCode, KeyEvent, KeyMap, Layout, Line, Modal, MouseEvent,
    MouseKind, PaletteCommand, Panel, Position, ProgressBar, Rect, RichText, Runtime,
    ShutdownSignal, Size, Span, Sparkline, Spinner, Style, Table, TableColumn, TableState, Text,
    TextArea, Theme, Tree, TreeNode, TreeState, Viewport, ViewportState, is_quit_key,
    terminal_size,
};

use agent_process::AgentProcess;

const TICK_INTERVAL: Duration = Duration::from_millis(50);
const SPLASH_DURATION: Duration = Duration::from_millis(1_000);

mod agent_process;

fn main() -> io::Result<()> {
    let shutdown = ShutdownSignal::install()?;
    let mut output = stdout();
    let mut terminal = TerminalGuard::enter(&mut output)?;
    let run_result = run(&mut output, &shutdown);
    let restore_result = terminal.restore(&mut output);

    run_result.and(restore_result)
}

fn run(output: &mut impl Write, shutdown: &ShutdownSignal) -> io::Result<()> {
    let mut runtime = Runtime::new(Some(TICK_INTERVAL));
    let mut app = App::new(Instant::now());

    loop {
        if shutdown.requested() {
            return Ok(());
        }
        if runtime.needs_redraw() {
            let view = app_view(terminal_size()?, &mut app);
            runtime.render_with_cursor(output, view.frame, view.cursor)?;
        }

        let event = runtime.next_event()?;
        if shutdown.requested() {
            return Ok(());
        }
        match event {
            Event::Key(key) => {
                let outcome = app.handle_key(key);
                if outcome.quit {
                    return Ok(());
                }
                if outcome.redraw {
                    runtime.request_redraw();
                }
            }
            Event::Mouse(mouse) => {
                if app.handle_mouse(mouse).redraw {
                    runtime.request_redraw();
                }
            }
            Event::Resize(_) => runtime.request_redraw(),
            Event::Tick(now) => {
                if app.advance(now) {
                    runtime.request_redraw();
                }
            }
        }
    }
}

const AGENTS: [&str; 4] = ["Codex", "Hermes", "Claude", "Local"];
const MAX_OUTPUT_LINES: usize = 1_000;
const AGENTS_FOCUS: FocusId = FocusId::new(1);
const OUTPUT_FOCUS: FocusId = FocusId::new(2);
const INPUT_FOCUS: FocusId = FocusId::new(3);
const TREE_ROOT: u64 = 1;
const TREE_WIDGETS: u64 = 4;
const SPLASH_ARTWORK: &str = r"⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣀⣤⣤⣤⣤⡼⠀⢀⡀⣀⢱⡄⡀⠀⠀⠀⢲⣤⣤⣤⣤⣀⣀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣴⣾⣿⣿⣿⣿⣿⡿⠛⠋⠁⣤⣿⣿⣿⣧⣷⠀⠀⠘⠉⠛⢻⣷⣿⣽⣿⣿⣷⣦⣄⡀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⢀⣴⣞⣽⣿⣿⣿⣿⣿⣿⣿⠁⠀⠀⠠⣿⣿⡟⢻⣿⣿⣇⠀⠀⠀⠀⠀⣿⣿⣿⣿⣿⣿⣿⣿⣟⢦⡀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⣠⣿⡾⣿⣿⣿⣿⣿⠿⣻⣿⣿⡀⠀⠀⠀⢻⣿⣷⡀⠻⣧⣿⠆⠀⠀⠀⠀⣿⣿⣿⡻⣿⣿⣿⣿⣿⠿⣽⣦⡀⠀⠀⠀⠀
⠀⠀⠀⠀⣼⠟⣩⣾⣿⣿⣿⢟⣵⣾⣿⣿⣿⣧⠀⠀⠀⠈⠿⣿⣿⣷⣈⠁⠀⠀⠀⠀⣰⣿⣿⣿⣿⣮⣟⢯⣿⣿⣷⣬⡻⣷⡄⠀⠀⠀
⠀⠀⢀⡜⣡⣾⣿⢿⣿⣿⣿⣿⣿⢟⣵⣿⣿⣿⣷⣄⠀⣰⣿⣿⣿⣿⣿⣷⣄⠀⢀⣼⣿⣿⣿⣷⡹⣿⣿⣿⣿⣿⣿⢿⣿⣮⡳⡄⠀⠀
⠀⢠⢟⣿⡿⠋⣠⣾⢿⣿⣿⠟⢃⣾⢟⣿⢿⣿⣿⣿⣾⡿⠟⠻⣿⣻⣿⣏⠻⣿⣾⣿⣿⣿⣿⡛⣿⡌⠻⣿⣿⡿⣿⣦⡙⢿⣿⡝⣆⠀
⠀⢯⣿⠏⣠⠞⠋⠀⣠⡿⠋⢀⣿⠁⢸⡏⣿⠿⣿⣿⠃⢠⣴⣾⣿⣿⣿⡟⠀⠘⢹⣿⠟⣿⣾⣷⠈⣿⡄⠘⢿⣦⠀⠈⠻⣆⠙⣿⣜⠆
⢀⣿⠃⡴⠃⢀⡠⠞⠋⠀⠀⠼⠋⠀⠸⡇⠻⠀⠈⠃⠀⣧⢋⣼⣿⣿⣿⣷⣆⠀⠈⠁⠀⠟⠁⡟⠀⠈⠻⠀⠀⠉⠳⢦⡀⠈⢣⠈⢿⡄
⣸⠇⢠⣷⠞⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⠻⠿⠿⠋⠀⢻⣿⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠙⢾⣆⠈⣷
⡟⠀⡿⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣴⣶⣤⡀⢸⣿⠇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢻⡄⢹
⡇⠀⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⡇⠀⠈⣿⣼⡟⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠃⢸
⢡⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⠶⣶⡟⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡼
⠈⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⡾⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠁
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⡁⢠⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣿⣿⣼⣀⣠⠂⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppPhase {
    Splash,
    Main,
}

struct Splash {
    started: Instant,
    spinner: Animation<&'static str>,
}

impl Splash {
    fn new(started: Instant) -> Self {
        Self {
            started,
            spinner: Animation::new(["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"])
                .frame_duration(Duration::from_millis(100)),
        }
    }

    fn advance(&mut self, now: Instant) -> bool {
        self.spinner.update(now)
    }

    fn is_ready(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= SPLASH_DURATION
    }
}

struct App {
    phase: AppPhase,
    splash: Splash,
    dashboard: Dashboard,
    theme: Theme,
}

impl App {
    fn new(started: Instant) -> Self {
        Self::with_theme(started, Theme::default())
    }

    fn with_theme(started: Instant, theme: Theme) -> Self {
        Self {
            phase: AppPhase::Splash,
            splash: Splash::new(started),
            dashboard: Dashboard::with_theme(theme),
            theme,
        }
    }

    fn advance(&mut self, now: Instant) -> bool {
        match self.phase {
            AppPhase::Splash => {
                let spinner_changed = self.splash.advance(now);
                if self.splash.is_ready(now) {
                    self.phase = AppPhase::Main;
                    true
                } else {
                    spinner_changed
                }
            }
            AppPhase::Main => self.dashboard.advance(now),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        if matches!(key.code, KeyCode::Char(character) if is_quit_key(character))
            || (key.modifiers.ctrl && matches!(key.code, KeyCode::Char('c' | 'C')))
        {
            return KeyOutcome {
                quit: true,
                redraw: false,
            };
        }

        match self.phase {
            AppPhase::Splash if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) => {
                self.phase = AppPhase::Main;
                KeyOutcome {
                    quit: false,
                    redraw: true,
                }
            }
            AppPhase::Splash => KeyOutcome::default(),
            AppPhase::Main => self.dashboard.handle_key(key),
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> KeyOutcome {
        match self.phase {
            AppPhase::Splash => KeyOutcome::default(),
            AppPhase::Main => self.dashboard.handle_mouse(mouse),
        }
    }
}

fn seeded_output() -> Vec<String> {
    (1..=50)
        .map(|line| format!("[{line:03}] DragonsTUI output event"))
        .collect()
}

fn agent_tree() -> Tree {
    Tree::new([TreeNode::new(TREE_ROOT, "src").children([
        TreeNode::new(2, "main.rs"),
        TreeNode::new(3, "runtime.rs"),
        TreeNode::new(TREE_WIDGETS, "widgets")
            .children([TreeNode::new(5, "table.rs"), TreeNode::new(6, "tree.rs")]),
    ])])
}

fn dashboard_keymap() -> KeyMap {
    let mut keymap = KeyMap::new();
    keymap.bind(
        KeyCode::Tab,
        Default::default(),
        CommandId::new("focus-next"),
    );
    keymap.bind(
        KeyCode::Tab,
        dragons_tui::KeyModifiers {
            ctrl: false,
            alt: false,
            shift: true,
        },
        CommandId::new("focus-previous"),
    );
    keymap.bind(
        KeyCode::Char('p'),
        dragons_tui::KeyModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
        CommandId::new("command-palette"),
    );
    keymap
}

fn palette_commands() -> [PaletteCommand; 5] {
    [
        PaletteCommand::new(CommandId::new("focus-agents"), "Focus Agents"),
        PaletteCommand::new(CommandId::new("focus-output"), "Focus Output"),
        PaletteCommand::new(CommandId::new("focus-input"), "Focus Input"),
        PaletteCommand::new(CommandId::new("toggle-tree"), "Toggle Agent Tree"),
        PaletteCommand::new(CommandId::new("quit"), "Quit"),
    ]
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct KeyOutcome {
    quit: bool,
    redraw: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct HitRegions {
    agents: Rect,
    agent_rows: Rect,
    output: Rect,
    input: Rect,
}

struct Dashboard {
    theme: Theme,
    spinner: Spinner,
    dragon: Animation<&'static str>,
    ticks: u64,
    focus: FocusState,
    keymap: KeyMap,
    agents: TableState,
    tree: TreeState,
    show_tree: bool,
    modal_active: bool,
    modal_previous_focus: Option<FocusId>,
    palette: Option<CommandPalette>,
    palette_previous_focus: Option<FocusId>,
    input: TextArea,
    last_prompt: Option<String>,
    agent: Option<AgentProcess>,
    status: String,
    output: Vec<String>,
    output_viewport: ViewportState,
    hit_regions: HitRegions,
}

impl Dashboard {
    #[cfg(test)]
    fn new() -> Self {
        Self::with_theme(Theme::default())
    }

    fn with_theme(theme: Theme) -> Self {
        Self {
            theme,
            spinner: Spinner::braille(),
            dragon: Animation::new(["[=    ]", "[==   ]", "[ === ]", "[  ===]", "[   ==]"])
                .frame_duration(Duration::from_millis(250)),
            ticks: 0,
            focus: FocusState::new([AGENTS_FOCUS, OUTPUT_FOCUS, INPUT_FOCUS]),
            keymap: dashboard_keymap(),
            agents: TableState::new(),
            tree: {
                let mut tree = TreeState::new();
                tree.expand(TREE_ROOT);
                tree.expand(TREE_WIDGETS);
                tree
            },
            show_tree: false,
            modal_active: false,
            modal_previous_focus: None,
            palette: None,
            palette_previous_focus: None,
            input: TextArea::new(),
            last_prompt: None,
            agent: None,
            status: "Stopped".to_owned(),
            output: seeded_output(),
            output_viewport: ViewportState::new(),
            hit_regions: HitRegions::default(),
        }
    }

    fn advance(&mut self, now: Instant) -> bool {
        let animation_changed = self.spinner.update(now) | self.dragon.update(now);
        if animation_changed {
            self.ticks += 1;
        }
        animation_changed | self.poll_agent()
    }

    fn poll_agent(&mut self) -> bool {
        let Some(agent) = self.agent.as_mut() else {
            return false;
        };
        let (output, running) = (agent.poll(), agent.is_running());

        let mut changed = match output {
            Ok(output) => self.push_output(output),
            Err(error) => {
                let status_changed = self.status != "Error";
                self.status = "Error".to_owned();
                let output_changed = self.push_output([format!("[process error] {error}")]);
                status_changed || output_changed
            }
        };
        if !running && self.status != "Exited" {
            self.status = "Exited".to_owned();
            changed = true;
        }
        changed
    }

    fn start_or_stop_agent(&mut self) {
        if self.agent.as_ref().is_some_and(AgentProcess::is_running) {
            self.stop_agent();
            return;
        }

        match AgentProcess::start("hermes", &["--cli"]) {
            Ok(agent) => {
                self.agent = Some(agent);
                self.status = "Running".to_owned();
                self.push_output(["[started] hermes --cli".to_owned()]);
            }
            Err(error) => {
                self.status = "Start failed".to_owned();
                self.push_output([format!("[start error] {error}")]);
            }
        }
    }

    fn stop_agent(&mut self) {
        let Some(agent) = self.agent.as_mut() else {
            return;
        };

        match agent.stop() {
            Ok(()) => {
                self.status = "Stopped".to_owned();
                self.push_output(["[stopped] Hermes".to_owned()]);
            }
            Err(error) => {
                self.status = "Stop failed".to_owned();
                self.push_output([format!("[stop error] {error}")]);
            }
        }
    }

    fn submit_input(&mut self) {
        if self.input.text().is_empty() {
            return;
        }
        if self.agent.as_ref().is_none_or(|agent| !agent.is_running()) {
            self.push_output(["[not running] Start Hermes with Enter".to_owned()]);
            return;
        }

        let prompt = self.input.text().to_owned();
        let send_result = self
            .agent
            .as_mut()
            .expect("a running agent exists after the guard")
            .send(&prompt);
        match send_result {
            Ok(()) => {
                self.last_prompt = Some(prompt.clone());
                self.input.clear();
                self.push_output([format!("> {prompt}")]);
            }
            Err(error) => {
                self.status = "Send failed".to_owned();
                self.push_output([format!("[send error] {error}")]);
            }
        }
    }

    fn push_output(&mut self, lines: impl IntoIterator<Item = String>) -> bool {
        let mut changed = false;
        for line in lines {
            self.output.push(line);
            changed = true;
        }
        while self.output.len() > MAX_OUTPUT_LINES {
            self.output.remove(0);
        }
        changed
    }

    fn open_palette(&mut self) {
        self.palette_previous_focus = self.focus.current();
        self.palette = Some(CommandPalette::new(palette_commands()));
    }

    fn close_palette(&mut self) {
        self.palette = None;
        if let Some(focus) = self.palette_previous_focus.take() {
            self.focus.set_focus(focus);
        }
    }

    fn execute_command(&mut self, command: &str) -> Option<KeyOutcome> {
        let redraw = match command {
            "focus-next" => self.focus.focus_next(),
            "focus-previous" => self.focus.focus_previous(),
            "command-palette" => {
                self.open_palette();
                true
            }
            "focus-agents" => self.focus.set_focus(AGENTS_FOCUS),
            "focus-output" => self.focus.set_focus(OUTPUT_FOCUS),
            "focus-input" => self.focus.set_focus(INPUT_FOCUS),
            "toggle-tree" => {
                self.show_tree = !self.show_tree;
                true
            }
            "quit" => {
                return Some(KeyOutcome {
                    quit: true,
                    redraw: false,
                });
            }
            _ => return None,
        };
        Some(KeyOutcome {
            quit: false,
            redraw,
        })
    }

    fn execute_palette_selection(&mut self) -> KeyOutcome {
        let Some(command) = self
            .palette
            .as_ref()
            .and_then(CommandPalette::execute_selected)
        else {
            return KeyOutcome::default();
        };
        self.palette = None;
        self.palette_previous_focus = None;
        self.execute_command(command.as_str()).unwrap_or_default()
    }

    fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        if matches!(key.code, KeyCode::Char(character) if is_quit_key(character))
            || (key.modifiers.ctrl && matches!(key.code, KeyCode::Char('c' | 'C')))
        {
            return KeyOutcome {
                quit: true,
                redraw: false,
            };
        }

        if self.modal_active {
            if matches!(key.code, KeyCode::Escape | KeyCode::Enter) {
                self.modal_active = false;
                if let Some(focus) = self.modal_previous_focus.take() {
                    self.focus.set_focus(focus);
                }
            }
            return KeyOutcome {
                quit: false,
                redraw: true,
            };
        }

        if self.palette.is_some() {
            if matches!(key.code, KeyCode::Escape)
                || (key.modifiers.ctrl && matches!(key.code, KeyCode::Char('p' | 'P')))
            {
                self.close_palette();
                return KeyOutcome {
                    quit: false,
                    redraw: true,
                };
            }
            if matches!(key.code, KeyCode::Enter) {
                return self.execute_palette_selection();
            }
            return KeyOutcome {
                quit: false,
                redraw: self
                    .palette
                    .as_mut()
                    .is_some_and(|palette| palette.handle_key(key)),
            };
        }

        if matches!(key.code, KeyCode::Char('m')) {
            self.modal_active = true;
            self.modal_previous_focus = self.focus.current();
            return KeyOutcome {
                quit: false,
                redraw: true,
            };
        }

        if let Some(command) = self.keymap.resolve(key).cloned() {
            return self.execute_command(command.as_str()).unwrap_or_default();
        }

        match self.focus.current() {
            Some(id) if id == AGENTS_FOCUS => match key.code {
                KeyCode::Char('t') => {
                    self.show_tree = !self.show_tree;
                    KeyOutcome {
                        quit: false,
                        redraw: true,
                    }
                }
                KeyCode::Enter => {
                    if self.show_tree {
                        return KeyOutcome {
                            quit: false,
                            redraw: agent_tree().toggle(&mut self.tree),
                        };
                    }
                    self.start_or_stop_agent();
                    KeyOutcome {
                        quit: false,
                        redraw: true,
                    }
                }
                KeyCode::Char('x') => {
                    self.stop_agent();
                    KeyOutcome {
                        quit: false,
                        redraw: true,
                    }
                }
                KeyCode::Up => {
                    if self.show_tree {
                        agent_tree().move_up(&mut self.tree);
                    } else {
                        self.agents.previous(AGENTS.len());
                    }
                    KeyOutcome {
                        quit: false,
                        redraw: true,
                    }
                }
                KeyCode::Down => {
                    if self.show_tree {
                        agent_tree().move_down(&mut self.tree);
                    } else {
                        self.agents.next(AGENTS.len());
                    }
                    KeyOutcome {
                        quit: false,
                        redraw: true,
                    }
                }
                KeyCode::Left if self.show_tree => KeyOutcome {
                    quit: false,
                    redraw: agent_tree().move_left(&mut self.tree),
                },
                KeyCode::Right if self.show_tree => KeyOutcome {
                    quit: false,
                    redraw: agent_tree().move_right(&mut self.tree),
                },
                _ => KeyOutcome::default(),
            },
            Some(id) if id == OUTPUT_FOCUS => match key.code {
                KeyCode::Up => KeyOutcome {
                    quit: false,
                    redraw: self.output_viewport.scroll_up(),
                },
                KeyCode::Down => KeyOutcome {
                    quit: false,
                    redraw: self.output_viewport.scroll_down(),
                },
                KeyCode::PageUp => KeyOutcome {
                    quit: false,
                    redraw: self.output_viewport.page_up(),
                },
                KeyCode::PageDown => KeyOutcome {
                    quit: false,
                    redraw: self.output_viewport.page_down(),
                },
                KeyCode::Home => KeyOutcome {
                    quit: false,
                    redraw: self.output_viewport.home(),
                },
                KeyCode::End => KeyOutcome {
                    quit: false,
                    redraw: self.output_viewport.end(),
                },
                _ => KeyOutcome::default(),
            },
            Some(id) if id == INPUT_FOCUS => match key.code {
                KeyCode::Enter if key.modifiers.ctrl && !self.input.text().is_empty() => {
                    self.submit_input();
                    KeyOutcome {
                        quit: false,
                        redraw: true,
                    }
                }
                _ => KeyOutcome {
                    quit: false,
                    redraw: self.input.handle_key(key),
                },
            },
            _ => KeyOutcome::default(),
        }
    }
    fn handle_mouse(&mut self, mouse: MouseEvent) -> KeyOutcome {
        if self.modal_active || self.palette.is_some() {
            return KeyOutcome::default();
        }
        let position = Position {
            x: mouse.x,
            y: mouse.y,
        };

        match mouse.kind {
            MouseKind::LeftDown if self.hit_regions.agents.contains(position) => {
                let focus_changed = self.focus.set_focus(AGENTS_FOCUS);
                let selected_changed = self.select_agent_at(position);
                KeyOutcome {
                    quit: false,
                    redraw: focus_changed || selected_changed,
                }
            }
            MouseKind::LeftDown if self.hit_regions.output.contains(position) => {
                let changed = self.focus.set_focus(OUTPUT_FOCUS);
                KeyOutcome {
                    quit: false,
                    redraw: changed,
                }
            }
            MouseKind::LeftDown if self.hit_regions.input.contains(position) => {
                let changed = self.focus.set_focus(INPUT_FOCUS);
                KeyOutcome {
                    quit: false,
                    redraw: changed,
                }
            }
            MouseKind::ScrollUp if self.hit_regions.output.contains(position) => KeyOutcome {
                quit: false,
                redraw: self.output_viewport.scroll_up(),
            },
            MouseKind::ScrollDown if self.hit_regions.output.contains(position) => KeyOutcome {
                quit: false,
                redraw: self.output_viewport.scroll_down(),
            },
            _ => KeyOutcome::default(),
        }
    }

    fn select_agent_at(&mut self, position: Position) -> bool {
        if !self.hit_regions.agent_rows.contains(position) {
            return false;
        }

        let row = usize::from(position.y.saturating_sub(self.hit_regions.agent_rows.y));
        let Some(index) = row.checked_sub(1) else {
            return false;
        };
        if index >= AGENTS.len() {
            return false;
        }

        let previous = self.agents.selected_index(AGENTS.len());
        self.agents.set_selected(index);
        previous != Some(index)
    }
}

struct DemoView {
    frame: Frame,
    cursor: Option<Position>,
}

fn app_view(size: Size, app: &mut App) -> DemoView {
    match app.phase {
        AppPhase::Splash => splash_view(size, &app.splash, app.theme),
        AppPhase::Main => demo_view(size, &mut app.dashboard),
    }
}

fn splash_view(size: Size, splash: &Splash, theme: Theme) -> DemoView {
    let background = Style::new().bg(theme.background);
    let artwork_style = background.patch(Style::new().fg(theme.secondary).bold());
    let status_style = background.patch(Style::new().fg(theme.muted));
    let spinner_style = background.patch(Style::new().fg(theme.success).bold());
    let mut frame = Frame::new(size.width, size.height);

    for y in 0..size.height {
        for x in 0..size.width {
            frame.set_cell(x, y, Cell::new(' ', background));
        }
    }

    let artwork = splash_artwork_rect(size);
    Text::new(SPLASH_ARTWORK)
        .style(artwork_style)
        .alignment(Alignment::Center)
        .render(&mut frame, artwork);

    let artwork_height = u16::try_from(SPLASH_ARTWORK.lines().count()).unwrap_or(u16::MAX);
    let status_y = artwork.y.saturating_add(artwork_height).saturating_add(1);
    RichText::new([Line::from([
        Span::styled("Initializing... ", status_style),
        Span::styled(
            splash.spinner.current().copied().unwrap_or(""),
            spinner_style,
        ),
    ])])
    .alignment(Alignment::Center)
    .render(
        &mut frame,
        Rect::new(
            0,
            status_y,
            size.width,
            size.height.saturating_sub(status_y),
        ),
    );

    DemoView {
        frame,
        cursor: None,
    }
}

fn splash_artwork_rect(size: Size) -> Rect {
    let artwork_height = u16::try_from(SPLASH_ARTWORK.lines().count()).unwrap_or(u16::MAX);
    let total_height = artwork_height.saturating_add(2);
    let y = size.height.saturating_sub(total_height) / 2;
    Rect::new(0, y, size.width, size.height.saturating_sub(y))
}

fn demo_view(size: Size, dashboard: &mut Dashboard) -> DemoView {
    let theme = dashboard.theme;
    let background = Style::new().bg(theme.background);
    let border_style = background.patch(Style::new().fg(theme.primary).bold());
    let agent_style = background.patch(Style::new().fg(theme.secondary).bold());
    let success_style = background.patch(Style::new().fg(theme.success).bold());
    let runtime_style = background.patch(Style::new().fg(theme.error).bold());
    let focus_style = background.patch(Style::new().fg(theme.warning).bold());
    let text_style = background.patch(Style::new().fg(theme.text));
    let status_style = background.patch(Style::new().fg(theme.muted));
    let hint_style = status_style.patch(Style::new().italic());
    let selected_agent_style = Style::new().fg(theme.success).bg(theme.primary).bold();
    let mut frame = Frame::new(size.width, size.height);

    for y in 0..size.height {
        for x in 0..size.width {
            frame.set_cell(x, y, Cell::new(' ', background));
        }
    }

    let screen = Rect::new(0, 0, size.width, size.height);
    let inner = Panel::new("DragonsTUI")
        .border_style(border_style)
        .title_style(border_style)
        .render(&mut frame, screen);
    let prompt_height = if dashboard.focus.current() == Some(INPUT_FOCUS) {
        5
    } else {
        3
    };
    let root = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(prompt_height),
        Constraint::Length(1),
    ])
    .gap(1)
    .split(inner);
    let body = Layout::horizontal([
        Constraint::Length(20),
        Constraint::Fill(1),
        Constraint::Percentage(25),
    ])
    .gap(1)
    .split(root[0]);
    let agents = body[0];
    let activity = body[1];
    let runtime = body[2];
    let prompt = root[1];
    let footer = root[2];

    let agents_focused = dashboard.focus.current() == Some(AGENTS_FOCUS);
    let agents_inner = Panel::new("Agents")
        .border_style(if agents_focused {
            focus_style
        } else {
            agent_style
        })
        .title_style(if agents_focused {
            focus_style
        } else {
            agent_style
        })
        .render(&mut frame, agents);
    let output_focused = dashboard.focus.current() == Some(OUTPUT_FOCUS);
    let activity_inner = Panel::new("Output")
        .border_set(BorderSet::square())
        .border_style(if output_focused {
            focus_style
        } else {
            runtime_style
        })
        .title_style(if output_focused {
            focus_style
        } else {
            runtime_style
        })
        .render(&mut frame, activity);
    let runtime_inner = Panel::new("Runtime")
        .border_set(BorderSet::double())
        .border_style(runtime_style)
        .title_style(runtime_style)
        .render(&mut frame, runtime);
    let prompt_inner = Panel::new("Prompt")
        .border_style(if dashboard.focus.current() == Some(INPUT_FOCUS) {
            focus_style
        } else {
            border_style
        })
        .title_style(if dashboard.focus.current() == Some(INPUT_FOCUS) {
            focus_style
        } else {
            border_style
        })
        .render(&mut frame, prompt);

    dashboard.hit_regions = HitRegions {
        agents,
        agent_rows: agents_inner,
        output: activity,
        input: prompt,
    };

    if dashboard.show_tree {
        agent_tree().selected_style(selected_agent_style).render(
            &mut frame,
            agents_inner,
            &mut dashboard.tree,
        );
    } else {
        Table::new([
            TableColumn::new(Constraint::Length(6)),
            TableColumn::new(Constraint::Length(7)),
            TableColumn::new(Constraint::Length(4)),
            TableColumn::new(Constraint::Fill(1)),
        ])
        .header([
            Line::from("NAME"),
            Line::from("STATUS"),
            Line::from("TOKENS"),
            Line::from("TIME"),
        ])
        .rows(vec![
            vec![
                Line::from("Codex"),
                Line::from("Working"),
                Line::from("12.4K"),
                Line::from("32s"),
            ],
            vec![
                Line::from("Hermes"),
                Line::from("Idle"),
                Line::from("8.1K"),
                Line::from("-"),
            ],
            vec![
                Line::from("Claude"),
                Line::from("Review"),
                Line::from("15.0K"),
                Line::from("12s"),
            ],
            vec![
                Line::from("Local"),
                Line::from("Ready"),
                Line::from("2.4K"),
                Line::from("-"),
            ],
        ])
        .selected_style(selected_agent_style)
        .render(&mut frame, agents_inner, &mut dashboard.agents);
    }
    Viewport::new(&dashboard.output).style(text_style).render(
        &mut frame,
        activity_inner,
        &mut dashboard.output_viewport,
    );
    if let Some(last_prompt) = &dashboard.last_prompt {
        Text::new(format!("Last prompt: {last_prompt}"))
            .style(hint_style)
            .render(&mut frame, row(runtime_inner, 9));
    }
    Text::new(format!("Status: {}", dashboard.status))
        .style(status_style)
        .render(&mut frame, runtime_inner);
    let (indicator, state, state_style) = if dashboard
        .agent
        .as_ref()
        .is_some_and(AgentProcess::is_running)
    {
        ("●", "Working", success_style)
    } else {
        (
            "○",
            "Idle",
            status_style.patch(Style::new().dim().strikethrough()),
        )
    };
    RichText::new([
        Line::from([
            Span::styled("Hermes ", agent_style),
            Span::styled(format!("{indicator} "), state_style),
            Span::styled(state, state_style),
        ]),
        Line::from([
            Span::styled("Tick ", status_style),
            Span::styled(dashboard.ticks.to_string(), text_style),
            Span::styled(format!(" {}", dashboard.spinner.current()), agent_style),
        ]),
        Line::from([
            Span::styled("日本語 ", text_style),
            Span::styled("● ", success_style),
            Span::styled("Ready", success_style),
        ]),
        Line::from([
            Span::styled("Dragon ", status_style),
            Span::styled(
                dashboard.dragon.current().copied().unwrap_or(""),
                runtime_style,
            ),
        ]),
    ])
    .render(&mut frame, row(runtime_inner, 1));
    render_canvas_demo(&mut frame, runtime_inner, success_style);
    render_visualization_demo(
        &mut frame,
        runtime_inner,
        agent_style,
        focus_style,
        status_style,
        success_style,
    );

    Text::new("> ").style(text_style).render(
        &mut frame,
        Rect::new(
            prompt_inner.x,
            prompt_inner.y,
            prompt_inner.width.min(2),
            prompt_inner.height,
        ),
    );
    let input_rect = Rect::new(
        prompt_inner.x.saturating_add(2),
        prompt_inner.y,
        prompt_inner.width.saturating_sub(2),
        prompt_inner.height,
    );
    let input_cursor = dashboard.input.render(&mut frame, input_rect, text_style);
    Text::new("Ctrl+P palette • Tab focus • ↑↓ scroll/select • q/Ctrl+C quit")
        .style(hint_style)
        .render(&mut frame, footer);

    if dashboard.modal_active {
        Modal::new(
            "Permission",
            [
                Line::from("Execute command?"),
                Line::from(""),
                Line::from("$ cargo test"),
                Line::from(""),
                Line::from("Enter / Escape closes"),
            ],
        )
        .size(34, 9)
        .border_style(focus_style)
        .content_style(text_style)
        .render(&mut frame, Rect::new(0, 0, size.width, size.height));
    } else if let Some(palette) = &dashboard.palette {
        palette.render(
            &mut frame,
            Rect::new(0, 0, size.width, size.height),
            focus_style,
            text_style,
            selected_agent_style,
        );
    }

    DemoView {
        frame,
        cursor: (!dashboard.modal_active
            && dashboard.palette.is_none()
            && dashboard.focus.current() == Some(INPUT_FOCUS))
        .then_some(input_cursor)
        .flatten(),
    }
}

fn row(rect: Rect, offset: u16) -> Rect {
    Rect::new(
        rect.x,
        rect.y.saturating_add(offset),
        rect.width,
        rect.height.saturating_sub(offset),
    )
}

fn render_canvas_demo(frame: &mut Frame, runtime: Rect, style: Style) {
    let target = Rect::new(
        runtime.x,
        runtime.y.saturating_add(5),
        runtime.width,
        runtime.height.saturating_sub(5).min(4),
    );
    if target.width == 0 || target.height == 0 {
        return;
    }

    let mut canvas = Canvas::new(target.width, target.height);
    let (logical_width, logical_height) = (canvas.logical_width(), canvas.logical_height());
    canvas.draw_rect(0, 0, logical_width, logical_height);

    if logical_width > 4 && logical_height > 4 {
        canvas.draw_line(
            2,
            2,
            (logical_width - 3) as i32,
            (logical_height - 3) as i32,
        );

        let waveform = [1_u32, 3, 5, 8, 5, 3, 1];
        let inner_width = logical_width - 4;
        let inner_height = logical_height - 4;
        for (index, values) in waveform.windows(2).enumerate() {
            let x0 = 2 + index as u32 * inner_width.saturating_sub(1) / 6;
            let x1 = 2 + (index as u32 + 1) * inner_width.saturating_sub(1) / 6;
            let y0 = 2 + (8 - values[0]) * inner_height.saturating_sub(1) / 8;
            let y1 = 2 + (8 - values[1]) * inner_height.saturating_sub(1) / 8;
            canvas.draw_line(x0 as i32, y0 as i32, x1 as i32, y1 as i32);
        }
    }

    canvas.render(frame, target, style);
}

fn render_visualization_demo(
    frame: &mut Frame,
    runtime: Rect,
    progress_style: Style,
    gauge_style: Style,
    unfilled_style: Style,
    sparkline_style: Style,
) {
    let progress_target = Rect::new(
        runtime.x,
        runtime.y.saturating_add(9),
        runtime.width,
        runtime.height.saturating_sub(9).min(2),
    );
    ProgressBar::new(0.72)
        .filled_style(progress_style)
        .unfilled_style(unfilled_style)
        .label("Agent 72%")
        .render(frame, progress_target);

    let gauge_target = Rect::new(
        runtime.x,
        runtime.y.saturating_add(11),
        runtime.width,
        runtime.height.saturating_sub(11).min(2),
    );
    Gauge::new(0.61)
        .filled_style(gauge_style)
        .unfilled_style(unfilled_style)
        .label("Tokens 61%")
        .render(frame, gauge_target);

    Sparkline::new([1.0, 3.0, 5.0, 8.0, 5.0, 3.0, 1.0])
        .style(sparkline_style)
        .render(
            frame,
            Rect::new(
                runtime.x,
                runtime.y.saturating_add(13),
                runtime.width,
                runtime.height.saturating_sub(13).min(1),
            ),
        );
}

#[cfg(test)]
fn app_frame(size: Size, app: &mut App) -> Frame {
    app_view(size, app).frame
}

#[cfg(test)]
fn demo_frame(size: Size, dashboard: &mut Dashboard) -> Frame {
    demo_view(size, dashboard).frame
}

struct TerminalGuard {
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
    cursor_hidden: bool,
    mouse_capture_enabled: bool,
    restored: bool,
}

impl TerminalGuard {
    fn inactive() -> Self {
        Self {
            raw_mode_enabled: false,
            alternate_screen_enabled: false,
            cursor_hidden: false,
            mouse_capture_enabled: false,
            restored: false,
        }
    }

    fn enter(output: &mut impl Write) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut guard = Self::inactive();
        guard.raw_mode_enabled = true;

        if let Err(error) = guard.setup_terminal(output) {
            let _ = guard.restore(output);
            return Err(error);
        }

        Ok(guard)
    }

    fn setup_terminal(&mut self, output: &mut impl Write) -> io::Result<()> {
        execute!(output, EnterAlternateScreen)?;
        self.alternate_screen_enabled = true;
        execute!(output, Hide)?;
        self.cursor_hidden = true;
        execute!(output, EnableMouseCapture)?;
        self.mouse_capture_enabled = true;
        execute!(output, Clear(ClearType::All))
    }

    fn restore(&mut self, output: &mut impl Write) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }

        let mut result = Ok(());
        if self.mouse_capture_enabled {
            result = execute!(output, DisableMouseCapture);
            self.mouse_capture_enabled = false;
        }
        if self.cursor_hidden {
            let cursor_result = execute!(output, Show);
            if result.is_ok() {
                result = cursor_result;
            }
            self.cursor_hidden = false;
        }
        if self.alternate_screen_enabled {
            let screen_result = execute!(output, LeaveAlternateScreen);
            if result.is_ok() {
                result = screen_result;
            }
            self.alternate_screen_enabled = false;
        }
        if self.raw_mode_enabled {
            let raw_mode_result = terminal::disable_raw_mode();
            if result.is_ok() {
                result = raw_mode_result;
            }
            self.raw_mode_enabled = false;
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
    use dragons_tui::{
        Color, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseKind, Size, ViewportState, diff,
        display_width,
    };

    use super::{
        AGENTS, AGENTS_FOCUS, App, AppPhase, Dashboard, INPUT_FOCUS, OUTPUT_FOCUS, SPLASH_ARTWORK,
        SPLASH_DURATION, app_frame, demo_frame, demo_view, splash_artwork_rect,
    };

    #[test]
    fn demo_frame_uses_constraint_layout_borders_and_unicode_text() {
        let mut dashboard = Dashboard::new();
        let frame = demo_frame(Size::new(80, 24), &mut dashboard);
        let buffer = frame.buffer();
        let theme = dragons_tui::Theme::default();

        assert_eq!(buffer.get(0, 0).unwrap().character, '╭');
        assert_eq!(buffer.get(0, 0).unwrap().style.fg, Some(theme.primary));
        assert!(buffer.get(0, 0).unwrap().style.attributes.bold);
        assert_eq!(buffer.get(1, 1).unwrap().character, '╭');
        assert_eq!(buffer.get(22, 1).unwrap().character, '┌');
        assert_eq!(buffer.get(60, 1).unwrap().character, '╔');
        assert_eq!(buffer.get(1, 1).unwrap().style.fg, Some(theme.warning));
        assert_eq!(buffer.get(2, 2).unwrap().character, 'N');
        assert_eq!(buffer.get(61, 2).unwrap().character, 'S');
        assert_eq!(buffer.get(61, 5).unwrap().character, '日');
        assert!((0..buffer.height()).any(|y| {
            (0..buffer.width()).any(|x| buffer.get(x, y).is_some_and(|cell| cell.character == '>'))
        }));
        assert!((0..buffer.height()).any(|y| {
            (0..buffer.width()).any(|x| buffer.get(x, y).is_some_and(|cell| cell.character == 'T'))
        }));
    }

    #[test]
    fn demo_uses_theme_colours_custom_border_sets_and_a_strikethrough_rich_span() {
        let mut dashboard = Dashboard::new();
        let frame = demo_frame(Size::new(80, 24), &mut dashboard);
        let buffer = frame.buffer();
        let theme = dragons_tui::Theme::default();

        assert_eq!(buffer.get(0, 0).unwrap().style.fg, Some(theme.primary));
        assert_eq!(buffer.get(22, 1).unwrap().character, '┌');
        assert_eq!(buffer.get(60, 1).unwrap().character, '╔');
        assert_eq!(buffer.get(60, 1).unwrap().style.fg, Some(theme.error));
        assert!(buffer.get(69, 3).unwrap().style.attributes.strikethrough);
    }

    #[test]
    fn demo_uses_fire_theme_background_and_selected_agent_semantics() {
        let mut dashboard = Dashboard::new();
        let frame = demo_frame(Size::new(80, 24), &mut dashboard);
        let buffer = frame.buffer();
        let theme = dragons_tui::Theme::default();

        assert_eq!(buffer.get(0, 0).unwrap().style.bg, Some(theme.background));
        assert_eq!(buffer.get(2, 3).unwrap().character, 'C');
        assert_eq!(buffer.get(2, 3).unwrap().style.fg, Some(theme.success));
        assert_eq!(buffer.get(2, 3).unwrap().style.bg, Some(theme.primary));
        assert_eq!(buffer.get(61, 7).unwrap().style.fg, Some(theme.success));
        assert_eq!(buffer.get(61, 7).unwrap().style.bg, Some(theme.background));
    }

    #[test]
    fn app_applies_a_supplied_theme_to_the_splash_and_dashboard() {
        let theme = dragons_tui::Theme {
            background: Color::rgb(1, 2, 3),
            primary: Color::rgb(4, 5, 6),
            secondary: Color::rgb(7, 8, 9),
            success: Color::rgb(10, 11, 12),
            warning: Color::rgb(13, 14, 15),
            error: Color::rgb(16, 17, 18),
            surface: Color::rgb(19, 20, 21),
            text: Color::rgb(22, 23, 24),
            muted: Color::rgb(25, 26, 27),
        };
        let start = std::time::Instant::now();
        let mut app = App::with_theme(start, theme);

        let splash = app_frame(Size::new(120, 40), &mut app);
        assert_eq!(
            splash.buffer().get(0, 0).unwrap().style.bg,
            Some(theme.background)
        );
        assert!((0..splash.buffer().height()).any(|y| {
            (0..splash.buffer().width()).any(|x| {
                splash.buffer().get(x, y).is_some_and(|cell| {
                    cell.character == '⣿' && cell.style.fg == Some(theme.secondary)
                })
            })
        }));

        app.handle_key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::default(),
        });
        let dashboard = app_frame(Size::new(80, 24), &mut app);
        assert_eq!(
            dashboard.buffer().get(0, 0).unwrap().style.bg,
            Some(theme.background)
        );
        assert_eq!(
            dashboard.buffer().get(0, 0).unwrap().style.fg,
            Some(theme.primary)
        );
    }

    #[test]
    fn demo_renders_a_theme_styled_multi_character_dragon_animation_frame() {
        let mut dashboard = Dashboard::new();
        let frame = demo_frame(Size::new(80, 24), &mut dashboard);
        let buffer = frame.buffer();

        assert_eq!(buffer.get(68, 6).unwrap().character, '[');
        assert_eq!(buffer.get(69, 6).unwrap().character, '=');
    }

    #[test]
    fn demo_renders_a_theme_styled_braille_canvas_rectangle_diagonal_and_waveform() {
        let mut dashboard = Dashboard::new();
        let frame = demo_frame(Size::new(80, 24), &mut dashboard);
        let buffer = frame.buffer();
        let theme = dragons_tui::Theme::default();

        assert_eq!(buffer.get(61, 7).unwrap().character, '⡏');
        assert_eq!(buffer.get(61, 7).unwrap().style.fg, Some(theme.success));
        assert!(
            buffer
                .get(61, 8)
                .is_some_and(|cell| matches!(cell.character, '⡇' | '⡗' | '⣇' | '⣗'))
        );
        assert!(buffer.get(69, 8).is_some_and(|cell| cell.character != ' '));
    }

    #[test]
    fn demo_renders_dragonfire_progress_gauge_and_activity_sparkline() {
        let mut dashboard = Dashboard::new();
        let frame = demo_frame(Size::new(80, 24), &mut dashboard);
        let buffer = frame.buffer();
        let theme = dragons_tui::Theme::default();

        let mut cells = (0..buffer.height())
            .flat_map(|y| (0..buffer.width()).filter_map(move |x| buffer.get(x, y)));
        assert!(
            cells
                .clone()
                .any(|cell| { cell.character == '█' && cell.style.fg == Some(theme.secondary) })
        );
        assert!(
            cells
                .clone()
                .any(|cell| { cell.character == '█' && cell.style.fg == Some(theme.warning) })
        );
        assert!(
            cells.any(|cell| { cell.character == '▁' && cell.style.fg == Some(theme.success) })
        );
    }

    #[test]
    fn dashboard_tick_requests_redraw_only_when_an_animation_frame_changes() {
        let start = std::time::Instant::now();
        let mut dashboard = Dashboard::new();

        assert!(!dashboard.advance(start));
        assert!(!dashboard.advance(start + std::time::Duration::from_millis(50)));
        assert!(dashboard.advance(start + std::time::Duration::from_millis(100)));
        assert!(!dashboard.advance(start + std::time::Duration::from_millis(150)));
        assert!(dashboard.advance(start + std::time::Duration::from_millis(200)));
    }

    #[test]
    fn demo_frame_handles_responsive_and_tiny_terminals() {
        for size in [
            Size::new(120, 40),
            Size::new(80, 24),
            Size::new(40, 15),
            Size::new(20, 8),
            Size::new(5, 3),
            Size::new(1, 1),
        ] {
            let mut dashboard = Dashboard::new();
            let frame = demo_frame(size, &mut dashboard);

            assert_eq!(frame.buffer().width(), size.width);
            assert_eq!(frame.buffer().height(), size.height);
        }
    }

    #[test]
    fn demo_frame_renders_agent_status_and_collected_output() {
        let mut dashboard = Dashboard::new();
        dashboard.output.clear();
        dashboard.output_viewport = ViewportState::new();
        dashboard.status = "Running".to_owned();
        dashboard.push_output(["agent output".to_owned()]);

        let frame = demo_frame(Size::new(80, 24), &mut dashboard);
        let buffer = frame.buffer();

        assert_eq!(buffer.get(23, 2).unwrap().character, 'a');
        assert_eq!(buffer.get(61, 2).unwrap().character, 'S');
    }

    #[test]
    fn demo_runtime_uses_mixed_style_rich_text() {
        let mut dashboard = Dashboard::new();
        let frame = demo_frame(Size::new(80, 24), &mut dashboard);
        let buffer = frame.buffer();
        let theme = dragons_tui::Theme::default();

        let name = buffer.get(61, 3).unwrap();
        assert_eq!(name.character, 'H');
        assert_eq!(name.style.fg, Some(theme.secondary));
        assert!(name.style.attributes.bold);

        let idle = buffer.get(68, 3).unwrap();
        assert_eq!(idle.character, '○');
        assert!(idle.style.attributes.dim);
        assert_eq!(buffer.get(61, 5).unwrap().character, '日');
        assert_eq!(buffer.get(68, 5).unwrap().character, '●');
    }

    #[test]
    fn dashboard_routes_focus_navigation_and_ctrl_c() {
        let mut dashboard = Dashboard::new();
        assert_eq!(dashboard.focus.current(), Some(AGENTS_FOCUS));

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Down,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.agents.selected_index(1), Some(0));

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(OUTPUT_FOCUS));
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(AGENTS_FOCUS));

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers {
                        ctrl: true,
                        alt: false,
                        shift: false,
                    },
                })
                .quit
        );
    }

    #[test]
    fn framework_focus_state_routes_tab_shift_tab_mouse_and_cursor_visibility() {
        let mut dashboard = Dashboard::new();
        assert_eq!(dashboard.focus.current(), Some(AGENTS_FOCUS));
        assert_eq!(demo_view(Size::new(80, 24), &mut dashboard).cursor, None);

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(OUTPUT_FOCUS));
        assert_eq!(demo_view(Size::new(80, 24), &mut dashboard).cursor, None);

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        assert!(
            demo_view(Size::new(80, 24), &mut dashboard)
                .cursor
                .is_some()
        );

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers {
                        shift: true,
                        ..KeyModifiers::default()
                    },
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(OUTPUT_FOCUS));
        assert_eq!(demo_view(Size::new(80, 24), &mut dashboard).cursor, None);

        let input = dashboard.hit_regions.input.position();
        assert!(
            dashboard
                .handle_mouse(MouseEvent {
                    x: input.x,
                    y: input.y,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        assert!(
            demo_view(Size::new(80, 24), &mut dashboard)
                .cursor
                .is_some()
        );
    }

    #[test]
    fn mouse_clicks_focus_interactive_regions_and_select_agents() {
        let mut dashboard = Dashboard::new();
        let _ = demo_frame(Size::new(80, 24), &mut dashboard);
        dashboard.focus.set_focus(OUTPUT_FOCUS);

        let agent_row = dashboard.hit_regions.agent_rows.position();
        assert!(
            dashboard
                .handle_mouse(MouseEvent {
                    x: agent_row.x,
                    y: agent_row.y,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(AGENTS_FOCUS));
        assert_eq!(dashboard.agents.selected_index(AGENTS.len()), Some(0));

        let output = dashboard.hit_regions.output.position();
        assert!(
            dashboard
                .handle_mouse(MouseEvent {
                    x: output.x,
                    y: output.y,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(OUTPUT_FOCUS));

        let input = dashboard.hit_regions.input.position();
        assert!(
            dashboard
                .handle_mouse(MouseEvent {
                    x: input.x,
                    y: input.y,
                    kind: MouseKind::LeftDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        assert!(dashboard.handle_key(KeyEvent::character('İ')).redraw);
        assert_eq!(dashboard.input.text(), "İ");
    }

    #[test]
    fn mouse_wheel_only_scrolls_output_and_ignores_noninteractive_events() {
        let mut dashboard = Dashboard::new();
        let _ = demo_frame(Size::new(80, 24), &mut dashboard);
        let output = dashboard.hit_regions.output.position();
        assert!(dashboard.focus.set_focus(INPUT_FOCUS));
        assert!(dashboard.output_viewport.end());
        let bottom = dashboard.output_viewport.offset();

        assert!(
            dashboard
                .handle_mouse(MouseEvent {
                    x: output.x,
                    y: output.y,
                    kind: MouseKind::ScrollUp,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.output_viewport.offset(), bottom - 1);
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        assert!(!dashboard.output_viewport.is_at_bottom());

        assert!(
            dashboard
                .handle_mouse(MouseEvent {
                    x: output.x,
                    y: output.y,
                    kind: MouseKind::ScrollDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert!(dashboard.output_viewport.is_at_bottom());

        let unchanged = dashboard.output_viewport.offset();
        for kind in [
            MouseKind::LeftDown,
            MouseKind::ScrollUp,
            MouseKind::Move,
            MouseKind::Drag(dragons_tui::MouseButton::Left),
        ] {
            assert!(
                !dashboard
                    .handle_mouse(MouseEvent {
                        x: 0,
                        y: 0,
                        kind,
                        modifiers: KeyModifiers::default(),
                    })
                    .redraw
            );
            assert_eq!(dashboard.output_viewport.offset(), unchanged);
            assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        }
    }

    #[test]
    fn mouse_events_are_safe_when_the_dashboard_has_tiny_or_empty_regions() {
        let mut dashboard = Dashboard::new();
        let _ = demo_frame(Size::new(1, 1), &mut dashboard);

        for kind in [
            MouseKind::LeftDown,
            MouseKind::ScrollUp,
            MouseKind::ScrollDown,
        ] {
            assert!(
                !dashboard
                    .handle_mouse(MouseEvent {
                        x: u16::MAX,
                        y: u16::MAX,
                        kind,
                        modifiers: KeyModifiers::default(),
                    })
                    .redraw
            );
        }
    }

    #[test]
    fn dashboard_routes_output_scroll_keys_without_regressing_other_focuses() {
        let mut dashboard = Dashboard::new();
        let _ = demo_frame(Size::new(80, 24), &mut dashboard);

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Down,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.agents.selected_index(1), Some(0));

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(OUTPUT_FOCUS));

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Down,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.output_viewport.offset(), 1);
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::PageDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert!(dashboard.output_viewport.offset() > 1);
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::PageUp,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.output_viewport.offset(), 1);
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::PageDown,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::End,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert!(dashboard.output_viewport.is_at_bottom());
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Home,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.output_viewport.offset(), 0);

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        assert!(dashboard.handle_key(KeyEvent::character('İ')).redraw);
        assert_eq!(dashboard.input.text(), "İ");
    }

    #[test]
    fn demo_resize_keeps_output_viewport_within_its_valid_range() {
        let mut dashboard = Dashboard::new();
        let _ = demo_frame(Size::new(80, 24), &mut dashboard);
        assert!(dashboard.output_viewport.end());

        for size in [
            Size::new(120, 40),
            Size::new(80, 24),
            Size::new(40, 15),
            Size::new(20, 8),
            Size::new(5, 3),
            Size::new(1, 1),
            Size::new(80, 24),
        ] {
            let _ = demo_frame(size, &mut dashboard);
            assert!(dashboard.output_viewport.offset() <= dashboard.output_viewport.max_scroll());
        }
        assert!(dashboard.output_viewport.is_at_bottom());
    }

    #[test]
    fn dashboard_quits_with_q_and_ctrl_c_in_every_focus_without_mutating_input() {
        let ctrl_c = KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers {
                ctrl: true,
                alt: false,
                shift: false,
            },
        };

        for focus in [AGENTS_FOCUS, OUTPUT_FOCUS, INPUT_FOCUS] {
            for key in [KeyEvent::character('q'), ctrl_c] {
                let mut dashboard = Dashboard::new();
                dashboard.focus.set_focus(focus);
                dashboard.input.insert('p');

                let outcome = dashboard.handle_key(key);

                assert!(outcome.quit, "{key:?} should quit from {focus:?}");
                assert!(!outcome.redraw);
                assert_eq!(dashboard.input.text(), "p");
            }
        }
    }

    #[test]
    fn dashboard_scrolls_output_and_preserves_unsent_input_when_stopped() {
        let mut dashboard = Dashboard::new();
        dashboard.output = (0..30).map(|line| format!("line {line}")).collect();
        dashboard.output_viewport = ViewportState::new();
        let _ = demo_frame(Size::new(80, 24), &mut dashboard);

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(OUTPUT_FOCUS));
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::End,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Up,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert!(!dashboard.output_viewport.is_at_bottom());

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::default(),
                })
                .redraw
        );
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        assert!(dashboard.handle_key(KeyEvent::character('p')).redraw);
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers {
                        ctrl: true,
                        alt: false,
                        shift: false,
                    },
                })
                .redraw
        );
        assert_eq!(dashboard.input.text(), "p");
        assert_eq!(dashboard.last_prompt, None);
        assert_eq!(
            dashboard.output.last().map(String::as_str),
            Some("[not running] Start Hermes with Enter")
        );
    }

    #[test]
    fn app_starts_in_splash_animates_and_enters_main_after_its_minimum_duration() {
        let start = std::time::Instant::now();
        let mut app = App::new(start);

        assert_eq!(app.phase, AppPhase::Splash);
        assert!(!app.advance(start));
        assert!(app.advance(start + std::time::Duration::from_millis(100)));
        assert_eq!(app.phase, AppPhase::Splash);
        assert!(app.advance(start + SPLASH_DURATION));
        assert_eq!(app.phase, AppPhase::Main);
    }

    #[test]
    fn splash_supports_enter_space_and_quit_shortcuts_without_reaching_dashboard_actions() {
        let start = std::time::Instant::now();
        for key in [
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::default(),
            },
            KeyEvent::character(' '),
        ] {
            let mut app = App::new(start);
            let outcome = app.handle_key(key);
            assert!(!outcome.quit);
            assert!(outcome.redraw);
            assert_eq!(app.phase, AppPhase::Main);
        }

        let ctrl_c = KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers {
                ctrl: true,
                alt: false,
                shift: false,
            },
        };
        for key in [KeyEvent::character('q'), ctrl_c] {
            let mut app = App::new(start);
            let outcome = app.handle_key(key);
            assert!(outcome.quit);
            assert!(!outcome.redraw);
            assert_eq!(app.phase, AppPhase::Splash);
        }
    }

    #[test]
    fn splash_centers_clips_and_transitions_without_leaving_artwork_cells_in_main() {
        let start = std::time::Instant::now();
        let mut app = App::new(start);
        assert_eq!(splash_artwork_rect(Size::new(120, 40)).y, 11);

        for size in [
            Size::new(120, 40),
            Size::new(80, 24),
            Size::new(40, 15),
            Size::new(20, 8),
            Size::new(5, 3),
            Size::new(1, 1),
            Size::new(80, 24),
        ] {
            let frame = app_frame(size, &mut app);
            assert_eq!(
                (frame.buffer().width(), frame.buffer().height()),
                (size.width, size.height)
            );
        }

        let splash = app_frame(Size::new(120, 40), &mut app);
        let splash_buffer = splash.buffer();
        let artwork = (0..splash_buffer.height())
            .flat_map(|y| (0..splash_buffer.width()).map(move |x| (x, y)))
            .find(|&(x, y)| {
                splash_buffer
                    .get(x, y)
                    .is_some_and(|cell| cell.character == '⣿')
            })
            .expect("the supplied dragon artwork is rendered in the splash frame");

        assert!(app.advance(start + SPLASH_DURATION));
        let main = app_frame(Size::new(120, 40), &mut app);
        assert_eq!(main.buffer().get(0, 0).unwrap().character, '╭');
        assert_ne!(
            main.buffer().get(artwork.0, artwork.1).unwrap().character,
            '⣿'
        );
        assert!(
            diff(Some(splash.buffer()), main.buffer())
                .iter()
                .any(|change| (change.x, change.y) == artwork)
        );
        assert!(
            app.handle_key(KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::default(),
            })
            .redraw
        );
    }

    #[test]
    fn splash_uses_surface_background_centered_artwork_and_a_styled_initializing_spinner() {
        let start = std::time::Instant::now();
        let mut app = App::new(start);
        let size = Size::new(120, 40);
        let frame = app_frame(size, &mut app);
        let buffer = frame.buffer();
        let theme = dragons_tui::Theme::default();
        let artwork = splash_artwork_rect(size);
        let first_line = SPLASH_ARTWORK.lines().next().unwrap();
        let first_x = size
            .width
            .saturating_sub(u16::try_from(display_width(first_line)).unwrap())
            / 2;
        let status_y = artwork
            .y
            .saturating_add(u16::try_from(SPLASH_ARTWORK.lines().count()).unwrap())
            .saturating_add(1);

        assert_eq!(buffer.get(0, 0).unwrap().character, ' ');
        assert_eq!(buffer.get(0, 0).unwrap().style.bg, Some(theme.background));
        assert_eq!(buffer.get(first_x, artwork.y).unwrap().character, '⠀');
        assert_eq!(
            buffer.get(first_x, artwork.y).unwrap().style.fg,
            Some(theme.secondary)
        );
        assert!(
            buffer
                .get(0, status_y)
                .is_none_or(|cell| cell.character != 'I')
        );
        assert!((0..size.width).any(|x| {
            buffer
                .get(x, status_y)
                .is_some_and(|cell| cell.character == 'I' && cell.style.fg == Some(theme.muted))
        }));
        assert!((0..size.width).any(|x| {
            buffer.get(x, status_y).is_some_and(|cell| {
                matches!(
                    cell.character,
                    '⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧'
                ) && cell.style.fg == Some(theme.success)
            })
        }));
    }

    struct FailingWriter;

    impl std::io::Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected setup output failure"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct ClearFailWriter {
        bytes: Vec<u8>,
        failed: bool,
    }

    impl std::io::Write for ClearFailWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            if !self.failed && buffer.windows(4).any(|window| window == b"\x1b[2J") {
                self.failed = true;
                return Err(std::io::Error::other("injected clear failure"));
            }
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn terminal_guard_disables_raw_mode_when_setup_output_fails() {
        use std::io::IsTerminal;

        if !std::io::stdin().is_terminal() {
            return;
        }

        let mut output = FailingWriter;
        assert!(super::TerminalGuard::enter(&mut output).is_err());
        assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    }

    #[test]
    fn terminal_guard_emits_mouse_capture_lifecycle_with_screen_cleanup() {
        let mut output = Vec::new();
        let mut guard = super::TerminalGuard::inactive();

        guard.setup_terminal(&mut output).unwrap();
        guard.restore(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\u{1b}[?1000h"));
        assert!(output.contains("\u{1b}[?1000l"));
        assert!(output.contains("\u{1b}[?1049h"));
        assert!(output.contains("\u{1b}[?1049l"));
        assert!(output.contains("\u{1b}[?25h"));
    }

    #[test]
    fn terminal_guard_disables_mouse_capture_after_late_setup_failure() {
        let mut output = ClearFailWriter {
            bytes: Vec::new(),
            failed: false,
        };
        let mut guard = super::TerminalGuard::inactive();

        assert!(guard.setup_terminal(&mut output).is_err());
        assert!(guard.mouse_capture_enabled);
        guard.restore(&mut output).unwrap();

        let output = String::from_utf8(output.bytes).unwrap();
        assert!(output.contains("\u{1b}[?1000h"));
        assert!(output.contains("\u{1b}[?1000l"));
        assert!(output.contains("\u{1b}[?1049l"));
    }

    #[test]
    fn terminal_guard_restores_raw_mode_after_a_run_error() {
        use std::io::IsTerminal;

        if !std::io::stdin().is_terminal() {
            return;
        }

        let mut output = Vec::new();
        let mut guard = super::TerminalGuard::enter(&mut output).unwrap();
        let run_result: std::io::Result<()> = Err(std::io::Error::other("injected run failure"));
        let result = run_result.and(guard.restore(&mut output));

        assert!(result.is_err());
        assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\u{1b}[?1049h"));
        assert!(output.contains("\u{1b}[?1049l"));
        assert!(output.contains("\u{1b}[?25h"));
    }

    #[test]
    fn modal_isolates_input_and_restores_the_previous_focus_on_escape() {
        let mut dashboard = Dashboard::new();
        dashboard.focus.set_focus(INPUT_FOCUS);

        assert!(dashboard.handle_key(KeyEvent::character('m')).redraw);
        assert!(dashboard.modal_active);
        assert!(
            demo_view(Size::new(80, 24), &mut dashboard)
                .cursor
                .is_none()
        );

        dashboard.handle_key(KeyEvent::character('x'));
        assert!(dashboard.modal_active);
        assert!(dashboard.input.text().is_empty());

        dashboard.handle_key(KeyEvent {
            code: KeyCode::Escape,
            modifiers: Default::default(),
        });
        assert!(!dashboard.modal_active);
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));
        assert!(
            demo_view(Size::new(80, 24), &mut dashboard)
                .cursor
                .is_some()
        );
    }

    #[test]
    fn command_palette_isolates_input_restores_escape_focus_and_executes_filtered_commands() {
        let ctrl_p = KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers {
                ctrl: true,
                alt: false,
                shift: false,
            },
        };
        let mut dashboard = Dashboard::new();
        dashboard.focus.set_focus(INPUT_FOCUS);

        assert!(dashboard.handle_key(ctrl_p).redraw);
        assert!(dashboard.palette.is_some());
        assert!(
            demo_view(Size::new(80, 24), &mut dashboard)
                .cursor
                .is_none()
        );
        dashboard.handle_key(KeyEvent::character('x'));
        assert!(dashboard.input.text().is_empty());

        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Escape,
                    modifiers: Default::default(),
                })
                .redraw
        );
        assert!(dashboard.palette.is_none());
        assert_eq!(dashboard.focus.current(), Some(INPUT_FOCUS));

        dashboard.handle_key(ctrl_p);
        for character in "output".chars() {
            assert!(dashboard.handle_key(KeyEvent::character(character)).redraw);
        }
        assert!(
            dashboard
                .handle_key(KeyEvent {
                    code: KeyCode::Enter,
                    modifiers: Default::default(),
                })
                .redraw
        );
        assert!(dashboard.palette.is_none());
        assert_eq!(dashboard.focus.current(), Some(OUTPUT_FOCUS));
    }

    #[test]
    fn command_palette_renders_safely_during_the_required_resize_sequence() {
        let mut dashboard = Dashboard::new();
        dashboard.handle_key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers {
                ctrl: true,
                alt: false,
                shift: false,
            },
        });

        for size in [
            Size::new(120, 40),
            Size::new(80, 24),
            Size::new(40, 15),
            Size::new(20, 8),
            Size::new(5, 3),
            Size::new(1, 1),
            Size::new(80, 24),
        ] {
            let frame = demo_frame(size, &mut dashboard);
            assert_eq!(
                (frame.buffer().width(), frame.buffer().height()),
                (size.width, size.height)
            );
        }
    }
}
