use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use dragons_tui::{
    Animation, Buffer, Canvas, Cell, Constraint, Frame, Gauge, Layout, Line, ProgressBar, Rect,
    RichText, Sparkline, Style, Table, TableColumn, TableState, Text, TextArea, Tree, TreeNode,
    TreeState, Viewport, ViewportState, diff, render_changed_cells,
};

const SIZES: [(&str, u16, u16); 4] = [
    ("80x24", 80, 24),
    ("120x40", 120, 40),
    ("200x60", 200, 60),
    ("300x100", 300, 100),
];
const SAMPLES: usize = 7;

fn main() {
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--list")
    {
        for scenario in scenario_groups() {
            println!("{scenario}");
        }
        return;
    }

    println!("scenario\tsize\titerations\tmedian_ns_per_operation");
    for &(label, width, height) in &SIZES {
        let size = Rect::new(0, 0, width, height);
        measure_foundation(label, size);
        measure_rendering(label, size);
    }
    measure_streaming_and_animation();
}

fn scenario_groups() -> [&'static str; 19] {
    [
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
    ]
}

fn measure_foundation(label: &str, area: Rect) {
    let iterations = iterations_for(area);
    measure("buffer_construction", label, iterations, || {
        black_box(Buffer::new(area.width, area.height));
    });

    let mut buffer = filled_buffer(area);
    measure("buffer_clear", label, iterations, || {
        buffer.clear();
        black_box(&buffer);
        buffer.set(0, 0, Cell::new('x', Style::new()));
    });

    measure("frame_creation", label, iterations, || {
        black_box(Frame::new(area.width, area.height));
    });

    let previous = filled_buffer(area);
    let identical = previous.clone();
    measure("diff_identical", label, iterations, || {
        black_box(diff(Some(&previous), &identical));
    });

    let mut single = previous.clone();
    if area.width > 0 && area.height > 0 {
        single.set(
            area.width / 2,
            area.height / 2,
            Cell::new('x', Style::new().bold()),
        );
    }
    measure("diff_single_cell", label, iterations, || {
        black_box(diff(Some(&previous), &single));
    });

    let sparse = changed_buffer(&previous, 20);
    measure("diff_sparse", label, iterations, || {
        black_box(diff(Some(&previous), &sparse));
    });

    let full = filled_with(area, Cell::new('█', Style::new().bold()));
    measure("diff_full", label, iterations.min(200), || {
        black_box(diff(Some(&previous), &full));
    });

    let single_changes = diff(Some(&previous), &single);
    let sparse_changes = diff(Some(&previous), &sparse);
    let full_changes = diff(Some(&previous), &full);
    measure_terminal_encoding("single", label, iterations, &single_changes);
    measure_terminal_encoding("sparse", label, iterations, &sparse_changes);
    measure_terminal_encoding("full", label, iterations.min(100), &full_changes);

    let layout = Layout::horizontal([
        Constraint::Length(24),
        Constraint::Percentage(35),
        Constraint::Fill(1),
    ])
    .gap(1);
    measure("layout", label, iterations, || {
        black_box(layout.split(area));
    });
}

fn measure_rendering(label: &str, area: Rect) {
    let iterations = iterations_for(area).min(300);
    let style = Style::new().bold();
    let text = Text::new("DragonsTUI renders explicit immediate-mode frames").style(style);
    measure("text_plain", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        text.render(&mut frame, area);
        black_box(frame);
    });

    let rich = RichText::new([
        Line::from("DragonsTUI "),
        Line::from("red orange amber yellow Unicode: İstanbul 你好 🚀"),
        Line::from("rendering through Frame and Buffer"),
    ]);
    measure("rich_text", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        rich.render(&mut frame, area);
        black_box(frame);
    });

    let mut unicode = TextArea::from("é ❤️ 👨‍👩‍👧‍👦 🇹🇷\nİstanbul 你好 🚀\né ❤️ 👨‍👩‍👧‍👦 🇹🇷");
    measure("unicode_grapheme", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        black_box(unicode.render(&mut frame, area, style));
        black_box(frame);
    });

    let table = sample_table();
    let mut table_state = TableState::new();
    measure("table", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        table.render(&mut frame, area, &mut table_state);
        black_box(frame);
    });

    let tree = sample_tree();
    let mut tree_state = TreeState::new();
    tree_state.expand(1);
    tree_state.expand(4);
    measure("tree", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        tree.render(&mut frame, area, &mut tree_state);
        black_box(frame);
    });

    let lines = (0..512)
        .map(|index| format!("[{index:03}] streamed output: İstanbul 你好 🚀"))
        .collect::<Vec<_>>();
    let viewport = Viewport::new(&lines).style(style);
    let mut viewport_state = ViewportState::new();
    viewport_state.end();
    measure("viewport", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        viewport.render(&mut frame, area, &mut viewport_state);
        black_box(frame);
    });

    let mut canvas = Canvas::new(area.width, area.height);
    canvas.draw_rect(0, 0, canvas.logical_width(), canvas.logical_height());
    canvas.draw_line(
        0,
        0,
        canvas.logical_width().saturating_sub(1) as i32,
        canvas.logical_height().saturating_sub(1) as i32,
    );
    measure("canvas", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        canvas.render(&mut frame, area, style);
        black_box(frame);
    });

    let sparkline =
        Sparkline::new((0..512).map(|index| f64::from((index * 13 % 97) as u16))).style(style);
    measure("sparkline", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        sparkline.render(&mut frame, area);
        black_box(frame);
    });

    let bar = ProgressBar::new(0.72)
        .filled_style(style)
        .unfilled_style(Style::new());
    let gauge = Gauge::new(0.61)
        .filled_style(style)
        .unfilled_style(Style::new());
    measure("visualization_bars", label, iterations, || {
        let mut frame = Frame::new(area.width, area.height);
        bar.render(&mut frame, area);
        gauge.render(&mut frame, area);
        black_box(frame);
    });
}

fn measure_streaming_and_animation() {
    let area = Rect::new(0, 0, 120, 40);
    measure("streaming", "120x40", 50, || {
        let mut lines = (0..256)
            .map(|index| format!("seed {index:03}"))
            .collect::<Vec<_>>();
        let mut state = ViewportState::new();
        for update in 0..64 {
            lines.push(format!("update {update:03}: İstanbul 你好 🚀"));
            let viewport = Viewport::new(&lines);
            let mut frame = Frame::new(area.width, area.height);
            viewport.render(&mut frame, area, &mut state);
            black_box(frame);
        }
    });

    for (label, frame_duration) in [
        ("animation_10fps", Duration::from_millis(100)),
        ("animation_20fps", Duration::from_millis(50)),
    ] {
        measure(label, "120x40", 500, || {
            let start = Instant::now();
            let mut animation = Animation::new(["⠋", "⠙", "⠹", "⠸"]).frame_duration(frame_duration);
            for tick in 0..20 {
                black_box(animation.update(start + frame_duration * tick));
            }
        });
    }
}

fn measure_terminal_encoding(
    change_kind: &str,
    label: &str,
    iterations: usize,
    changes: &[dragons_tui::ChangedCell],
) {
    let scenario = format!("terminal_encode_{change_kind}");
    let mut output = Vec::with_capacity(changes.len().saturating_mul(20));
    measure(&scenario, label, iterations, || {
        output.clear();
        render_changed_cells(&mut output, changes, false)
            .expect("in-memory terminal encoding should succeed");
        black_box(&output);
    });
}

fn measure(name: &str, size: &str, iterations: usize, mut operation: impl FnMut()) {
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        samples.push(started.elapsed().as_nanos() / iterations as u128);
    }
    samples.sort_unstable();
    println!("{name}\t{size}\t{iterations}\t{}", samples[SAMPLES / 2]);
}

fn iterations_for(area: Rect) -> usize {
    match usize::from(area.width) * usize::from(area.height) {
        0..=2_000 => 2_000,
        2_001..=8_000 => 1_000,
        _ => 300,
    }
}

fn filled_buffer(area: Rect) -> Buffer {
    filled_with(area, Cell::new(' ', Style::new()))
}

fn filled_with(area: Rect, cell: Cell) -> Buffer {
    let mut buffer = Buffer::new(area.width, area.height);
    for y in 0..area.height {
        for x in 0..area.width {
            buffer.set(x, y, cell);
        }
    }
    buffer
}

fn changed_buffer(previous: &Buffer, stride: usize) -> Buffer {
    let mut current = previous.clone();
    for y in 0..current.height() {
        for x in 0..current.width() {
            let index = usize::from(y) * usize::from(current.width()) + usize::from(x);
            if index % stride == 0 {
                current.set(x, y, Cell::new('•', Style::new().bold()));
            }
        }
    }
    current
}

fn sample_table() -> Table {
    Table::new([
        TableColumn::new(Constraint::Length(12)),
        TableColumn::new(Constraint::Length(10)),
        TableColumn::new(Constraint::Fill(1)),
    ])
    .header([
        Line::from("NAME"),
        Line::from("STATUS"),
        Line::from("DETAIL"),
    ])
    .rows((0..128).map(|index| {
        vec![
            Line::from(format!("Agent-{index:03}")),
            Line::from(if index % 3 == 0 { "Working" } else { "Ready" }),
            Line::from("İstanbul 你好 🚀 é ❤️"),
        ]
    }))
    .selected_style(Style::new().bold())
}

fn sample_tree() -> Tree {
    Tree::new([TreeNode::new(1, "DragonsTUI").children([
        TreeNode::new(2, "src").children([
            TreeNode::new(3, "runtime.rs"),
            TreeNode::new(4, "widgets")
                .children((5..133).map(|id| TreeNode::new(id, format!("widget-{id:03}.rs")))),
        ]),
        TreeNode::new(200, "examples"),
    ])])
}
