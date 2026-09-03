use std::collections::HashSet;

/// Application-defined stable focus identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FocusId(u32);

impl FocusId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

/// Application-owned ordered focus navigation state.
///
/// It has no automatic event routing or mouse hit testing.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FocusState {
    order: Vec<FocusId>,
    current: Option<FocusId>,
}

impl FocusState {
    pub fn new(order: impl IntoIterator<Item = FocusId>) -> Self {
        let order = deduplicate(order);
        let current = order.first().copied();
        Self { order, current }
    }

    pub fn current(&self) -> Option<FocusId> {
        self.current
    }

    pub fn set_focus(&mut self, id: FocusId) -> bool {
        if !self.order.contains(&id) || self.current == Some(id) {
            return false;
        }

        self.current = Some(id);
        true
    }

    pub fn replace_order(&mut self, order: impl IntoIterator<Item = FocusId>) -> bool {
        let order = deduplicate(order);
        let current = self
            .current
            .filter(|current| order.contains(current))
            .or_else(|| order.first().copied());
        let changed = self.current != current;
        self.order = order;
        self.current = current;
        changed
    }

    pub fn focus_next(&mut self) -> bool {
        self.advance(1)
    }

    pub fn focus_previous(&mut self) -> bool {
        self.advance(-1)
    }

    fn advance(&mut self, direction: i8) -> bool {
        let Some(current) = self.current else {
            return false;
        };
        let Some(index) = self.order.iter().position(|id| *id == current) else {
            return false;
        };
        if self.order.len() <= 1 {
            return false;
        }

        let next = if direction.is_negative() {
            (index + self.order.len() - 1) % self.order.len()
        } else {
            (index + 1) % self.order.len()
        };
        self.current = Some(self.order[next]);
        true
    }
}

fn deduplicate(order: impl IntoIterator<Item = FocusId>) -> Vec<FocusId> {
    let mut seen = HashSet::new();
    order.into_iter().filter(|id| seen.insert(*id)).collect()
}
