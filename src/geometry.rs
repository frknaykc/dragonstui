/// A zero-based terminal cell coordinate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

/// Terminal width and height in cells.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

impl Size {
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}

/// A zero-based, width/height rectangle in terminal cells.
///
/// Geometry helpers use saturating arithmetic so tiny and extreme terminal sizes remain safe.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn position(self) -> Position {
        Position {
            x: self.x,
            y: self.y,
        }
    }

    pub fn size(self) -> Size {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    pub fn right(self) -> u16 {
        self.x.saturating_add(self.width)
    }

    pub fn bottom(self) -> u16 {
        self.y.saturating_add(self.height)
    }

    pub fn contains(self, position: Position) -> bool {
        position.x >= self.x
            && position.x < self.right()
            && position.y >= self.y
            && position.y < self.bottom()
    }

    pub fn inner(self) -> Self {
        Self {
            x: self.x.saturating_add(u16::from(self.width > 0)),
            y: self.y.saturating_add(u16::from(self.height > 0)),
            width: self.width.saturating_sub(2),
            height: self.height.saturating_sub(2),
        }
    }
}

pub fn split_horizontal(rect: Rect, left_width: u16) -> (Rect, Rect) {
    let left_width = left_width.min(rect.width);
    let right_width = rect.width - left_width;

    (
        Rect::new(rect.x, rect.y, left_width, rect.height),
        Rect::new(
            rect.x.saturating_add(left_width),
            rect.y,
            right_width,
            rect.height,
        ),
    )
}

pub fn split_vertical(rect: Rect, top_height: u16) -> (Rect, Rect) {
    let top_height = top_height.min(rect.height);
    let bottom_height = rect.height - top_height;

    (
        Rect::new(rect.x, rect.y, rect.width, top_height),
        Rect::new(
            rect.x,
            rect.y.saturating_add(top_height),
            rect.width,
            bottom_height,
        ),
    )
}
