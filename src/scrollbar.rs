use crate::{Cell, Frame, Position, Rect, Style, ViewportState};

/// The track and thumb rectangles derived from a [`ViewportState`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScrollbarGeometry {
    pub track: Rect,
    pub thumb: Rect,
}

/// Stateless rendering and geometry helpers for a vertical viewport scrollbar.
pub struct Scrollbar;

impl Scrollbar {
    /// Returns no geometry when the viewport cannot scroll or the track has no height.
    pub fn geometry(viewport: &ViewportState, track: Rect) -> Option<ScrollbarGeometry> {
        if track.width == 0 || track.height == 0 || viewport.max_scroll() == 0 {
            return None;
        }
        let track_height = usize::from(track.height);
        let content_height = viewport.content_height().max(1);
        let viewport_height = viewport.viewport_height().min(content_height);
        let thumb_height = track_height
            .saturating_mul(viewport_height)
            .saturating_add(content_height.saturating_sub(1))
            / content_height;
        let thumb_height = thumb_height.clamp(1, track_height);
        let travel = track_height.saturating_sub(thumb_height);
        let thumb_offset = if travel == 0 {
            0
        } else {
            travel.saturating_mul(viewport.offset()) / viewport.max_scroll()
        };
        Some(ScrollbarGeometry {
            track,
            thumb: Rect::new(
                track.x,
                track
                    .y
                    .saturating_add(u16::try_from(thumb_offset).unwrap_or(u16::MAX)),
                track.width,
                u16::try_from(thumb_height).unwrap_or(track.height),
            ),
        })
    }

    /// Renders a real track and thumb without reserving any additional layout width.
    pub fn render(
        frame: &mut Frame,
        viewport: &ViewportState,
        track: Rect,
        track_style: Style,
        thumb_style: Style,
    ) -> Option<ScrollbarGeometry> {
        let geometry = Self::geometry(viewport, track)?;
        for offset in 0..geometry.track.height {
            frame.set_cell(
                geometry.track.x,
                geometry.track.y.saturating_add(offset),
                Cell::new('│', track_style),
            );
        }
        for offset in 0..geometry.thumb.height {
            frame.set_cell(
                geometry.thumb.x,
                geometry.thumb.y.saturating_add(offset),
                Cell::new('█', thumb_style),
            );
        }
        Some(geometry)
    }
}

/// Caller-owned pointer capture for a [`Scrollbar`] thumb.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScrollbarState {
    dragging: bool,
}

impl ScrollbarState {
    pub const fn new() -> Self {
        Self { dragging: false }
    }

    pub const fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn start_drag(&mut self, geometry: ScrollbarGeometry, point: Position) -> bool {
        if geometry.thumb.contains(point) {
            self.dragging = true;
            true
        } else {
            false
        }
    }

    /// Moves the shared viewport by centering the thumb around a track click.
    pub fn track_click(
        &mut self,
        viewport: &mut ViewportState,
        track: Rect,
        point: Position,
    ) -> bool {
        let Some(geometry) = Scrollbar::geometry(viewport, track) else {
            return false;
        };
        if !geometry.track.contains(point) {
            return false;
        }
        set_viewport_from_thumb_top(viewport, geometry, point.y)
    }

    pub fn drag_to(&mut self, viewport: &mut ViewportState, track: Rect, point: Position) -> bool {
        if !self.dragging {
            return false;
        }
        let Some(geometry) = Scrollbar::geometry(viewport, track) else {
            return false;
        };
        set_viewport_from_thumb_top(viewport, geometry, point.y)
    }

    pub fn stop_drag(&mut self) -> bool {
        let was_dragging = self.dragging;
        self.dragging = false;
        was_dragging
    }
}

fn set_viewport_from_thumb_top(
    viewport: &mut ViewportState,
    geometry: ScrollbarGeometry,
    pointer_y: u16,
) -> bool {
    let track_height = usize::from(geometry.track.height);
    let thumb_height = usize::from(geometry.thumb.height);
    let travel = track_height.saturating_sub(thumb_height);
    if travel == 0 {
        return viewport.set_offset(0);
    }
    let centered = usize::from(pointer_y.saturating_sub(geometry.track.y))
        .saturating_sub(thumb_height / 2)
        .min(travel);
    viewport.set_offset(centered.saturating_mul(viewport.max_scroll()) / travel)
}
