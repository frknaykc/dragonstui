use crate::Color;

/// Dragonfire's semantic RGB palette value.
///
/// Themes are plain values: primitives receive styles from the application rather than storing a
/// global theme or a theme-switching runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    pub background: Color,
    pub primary: Color,
    pub secondary: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub surface: Color,
    pub text: Color,
    pub muted: Color,
}

impl Theme {
    pub const fn new() -> Self {
        Self {
            background: Color::rgb(10, 8, 6),
            primary: Color::rgb(120, 20, 10),
            secondary: Color::rgb(220, 70, 10),
            success: Color::rgb(255, 200, 55),
            warning: Color::rgb(245, 130, 20),
            error: Color::rgb(180, 35, 10),
            surface: Color::rgb(20, 14, 10),
            text: Color::rgb(240, 225, 205),
            muted: Color::rgb(145, 105, 75),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}
