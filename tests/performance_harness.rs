use std::process::Command;

#[test]
fn measurement_binary_lists_the_required_reproducible_scenarios() {
    let output = Command::new(env!("CARGO_BIN_EXE_dragonstui_measure"))
        .arg("--list")
        .output()
        .expect("measurement binary should run");

    assert!(output.status.success());
    let scenarios = String::from_utf8(output.stdout).expect("scenario list should be UTF-8");
    for required in [
        "buffer_construction",
        "buffer_clear",
        "frame_creation",
        "diff_identical",
        "diff_single_cell",
        "diff_sparse",
        "diff_full",
        "terminal_encode",
        "layout",
        "text_plain",
        "rich_text",
        "unicode_grapheme",
        "table",
        "tree",
        "viewport",
        "canvas",
        "sparkline",
        "streaming",
        "animation",
    ] {
        assert!(scenarios.lines().any(|line| line == required), "{required}");
    }
}
