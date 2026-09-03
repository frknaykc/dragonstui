use crate::Rect;

/// Primary axis used by [`Layout`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Horizontal,
    Vertical,
}

/// A fixed, proportional, or weighted layout allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Constraint {
    Length(u16),
    /// Percentage values above 100 are treated as 100.
    Percentage(u16),
    /// Zero-weight fills receive no space and do not participate in weight sharing.
    Fill(u16),
}

/// Explicit layout resolver for deriving child rectangles.
///
/// Resolves lengths first, then percentages of the gap-adjusted parent axis,
/// then distributes the remainder among positive fills. Any unclaimed space is left trailing.
pub struct Layout {
    direction: Direction,
    constraints: Vec<Constraint>,
    gap: u16,
}

impl Layout {
    pub fn new(direction: Direction) -> Self {
        Self {
            direction,
            constraints: Vec::new(),
            gap: 0,
        }
    }

    pub fn horizontal(constraints: impl Into<Vec<Constraint>>) -> Self {
        Self::new(Direction::Horizontal).constraints(constraints)
    }

    pub fn vertical(constraints: impl Into<Vec<Constraint>>) -> Self {
        Self::new(Direction::Vertical).constraints(constraints)
    }

    pub fn constraints(mut self, constraints: impl Into<Vec<Constraint>>) -> Self {
        self.constraints = constraints.into();
        self
    }

    pub fn gap(mut self, gap: u16) -> Self {
        self.gap = gap;
        self
    }

    pub fn split(&self, parent: Rect) -> Vec<Rect> {
        let count = self.constraints.len();
        if count == 0 {
            return Vec::new();
        }

        let primary = match self.direction {
            Direction::Horizontal => parent.width,
            Direction::Vertical => parent.height,
        };
        let separators = count.saturating_sub(1);
        let requested_gap = u32::from(self.gap).saturating_mul(separators as u32);
        let total_gap = u16::try_from(requested_gap)
            .unwrap_or(u16::MAX)
            .min(primary);
        let distributable = primary - total_gap;
        let mut sizes = vec![0; count];
        let mut remaining = distributable;

        for (index, constraint) in self.constraints.iter().enumerate() {
            if let Constraint::Length(length) = constraint {
                let size = (*length).min(remaining);
                sizes[index] = size;
                remaining -= size;
            }
        }

        for (index, constraint) in self.constraints.iter().enumerate() {
            if let Constraint::Percentage(percentage) = constraint {
                let requested = u16::try_from(
                    u32::from(distributable) * u32::from((*percentage).min(100)) / 100,
                )
                .unwrap_or(distributable);
                let size = requested.min(remaining);
                sizes[index] = size;
                remaining -= size;
            }
        }

        let total_weight: u64 = self
            .constraints
            .iter()
            .filter_map(|constraint| match constraint {
                Constraint::Fill(weight) if *weight > 0 => Some(u64::from(*weight)),
                _ => None,
            })
            .sum();
        if total_weight > 0 && remaining > 0 {
            let mut assigned = 0;
            for (index, constraint) in self.constraints.iter().enumerate() {
                if let Constraint::Fill(weight) = constraint {
                    let size =
                        u16::try_from(u64::from(remaining) * u64::from(*weight) / total_weight)
                            .unwrap_or(remaining);
                    sizes[index] = size;
                    assigned += size;
                }
            }

            let mut remainder = remaining - assigned;
            for (index, constraint) in self.constraints.iter().enumerate() {
                if remainder == 0 {
                    break;
                }
                if matches!(constraint, Constraint::Fill(weight) if *weight > 0) {
                    sizes[index] += 1;
                    remainder -= 1;
                }
            }
        }

        let mut areas = Vec::with_capacity(count);
        let mut position = match self.direction {
            Direction::Horizontal => parent.x,
            Direction::Vertical => parent.y,
        };
        let mut gap_remaining = total_gap;

        for (index, size) in sizes.into_iter().enumerate() {
            let area = match self.direction {
                Direction::Horizontal => Rect::new(position, parent.y, size, parent.height),
                Direction::Vertical => Rect::new(parent.x, position, parent.width, size),
            };
            areas.push(area);

            if index + 1 < count {
                position = position.saturating_add(size);
                let gap = self.gap.min(gap_remaining);
                position = position.saturating_add(gap);
                gap_remaining -= gap;
            }
        }

        areas
    }
}
