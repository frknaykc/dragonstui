use std::process::Command;

#[test]
fn showcase_binary_has_a_dependency_free_help_surface() {
    let output = Command::new(env!("CARGO_BIN_EXE_dragonstui-showcase"))
        .arg("--help")
        .output()
        .expect("showcase binary should run");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Dragonfire showcase"));
}
