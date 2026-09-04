use crate::Rect;

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
        let desired_master_width =
            u16::try_from(u32::from(area.width) * u32::from(self.master_percent.min(100)) / 100)
                .unwrap_or(area.width);
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
