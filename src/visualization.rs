use crate::{Cell, Frame, Rect, Style};

/// A horizontal ratio bar. Finite ratios are clamped to `[0.0, 1.0]`; NaN and
/// negative infinity become empty, while positive infinity becomes full.
/// Filled cells use `floor(width * ratio)`. A label, when supplied, renders on
/// the next target row so it does not alter the bar's width calculation.
pub struct ProgressBar {
    ratio: f64,
    filled_style: Style,
    unfilled_style: Style,
    label: Option<String>,
}

/// A ratio/value visualization with the same compact rendering contract as a
/// [`ProgressBar`], retained as a separate semantic primitive.
pub struct Gauge {
    ratio: f64,
    filled_style: Style,
    unfilled_style: Style,
    label: Option<String>,
}

/// A one-row sparkline that renders the latest samples first when the target
/// is narrower than the input. Non-finite samples normalize to zero; equal
/// values render as the stable middle glyph `▅`.
pub struct Sparkline {
    samples: Vec<f64>,
    style: Style,
}

impl ProgressBar {
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio,
            filled_style: Style::new(),
            unfilled_style: Style::new(),
            label: None,
        }
    }

    pub fn filled_style(mut self, style: Style) -> Self {
        self.filled_style = style;
        self
    }

    pub fn unfilled_style(mut self, style: Style) -> Self {
        self.unfilled_style = style;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn render(&self, frame: &mut Frame, target: Rect) {
        render_bar(
            frame,
            target,
            self.ratio,
            self.filled_style,
            self.unfilled_style,
            self.label.as_deref(),
        );
    }
}

impl Gauge {
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio,
            filled_style: Style::new(),
            unfilled_style: Style::new(),
            label: None,
        }
    }

    pub fn filled_style(mut self, style: Style) -> Self {
        self.filled_style = style;
        self
    }

    pub fn unfilled_style(mut self, style: Style) -> Self {
        self.unfilled_style = style;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn render(&self, frame: &mut Frame, target: Rect) {
        render_bar(
            frame,
            target,
            self.ratio,
            self.filled_style,
            self.unfilled_style,
            self.label.as_deref(),
        );
    }
}

impl Sparkline {
    pub fn new(samples: impl IntoIterator<Item = f64>) -> Self {
        Self {
            samples: samples.into_iter().collect(),
            style: Style::new(),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn render(&self, frame: &mut Frame, target: Rect) {
        if target.width == 0 || target.height == 0 {
            return;
        }

        let visible = self
            .samples
            .iter()
            .rev()
            .take(usize::from(target.width))
            .map(|sample| if sample.is_finite() { *sample } else { 0.0 })
            .collect::<Vec<_>>();
        if visible.is_empty() {
            return;
        }

        let minimum = visible.iter().copied().fold(f64::INFINITY, f64::min);
        let maximum = visible.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        for (index, sample) in visible.into_iter().rev().enumerate() {
            let level = sparkline_level(sample, minimum, maximum);
            let Some(frame_x) = target.x.checked_add(index as u16) else {
                continue;
            };
            frame.set_cell(
                frame_x,
                target.y,
                Cell::new(SPARKLINE_LEVELS[level], self.style),
            );
        }
    }
}

const SPARKLINE_LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn sparkline_level(sample: f64, minimum: f64, maximum: f64) -> usize {
    if minimum == maximum {
        return 4;
    }

    let scale = minimum.abs().max(maximum.abs());
    let minimum = minimum / scale;
    let maximum = maximum / scale;
    let sample = sample / scale;
    let normalized = (sample - minimum) / (maximum - minimum);
    (normalized * 7.0).round().clamp(0.0, 7.0) as usize
}

fn render_bar(
    frame: &mut Frame,
    target: Rect,
    ratio: f64,
    filled_style: Style,
    unfilled_style: Style,
    label: Option<&str>,
) {
    if target.width == 0 || target.height == 0 {
        return;
    }

    let filled = filled_cells(target.width, ratio);
    for x in 0..target.width {
        let Some(frame_x) = target.x.checked_add(x) else {
            continue;
        };
        let character = if x < filled { '█' } else { '░' };
        let style = if x < filled {
            filled_style
        } else {
            unfilled_style
        };
        frame.set_cell(frame_x, target.y, Cell::new(character, style));
    }
    if let (true, Some(label)) = (target.height > 1, label) {
        frame.write_text_in(target, 0, 1, label, filled_style);
    }
}

fn filled_cells(width: u16, ratio: f64) -> u16 {
    let ratio = if ratio.is_nan() {
        0.0
    } else if ratio.is_sign_positive() && ratio.is_infinite() {
        1.0
    } else if ratio.is_sign_negative() && ratio.is_infinite() {
        0.0
    } else {
        ratio.clamp(0.0, 1.0)
    };
    (f64::from(width) * ratio).floor() as u16
}
