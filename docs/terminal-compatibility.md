# Terminal Compatibility

## Contract

DragonsTUI is an ANSI/Crossterm terminal UI library. Its compatibility boundary is deliberately small:

- an interactive terminal that supports cursor positioning, clear screen, cursor visibility, and an alternate screen;
- Unicode-capable text input/output using a font with the required glyphs;
- 24-bit SGR color when Dragonfire's exact RGB palette is desired; and
- SGR mouse reporting when mouse interaction is desired.

Keyboard operation remains available without mouse reporting. The framework uses `crossterm` 0.29 for raw mode, terminal size, event decoding, and terminal lifecycle. DragonsTUI does not maintain a terminal database, fingerprint emulators, replace terminfo, or negotiate a component/runtime capability tree.

## Status Terms

- **Known by protocol** means DragonsTUI emits or consumes the named ANSI/Crossterm protocol; it does not mean a specific emulator was visually tested.
- **Actually tested** means it was exercised in this repository's macOS environment.
- **Unverified** means no claim is made about real behavior on that environment.

## Environment Matrix

| Environment | Expected / known by protocol | Actually tested | Known limitations |
| --- | --- | --- | --- |
| Hermes-managed macOS POSIX PTY | Alternate screen, cursor lifecycle, raw-mode event input, SGR mouse input, resize signals | Yes: lifecycle helper exercised splash/main input, SGR click/wheel, Ctrl+P, the 120×40 → 80×24 → 40×15 → 20×8 → 5×3 → 1×1 → 80×24 sequence, `q`, and Ctrl+C | This PTY check validates lifecycle/protocol handling, not an emulator's visual font/color rendering. Its launch environment reported `TERM=dumb`, so it is not evidence for a named emulator or true-color rendering. |
| xterm-compatible terminals | Standard CSI cursor/clear/alternate-screen sequences, SGR styles, SGR mouse, UTF-8 are the protocol assumptions | Unverified in a named xterm-compatible emulator | User terminal configuration, multiplexer behavior, font, and color mode determine actual display. |
| tmux | Expected to relay the same terminal protocol when its client terminal supports it | Unverified: `tmux` was not installed in the measurement environment | Color and mouse behavior can be limited by tmux/client configuration. |
| Kitty | Expected to meet the standard ANSI/UTF-8 assumptions | Unverified: Kitty was not installed | No Kitty-specific protocol is required or implemented. |
| WezTerm | Expected to meet the standard ANSI/UTF-8 assumptions | Unverified: WezTerm was not installed | No WezTerm-specific protocol is required or implemented. |
| Ghostty | Expected to meet the standard ANSI/UTF-8 assumptions | Unverified: Ghostty was not installed | No Ghostty-specific protocol is required or implemented. |
| Alacritty | Expected to meet the standard ANSI/UTF-8 assumptions | Unverified: Alacritty was not installed | No Alacritty-specific protocol is required or implemented. |
| iTerm2 | Expected to meet the standard ANSI/UTF-8 assumptions | Unverified: iTerm2 was not installed | No iTerm2-specific protocol is required or implemented. |
| Windows Terminal | Crossterm is the portability layer, but DragonsTUI has no Windows-specific branch | Unverified: no Windows environment was available | No Windows Terminal visual, lifecycle, color, or input claim is made from this macOS run. |

## Capabilities

No public `TerminalCapabilities` API is added in this release. There is no existing library consumer that needs runtime capability branching:

- `Style::Color` currently carries only exact RGB values and `terminal.rs` encodes them as Crossterm RGB SGR colors.
- Mouse capture is enabled by the application terminal guard; applications can remain keyboard-only by simply not routing mouse events to UI policy.
- Hyperlinks are not represented in `Style`, `Text`, or `RichText`.
- Unicode/Braille rendering is an application/rendering contract, not a runtime terminal query.

Adding a public capability object now would create state without a concrete consumer and would not change output behavior. This preserves the M19 explicit immediate-mode architecture.

## Color

Dragonfire uses 24-bit RGB SGR output. DragonsTUI does not implement color quantization or a palette fallback. On a terminal that does not render true color, the terminal/multiplexer may degrade colors; the application remains functional, but the exact Dragonfire palette is not guaranteed. This is intentional: no measured or current application requirement justifies a color-conversion subsystem.

## Mouse and Input

The dashboard terminal guard enters the alternate screen, enables mouse capture, and restores cursor, screen, mouse capture, raw mode, canonical mode, and echo on exit paths. `normalize_crossterm_event` maps supported key presses, resize events, left/right/middle button events, drag, move, and vertical wheel events into DragonsTUI's terminal-independent `Event` values. Horizontal wheel events are currently ignored.

Mouse support is optional at the application-policy level. Applications must provide keyboard routes for their essential interactions; DragonsTUI does not provide automatic event bubbling or focus dispatch.

## Unicode and Braille

DragonsTUI uses `unicode-width` for terminal column calculations and `unicode-segmentation` for grapheme-safe `TextInput` and `TextArea` editing. It preserves the Buffer wide-cell/continuation invariant for supported wide characters.

The library cannot control an emulator's font, emoji presentation, ambiguous-width policy, or the rendered width of every emoji sequence. CJK, emoji, ZWJ families, variation selectors, and flags therefore require an emulator/font combination whose display width agrees sufficiently with the terminal's input/output behavior. Braille canvas output uses Unicode Braille code points; a font without Braille glyph support cannot be repaired by the framework.

## Hyperlinks

OSC 8 hyperlinks are deferred. No current primitive requires them, and no capability API or hyperlink-style surface has a real consumer. They are not emitted.

## Validation and Reproduction

The repository includes a dependency-free POSIX PTY acceptance helper:

```sh
cargo build
python3 tools/pty_smoke.py --exit q -- target/debug/dragons_tui
python3 tools/pty_smoke.py --exit ctrl-c -- target/debug/dragons_tui
```

It validates application exit, alternate-screen and cursor lifecycle sequences, mouse enable/disable sequences, canonical/echo restoration, resize delivery, keyboard input, SGR mouse input, and command-palette entry. It deliberately does not claim visual validation in any named terminal emulator.

## Deferred Work

Revisit terminal capabilities only when a concrete application needs a behavior branch that cannot be expressed by the current explicit application policy—for example, a product requirement for an actual non-true-color palette fallback or an OSC 8 link primitive. Any such work should be tested in the target terminal, not inferred from a terminal-name database.
