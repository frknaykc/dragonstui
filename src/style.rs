/// An exact 24-bit terminal color.
///
/// DragonsTUI emits RGB SGR color; fallback behavior is terminal-side.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color {
    Rgb { r: u8, g: u8, b: u8 },
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb { r, g, b }
    }
}

/// Boolean text attributes carried by a [`Style`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Attributes {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reverse: bool,
}

/// Immutable-by-value terminal style builder.
///
/// [`Style::patch`] overlays colors and adds enabled attributes; `false` does not remove an
/// attribute already enabled by a base style.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub attributes: Attributes,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.attributes.bold = true;
        self
    }

    pub fn dim(mut self) -> Self {
        self.attributes.dim = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.attributes.italic = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.attributes.underline = true;
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.attributes.strikethrough = true;
        self
    }

    pub fn reverse(mut self) -> Self {
        self.attributes.reverse = true;
        self
    }

    /// Overlays specified colours and unions enabled attributes.
    ///
    /// This compact model treats `false` attributes as unspecified: patches
    /// can add attributes but do not disable attributes already present.
    pub fn patch(self, overlay: Self) -> Self {
        Self {
            fg: overlay.fg.or(self.fg),
            bg: overlay.bg.or(self.bg),
            attributes: Attributes {
                bold: self.attributes.bold || overlay.attributes.bold,
                dim: self.attributes.dim || overlay.attributes.dim,
                italic: self.attributes.italic || overlay.attributes.italic,
                underline: self.attributes.underline || overlay.attributes.underline,
                strikethrough: self.attributes.strikethrough || overlay.attributes.strikethrough,
                reverse: self.attributes.reverse || overlay.attributes.reverse,
            },
        }
    }
}
