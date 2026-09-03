use crate::{Cell, Frame, Rect, Style};

const BRAILLE_BASE: u32 = 0x2800;
const LEFT: u8 = 1;
const RIGHT: u8 = 2;
const TOP: u8 = 4;
const BOTTOM: u8 = 8;

/// An owned 2×4-dot-per-cell Unicode Braille drawing surface.
///
/// Draw in logical dot coordinates, then render the canvas explicitly into a frame rectangle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Canvas {
    width: u16,
    height: u16,
    dots: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            dots: vec![0; usize::from(width) * usize::from(height)],
        }
    }

    pub const fn width(&self) -> u16 {
        self.width
    }

    pub const fn height(&self) -> u16 {
        self.height
    }

    pub const fn logical_width(&self) -> u32 {
        self.width as u32 * 2
    }

    pub const fn logical_height(&self) -> u32 {
        self.height as u32 * 4
    }

    pub fn set_point(&mut self, x: u32, y: u32) -> bool {
        if x >= self.logical_width() || y >= self.logical_height() {
            return false;
        }

        let cell_x = u16::try_from(x / 2).expect("logical x is bounded by canvas width");
        let cell_y = u16::try_from(y / 4).expect("logical y is bounded by canvas height");
        let bit = braille_bit((x % 2) as u8, (y % 4) as u8);
        let index = self.index(cell_x, cell_y);
        let Some(mask) = self.dots.get_mut(index) else {
            return false;
        };
        *mask |= bit;
        true
    }

    pub fn clear(&mut self) {
        self.dots.fill(0);
    }

    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.draw_line_i64(i64::from(x0), i64::from(y0), i64::from(x1), i64::from(y1));
    }

    fn draw_line_i64(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) {
        let Some((mut x0, mut y0, x1, y1)) = self.clip_line(x0, y0, x1, y1) else {
            return;
        };

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            self.set_point(x0 as u32, y0 as u32);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let twice_error = error * 2;
            if twice_error >= dy {
                error += dy;
                x0 += sx;
            }
            if twice_error <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    pub fn draw_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        let x0 = i64::from(x);
        let y0 = i64::from(y);
        let x1 = x0 + i64::from(width) - 1;
        let y1 = y0 + i64::from(height) - 1;
        self.draw_line_i64(x0, y0, x1, y0);
        self.draw_line_i64(x0, y1, x1, y1);
        self.draw_line_i64(x0, y0, x0, y1);
        self.draw_line_i64(x1, y0, x1, y1);
    }

    pub fn render(&self, frame: &mut Frame, target: Rect, style: Style) {
        let visible_width = self.width.min(target.width);
        let visible_height = self.height.min(target.height);
        for y in 0..visible_height {
            let Some(frame_y) = target.y.checked_add(y) else {
                continue;
            };
            for x in 0..visible_width {
                let Some(frame_x) = target.x.checked_add(x) else {
                    continue;
                };
                let character = braille_character(self.dots[self.index(x, y)]);
                frame.set_cell(frame_x, frame_y, Cell::new(character, style));
            }
        }
    }

    fn clip_line(
        &self,
        mut x0: i64,
        mut y0: i64,
        mut x1: i64,
        mut y1: i64,
    ) -> Option<(i64, i64, i64, i64)> {
        let max_x = i64::from(self.logical_width()).checked_sub(1)?;
        let max_y = i64::from(self.logical_height()).checked_sub(1)?;

        for _ in 0..8 {
            let code0 = out_code(x0, y0, max_x, max_y);
            let code1 = out_code(x1, y1, max_x, max_y);
            if code0 | code1 == 0 {
                return Some((x0, y0, x1, y1));
            }
            if code0 & code1 != 0 {
                return None;
            }

            let code = if code0 != 0 { code0 } else { code1 };
            let (x, y) = if code & TOP != 0 {
                let denominator = y1 - y0;
                (x0 + (x1 - x0) * -y0 / denominator, 0)
            } else if code & BOTTOM != 0 {
                let denominator = y1 - y0;
                (x0 + (x1 - x0) * (max_y - y0) / denominator, max_y)
            } else if code & RIGHT != 0 {
                let denominator = x1 - x0;
                (max_x, y0 + (y1 - y0) * (max_x - x0) / denominator)
            } else {
                let denominator = x1 - x0;
                (0, y0 + (y1 - y0) * -x0 / denominator)
            };

            if code == code0 {
                x0 = x;
                y0 = y;
            } else {
                x1 = x;
                y1 = y;
            }
        }

        None
    }

    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.width) + usize::from(x)
    }
}

fn braille_bit(x: u8, y: u8) -> u8 {
    match (x, y) {
        (0, 0) => 0b0000_0001,
        (0, 1) => 0b0000_0010,
        (0, 2) => 0b0000_0100,
        (1, 0) => 0b0000_1000,
        (1, 1) => 0b0001_0000,
        (1, 2) => 0b0010_0000,
        (0, 3) => 0b0100_0000,
        (1, 3) => 0b1000_0000,
        _ => 0,
    }
}

fn braille_character(mask: u8) -> char {
    if mask == 0 {
        ' '
    } else {
        char::from_u32(BRAILLE_BASE + u32::from(mask)).expect("Braille bitmasks are valid Unicode")
    }
}

fn out_code(x: i64, y: i64, max_x: i64, max_y: i64) -> u8 {
    let mut code = 0;
    if x < 0 {
        code |= LEFT;
    } else if x > max_x {
        code |= RIGHT;
    }
    if y < 0 {
        code |= TOP;
    } else if y > max_y {
        code |= BOTTOM;
    }
    code
}
