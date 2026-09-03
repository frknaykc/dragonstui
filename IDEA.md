DragonsTUI — IDEA.md

Project Overview

DragonsTUI is a high-performance, highly customizable terminal user interface engine written in Rust.

The long-term goal is to use DragonsTUI as the foundation for a terminal-native AI agent control center where multiple AI coding agents and agent runtimes can be launched, monitored, controlled, and orchestrated from a single interface.

Examples of agents that may eventually be integrated include:

* Hermes Agent
* Codex CLI
* Claude Code
* OpenCode
* Gemini CLI
* Custom local or remote agents

DragonsTUI should not depend on an existing high-level TUI framework such as Ratatui, Bubble Tea, Textual, or OpenTUI.

Those projects may be studied as architectural references, but DragonsTUI should own its rendering model, component system, event handling, styling, animation, and terminal abstraction.

⸻

Core Vision

DragonsTUI should feel closer to a modern desktop UI framework than a traditional terminal application.

The terminal should be treated as a render target rather than as a limitation.

The framework should eventually support:

* rich colors
* Unicode graphics
* ASCII art
* animations
* custom widgets
* layouts
* panels
* modals
* tabs
* scrolling
* keyboard navigation
* mouse input
* responsive resizing
* syntax highlighting
* diff viewers
* markdown rendering
* graphs
* progress indicators
* custom themes
* agent activity visualizations

Performance should remain a first-class concern.

The UI should remain responsive while multiple external processes, AI agents, streams, animations, and background operations are active.

⸻

Primary Goals

1. Build a Custom Rust TUI Engine

DragonsTUI should implement its own core TUI runtime.

The initial engine should include:

* terminal initialization and restoration
* raw mode
* alternate screen support
* keyboard events
* terminal resize events
* screen buffer
* frame rendering
* cell representation
* text rendering
* colors and text styles
* basic layout
* component/widget abstraction
* event loop
* efficient terminal updates

The engine should avoid redrawing the entire terminal when unnecessary.

A previous-frame/current-frame buffer comparison should eventually allow only changed cells to be written to the terminal.

Conceptually:

Application State
       |
       v
   UI Render
       |
       v
 Current Frame
       |
       | diff
       v
Previous Frame
       |
       v
Changed Cells
       |
       v
ANSI Output
       |
       v
Terminal

⸻

2. Extreme Customizability

DragonsTUI should allow applications to control presentation at a low level without forcing a rigid design system.

Styling should eventually support:

* RGB / true color
* foreground colors
* background colors
* bold
* dim
* italic
* underline
* strikethrough where supported
* borders
* padding
* margins
* alignment
* themes

Possible future API direction:

Style::new()
    .fg(Color::rgb(140, 200, 255))
    .bg(Color::rgb(20, 22, 30))
    .bold();

The exact API is not fixed yet.

Avoid prematurely designing a large public API before real application requirements exist.

⸻

3. Animation Support

Animations are an important part of the project.

DragonsTUI should eventually make it easy to implement:

* spinners
* animated status indicators
* progress animations
* loading effects
* pulsing text
* animated ASCII art
* Braille-based graphics
* charts
* waveform-style visualizations
* transitions between UI states

Animations must not require the entire screen to be unnecessarily redrawn.

The runtime should eventually support controlled tick/update scheduling.

Example conceptual API:

Animation::new()
    .frames(["◐", "◓", "◑", "◒"])
    .fps(12);

This API is only illustrative and should not be implemented unless it naturally fits the architecture.

⸻

4. Performance

Performance is a core design requirement.

DragonsTUI should aim for:

* low startup latency
* low memory overhead
* minimal allocations during rendering where practical
* efficient frame diffing
* minimal ANSI output
* responsive input handling
* smooth animation
* efficient handling of streaming data
* predictable behavior under heavy event load

Do not optimize blindly.

Correctness and architectural simplicity should come first.

Performance optimizations should be based on measurable bottlenecks.

⸻

5. Multi-Agent Control Center

Once the TUI engine is sufficiently usable, DragonsTUI will be used to build an AI agent control interface.

The envisioned interface may look conceptually like:

┌ Agents ────────────┬ Active Agent ────────────────────────────┐
│                    │                                          │
│ ● Codex            │ Codex                                    │
│ ● Hermes           │                                          │
│ ○ Claude           │ ● Reading src/auth.rs                    │
│ ○ Local Agent      │ ● Searching project                      │
│                    │ ✓ Modified middleware.rs                 │
│                    │                                          │
│                    │ Working on authentication...             │
│                    │                                          │
├────────────────────┼──────────────────────────────────────────┤
│ Tasks              │ Diff                                     │
│                    │                                          │
│ ● Authentication   │ - old implementation                    │
│ ○ Tests            │ + new implementation                    │
│ ○ Review           │                                          │
├────────────────────┴──────────────────────────────────────────┤
│ > Send message to Codex...                                    │
└───────────────────────────────────────────────────────────────┘

This UI is not part of the first milestone.

It represents the long-term application that will validate and drive the framework.

⸻

Agent Architecture

The TUI should not directly depend on the implementation details of any specific AI agent.

Different agents should eventually be connected through adapters.

Conceptually:

                         DragonsTUI
                             |
                       Agent Manager
                             |
           +-----------------+-----------------+
           |                 |                 |
           v                 v                 v
      CodexAdapter      HermesAdapter     ClaudeAdapter
           |                 |                 |
           v                 v                 v
       codex CLI         Hermes Agent      Claude Code

A future shared abstraction might resemble:

trait Agent {
    fn start(&mut self);
    fn send(&mut self, message: &str);
    fn interrupt(&mut self);
    fn stop(&mut self);
}

The exact trait design should not be finalized until at least two real agent integrations exist.

Avoid speculative abstractions.

⸻

Agent Events

Different agent implementations should eventually produce normalized events that the UI can consume.

Possible event types:

AgentStarted
AgentStopped
TextReceived
Thinking
ToolStarted
ToolFinished
ApprovalRequested
FileChanged
CommandStarted
CommandFinished
Error
TaskCompleted

This allows the UI to treat different agent runtimes consistently.

Again, the concrete event model should emerge from real integrations rather than being over-designed in advance.

⸻

Process Integration

Some agents may provide structured APIs or protocols.

Others may only expose CLI interfaces.

DragonsTUI should eventually be capable of communicating with agents using mechanisms such as:

* subprocess stdin/stdout
* pseudo terminals (PTY)
* structured JSON streams
* JSON-RPC
* HTTP
* WebSocket
* MCP
* custom protocols

The first integrations should prefer the simplest reliable interface exposed by the target agent.

⸻

Long-Term Architecture

The project may eventually separate into layers similar to:

DragonsTUI
|
+-- terminal
|   +-- raw mode
|   +-- input
|   +-- ANSI
|   +-- capabilities
|
+-- renderer
|   +-- Cell
|   +-- Buffer
|   +-- Frame
|   +-- Diff
|
+-- layout
|
+-- widgets
|
+-- animation
|
+-- runtime
|   +-- event loop
|   +-- timers
|   +-- tasks
|
+-- agent
|   +-- manager
|   +-- events
|   +-- adapters
|
+-- app
    +-- multi-agent console

This structure is directional only.

Do not create all modules immediately.

Directories and abstractions should be introduced only when required.

⸻

Initial Scope

The first milestone should be intentionally small.

The goal is not to immediately implement a full TUI framework.

The first milestone should prove that DragonsTUI can:

1. enter terminal raw mode
2. safely restore the terminal on exit
3. detect terminal dimensions
4. receive keyboard input
5. receive resize events
6. render styled text at controlled positions
7. maintain a screen buffer
8. render a frame
9. exit cleanly

Example first demo:

╭─ DragonsTUI ─────────────────────╮
│                                 │
│  Hello from DragonsTUI          │
│                                 │
│  Terminal: 120 x 40             │
│                                 │
│  Press q to quit                │
│                                 │
╰─────────────────────────────────╯

Success means:

* no broken terminal state after exit
* keyboard input works
* resizing works
* content redraws correctly
* code remains small and understandable

⸻

Second Milestone

After the foundation works, add:

* Cell
* Buffer
* Frame
* style representation
* frame diffing
* basic rectangle/layout primitive
* borders
* basic text widget
* basic animation tick

Possible demo:

╭──────────────────────────────────────────╮
│ DragonsTUI                               │
├──────────────────────────────────────────┤
│                                          │
│  Agent One     ● Running                 │
│  Agent Two     ◐ Thinking                │
│  Agent Three   ○ Idle                    │
│                                          │
╰──────────────────────────────────────────╯

⸻

Third Milestone

Build a small real application using DragonsTUI.

Do not continue adding generic framework features without validating them through a real application.

The first real application should be a minimal agent dashboard.

Initial requirements:

* list agents
* select an agent
* start a process
* display process output
* send input
* stop the process
* scroll output
* show agent status

At first, only one external agent integration is required.

Hermes Agent is the preferred first integration.

Once that works, Codex CLI can be added.

Only after multiple integrations exist should a generic agent abstraction be stabilized.

⸻

Non-Goals for Early Versions

Do NOT attempt to build all of the following immediately:

* a complete Ratatui replacement
* a complete Bubble Tea replacement
* web UI
* desktop UI
* plugin marketplace
* remote agent execution
* distributed agents
* complex orchestration
* MCP server/client framework
* full markdown renderer
* full syntax highlighting engine
* advanced graphics engine
* dozens of widgets
* custom scripting language

These may become future goals.

They are intentionally excluded from the initial implementation.

⸻

Future Web and Desktop Clients

DragonsTUI itself is terminal-focused.

However, the agent runtime and protocol should eventually be separable from the terminal presentation layer.

A future architecture may look like:

                  Agent Runtime
                       |
                 Shared Protocol
                       |
          +------------+------------+
          |            |            |
          v            v            v
      DragonsTUI      Web         Desktop
        Rust       TypeScript      Tauri

The TUI renderer should not be distorted merely to share UI code with web clients.

Instead, share domain models, protocols, events, and agent state where appropriate.

⸻

Design Principles

Simplicity First

Implement the minimum mechanism required by the current milestone.

Avoid speculative flexibility.

Avoid large abstractions with only one implementation.

⸻

Build From Real Requirements

The framework should evolve while building the multi-agent console.

When the application genuinely needs a widget or capability, add it to the framework.

Do not build large widget libraries in advance.

⸻

Performance by Architecture

Prefer:

event
  ->
state change
  ->
render
  ->
frame diff
  ->
minimal terminal output

rather than constantly redrawing everything.

⸻

Explicit State

UI behavior should be driven by explicit application state.

Avoid hidden global state.

⸻

Terminal Safety

The terminal must always be restored correctly when the application exits or encounters recoverable errors.

Terminal corruption is considered a critical bug.

⸻

Cross-Platform Direction

The long-term target is:

* Linux
* macOS
* Windows

The initial implementation may prioritize Linux/macOS if doing so significantly simplifies early development, but platform-specific assumptions should be clearly documented.

⸻

Testability

Core rendering logic should be testable without requiring a real terminal.

For example:

* buffer operations
* frame diffing
* layout calculations
* styling
* clipping

should eventually have deterministic unit tests.

Terminal integration tests can be added separately.

⸻

Technical Direction

Primary language:

Rust

Rust was selected because DragonsTUI prioritizes:

* performance
* low-level terminal control
* predictable resource usage
* strong type safety
* concurrency
* native binary distribution
* long-term maintainability
* the ability to build custom rendering infrastructure

External crates are allowed.

However, high-level TUI frameworks should not become the foundation of DragonsTUI.

Low-level crates may be used where reimplementing operating-system or terminal primitives would provide little project value.

For example, dependencies for:

* terminal raw mode
* terminal events
* Unicode width calculations
* PTY support
* async runtime

may be acceptable.

Each dependency should solve a concrete problem.

⸻

Open Questions

These questions should be resolved through implementation rather than prematurely:

1. Immediate-mode vs retained-mode UI?
2. Component tree vs direct frame rendering?
3. Synchronous vs asynchronous core event loop?
4. How should animations be scheduled?
5. How aggressive should frame diffing be?
6. Should layout use a custom flex model?
7. How should async agent events enter the UI loop?
8. Which terminal backend abstractions are actually necessary?
9. Which features belong in DragonsTUI versus the agent application?
10. What is the smallest useful public API?

Do not answer all of these before coding.

Use small experiments and benchmarks where necessary.

⸻

First Development Task

Start with the smallest vertical slice.

Create a Rust application that:

* enters raw mode
* enters alternate screen
* reads terminal size
* renders a bordered DragonsTUI screen
* handles resize
* handles q to quit
* restores terminal state correctly

Do not implement a generic widget framework yet.

Do not implement agent integration yet.

Do not implement animation yet.

Verify terminal restoration and resizing before expanding the architecture.

⸻

Definition of Success

DragonsTUI succeeds if it becomes:

1. a fast and expressive Rust TUI engine,
2. powerful enough to build visually rich terminal applications,
3. responsive under streaming and concurrent workloads,
4. highly customizable without fighting the framework,
5. the foundation of a multi-agent AI terminal control center,
6. useful as an independent TUI project beyond the agent application itself.

The guiding principle is:

Build the smallest powerful core, then let real applications determine what the framework becomes.
