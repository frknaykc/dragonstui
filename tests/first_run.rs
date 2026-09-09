use std::process::Command;

fn check_terminal_boundary(binary: &str) {
    let output = Command::new(binary).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("requires an interactive terminal"));
    assert!(error.contains("--help"));
    assert!(!error.contains('\u{1b}'));
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(!help.stdout.contains(&0x1b));
}

#[test]
fn dashboard_first_run_without_tty_is_actionable() {
    check_terminal_boundary(env!("CARGO_BIN_EXE_dragons_tui"));
    let help = Command::new(env!("CARGO_BIN_EXE_dragons_tui"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("dragonstui-showcase --help"));
    assert!(text.contains("No config file"));
}

#[cfg(feature = "adapter-showcase")]
#[test]
fn showcase_help_explains_first_run_and_shared_root() {
    let binary = env!("CARGO_BIN_EXE_dragonstui-showcase");
    check_terminal_boundary(binary);
    let help = Command::new(binary).arg("--help").output().unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    for expected in [
        "press 8",
        "--adapter-root",
        "--root",
        "No config file",
        "in memory",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
}
