use std::collections::BTreeSet;

use crate::{Frame, Line, Rect, Span, Style, ViewportState};

/// A stable, unique-ID tree node with a rich-text label and owned children.
///
/// IDs identify selection and expansion in [`TreeState`], so every node in one [`Tree`] must have
/// a distinct ID that remains stable while its state is retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeNode {
    id: u64,
    label: Line,
    children: Vec<TreeNode>,
}

impl TreeNode {
    /// Creates a node whose ID is unique within its containing tree.
    pub fn new(id: u64, label: impl Into<Line>) -> Self {
        Self {
            id,
            label: label.into(),
            children: Vec::new(),
        }
    }

    pub fn children(mut self, children: impl IntoIterator<Item = TreeNode>) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn label(&self) -> &Line {
        &self.label
    }

    pub fn children_ref(&self) -> &[TreeNode] {
        &self.children
    }
}

/// Caller-owned selection, expansion, and scroll state for [`Tree`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TreeState {
    selected: Option<u64>,
    expanded: BTreeSet<u64>,
    viewport: ViewportState,
}

impl TreeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_selected(&mut self, id: u64) {
        self.selected = Some(id);
    }

    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    pub fn expand(&mut self, id: u64) {
        self.expanded.insert(id);
    }

    pub fn collapse(&mut self, id: u64) {
        self.expanded.remove(&id);
    }

    pub fn is_expanded(&self, id: u64) -> bool {
        self.expanded.contains(&id)
    }

    pub fn scroll_up(&mut self) -> bool {
        self.viewport.scroll_up()
    }

    pub fn scroll_down(&mut self) -> bool {
        self.viewport.scroll_down()
    }

    pub fn page_up(&mut self) -> bool {
        self.viewport.page_up()
    }

    pub fn page_down(&mut self) -> bool {
        self.viewport.page_down()
    }

    pub fn viewport(&self) -> &ViewportState {
        &self.viewport
    }
}

/// A collapsible tree rendered with explicit [`TreeState`].
///
/// Node IDs must be stable within an application's data set because expansion and selection are
/// keyed by ID rather than by tree position.
pub struct Tree {
    roots: Vec<TreeNode>,
    selected_style: Style,
}

impl Tree {
    pub fn new(roots: impl IntoIterator<Item = TreeNode>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
            selected_style: Style::new(),
        }
    }

    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }

    pub fn move_up(&self, state: &mut TreeState) -> bool {
        let visible = self.normalize(state);
        let Some(index) = visible
            .iter()
            .position(|row| Some(row.node.id) == state.selected)
        else {
            return false;
        };
        if index == 0 {
            return false;
        }
        state.selected = Some(visible[index - 1].node.id);
        true
    }

    pub fn move_down(&self, state: &mut TreeState) -> bool {
        let visible = self.normalize(state);
        let Some(index) = visible
            .iter()
            .position(|row| Some(row.node.id) == state.selected)
        else {
            return false;
        };
        let Some(next) = visible.get(index + 1) else {
            return false;
        };
        state.selected = Some(next.node.id);
        true
    }

    pub fn move_right(&self, state: &mut TreeState) -> bool {
        let visible = self.normalize(state);
        let Some(node) = visible
            .iter()
            .find(|row| Some(row.node.id) == state.selected)
            .map(|row| row.node)
        else {
            return false;
        };
        let Some(first_child) = node.children.first() else {
            return false;
        };
        if state.expanded.insert(node.id) {
            return true;
        }
        if state.selected == Some(first_child.id) {
            return false;
        }
        state.selected = Some(first_child.id);
        true
    }

    pub fn move_left(&self, state: &mut TreeState) -> bool {
        let visible = self.normalize(state);
        let Some(row) = visible
            .iter()
            .find(|row| Some(row.node.id) == state.selected)
        else {
            return false;
        };
        if state.expanded.remove(&row.node.id) {
            return true;
        }
        let Some(parent) = row.parent else {
            return false;
        };
        state.selected = Some(parent);
        true
    }

    pub fn toggle(&self, state: &mut TreeState) -> bool {
        let visible = self.normalize(state);
        let Some(node) = visible
            .iter()
            .find(|row| Some(row.node.id) == state.selected)
            .map(|row| row.node)
        else {
            return false;
        };
        if node.children.is_empty() {
            return false;
        }
        if !state.expanded.remove(&node.id) {
            state.expanded.insert(node.id);
        }
        true
    }

    pub fn render(&self, frame: &mut Frame, rect: Rect, state: &mut TreeState) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let visible = self.normalize(state);
        state.viewport.update_dimensions(visible.len(), rect.height);
        let start = state.viewport.offset();
        let end = start
            .saturating_add(usize::from(rect.height))
            .min(visible.len());
        for (row_index, row) in visible[start..end].iter().enumerate() {
            let prefix = format!(
                "{}{} ",
                "  ".repeat(row.depth),
                if row.node.children.is_empty() {
                    '•'
                } else if state.is_expanded(row.node.id) {
                    '▼'
                } else {
                    '▶'
                }
            );
            let selected = state.selected == Some(row.node.id);
            let line = Line::new(
                [Span::styled(
                    prefix,
                    if selected {
                        self.selected_style
                    } else {
                        Style::new()
                    },
                )]
                .into_iter()
                .chain(row.node.label.spans().iter().map(|span| {
                    Span::styled(
                        span.content(),
                        if selected {
                            span.style().patch(self.selected_style)
                        } else {
                            span.style()
                        },
                    )
                })),
            );
            let Some(y) = rect.y.checked_add(row_index as u16) else {
                break;
            };
            line.render(frame, Rect::new(rect.x, y, rect.width, 1));
        }
    }

    fn normalize<'a>(&'a self, state: &mut TreeState) -> Vec<VisibleNode<'a>> {
        let mut ids = BTreeSet::new();
        collect_ids(&self.roots, &mut ids);
        state.expanded.retain(|id| ids.contains(id));
        let mut visible = Vec::new();
        flatten(&self.roots, None, 0, &state.expanded, &mut visible);
        if !visible
            .iter()
            .any(|row| Some(row.node.id) == state.selected)
        {
            state.selected = visible.first().map(|row| row.node.id);
        }
        visible
    }
}

struct VisibleNode<'a> {
    node: &'a TreeNode,
    parent: Option<u64>,
    depth: usize,
}

fn collect_ids(nodes: &[TreeNode], ids: &mut BTreeSet<u64>) {
    for node in nodes {
        ids.insert(node.id);
        collect_ids(&node.children, ids);
    }
}

fn flatten<'a>(
    nodes: &'a [TreeNode],
    parent: Option<u64>,
    depth: usize,
    expanded: &BTreeSet<u64>,
    visible: &mut Vec<VisibleNode<'a>>,
) {
    for node in nodes {
        visible.push(VisibleNode {
            node,
            parent,
            depth,
        });
        if expanded.contains(&node.id) {
            flatten(
                &node.children,
                Some(node.id),
                depth.saturating_add(1),
                expanded,
                visible,
            );
        }
    }
}
