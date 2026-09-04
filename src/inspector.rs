use crate::{Position, Rect};

/// Derived master/detail rectangles for a reusable inspector surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InspectorAreas {
    pub master: Rect,
    pub divider: Option<Rect>,
    pub detail: Rect,
    horizontal: bool,
}

impl InspectorAreas {
    pub fn is_horizontal(self) -> bool {
        self.horizontal
    }
}

/// Small responsive master/detail layout with a one-cell divider in horizontal mode.
///
/// If the requested minimum widths cannot fit, the layout uses a vertical stack rather than
/// collapsing or overlapping either pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InspectorLayout {
    master_percent: u16,
    minimum_master_width: u16,
    minimum_detail_width: u16,
}

impl InspectorLayout {
    pub const fn new(
        master_percent: u16,
        minimum_master_width: u16,
        minimum_detail_width: u16,
    ) -> Self {
        Self {
            master_percent,
            minimum_master_width,
            minimum_detail_width,
        }
    }

    pub fn split(self, area: Rect) -> InspectorAreas {
        self.split_at(area, None)
    }

    fn split_at(self, area: Rect, requested_master_width: Option<u16>) -> InspectorAreas {
        let divider_width = 1;
        let required_width = self
            .minimum_master_width
            .saturating_add(self.minimum_detail_width)
            .saturating_add(divider_width);
        if area.width < required_width {
            let master_height = area.height.saturating_add(1) / 2;
            return InspectorAreas {
                master: Rect::new(area.x, area.y, area.width, master_height),
                divider: None,
                detail: Rect::new(
                    area.x,
                    area.y.saturating_add(master_height),
                    area.width,
                    area.height.saturating_sub(master_height),
                ),
                horizontal: false,
            };
        }

        let maximum_master_width = area
            .width
            .saturating_sub(divider_width)
            .saturating_sub(self.minimum_detail_width);
        let desired_master_width = requested_master_width.unwrap_or_else(|| {
            u16::try_from(u32::from(area.width) * u32::from(self.master_percent.min(100)) / 100)
                .unwrap_or(area.width)
        });
        let master_width = desired_master_width
            .max(self.minimum_master_width)
            .min(maximum_master_width);
        let detail_x = area
            .x
            .saturating_add(master_width)
            .saturating_add(divider_width);
        InspectorAreas {
            master: Rect::new(area.x, area.y, master_width, area.height),
            divider: Some(Rect::new(
                area.x.saturating_add(master_width),
                area.y,
                divider_width,
                area.height,
            )),
            detail: Rect::new(
                detail_x,
                area.y,
                area.width
                    .saturating_sub(master_width)
                    .saturating_sub(divider_width),
                area.height,
            ),
            horizontal: true,
        }
    }
}

/// Persistent divider state for a master/detail inspector layout.
///
/// The state stores a terminal-cell master width. Resolution remains delegated to
/// [`InspectorLayout`] so every drag and terminal resize shares the same minimum constraints.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InspectorSplitState {
    requested_master_width: Option<u16>,
    dragging: bool,
}

impl InspectorSplitState {
    pub const fn new() -> Self {
        Self {
            requested_master_width: None,
            dragging: false,
        }
    }

    pub fn split(&self, layout: InspectorLayout, area: Rect) -> InspectorAreas {
        layout.split_at(area, self.requested_master_width)
    }

    pub const fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Begins a drag only when the pointer is on the current divider.
    pub fn start_drag(&mut self, layout: InspectorLayout, area: Rect, point: Position) -> bool {
        if self
            .split(layout, area)
            .divider
            .is_some_and(|divider| divider.contains(point))
        {
            self.dragging = true;
            true
        } else {
            false
        }
    }

    /// Updates the requested master width from a zero-based terminal-cell coordinate.
    pub fn drag_to(&mut self, layout: InspectorLayout, area: Rect, point: Position) -> bool {
        if !self.dragging || !self.split(layout, area).is_horizontal() {
            return false;
        }
        let resolved = layout.split_at(area, Some(point.x.saturating_sub(area.x)));
        let changed = self.requested_master_width != Some(resolved.master.width);
        self.requested_master_width = Some(resolved.master.width);
        changed
    }

    /// Ends an active divider drag. Further pointer movement has no effect until another hit.
    pub fn stop_drag(&mut self) -> bool {
        let was_dragging = self.dragging;
        self.dragging = false;
        was_dragging
    }
}
