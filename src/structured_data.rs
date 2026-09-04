use serde_json::Value;

use crate::{Frame, Line, Rect, Style, Tree, TreeNode, TreeState};

/// A borrowed, read-only adapter from a `serde_json::Value` to the reusable [`Tree`] primitive.
///
/// Node identity derives from a canonical path, so selection and expansion stay stable when an
/// equivalent structured value is rendered again.
pub struct StructuredData<'a> {
    value: &'a Value,
}

impl<'a> StructuredData<'a> {
    pub fn new(value: &'a Value) -> Self {
        Self { value }
    }

    /// Returns the stable node ID for an existing canonical path.
    pub fn id_for_path(&self, path: &str) -> Option<u64> {
        let mut paths = Vec::new();
        collect_paths(self.value, "root", &mut paths);
        paths
            .into_iter()
            .any(|candidate| candidate == path)
            .then(|| path_id(path))
    }

    pub fn move_up(&self, state: &mut TreeState) -> bool {
        self.tree().move_up(state)
    }

    pub fn move_down(&self, state: &mut TreeState) -> bool {
        self.tree().move_down(state)
    }

    pub fn move_left(&self, state: &mut TreeState) -> bool {
        self.tree().move_left(state)
    }

    pub fn move_right(&self, state: &mut TreeState) -> bool {
        self.tree().move_right(state)
    }

    pub fn toggle(&self, state: &mut TreeState) -> bool {
        self.tree().toggle(state)
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        rect: Rect,
        state: &mut TreeState,
        selected_style: Style,
    ) {
        self.tree()
            .selected_style(selected_style)
            .render(frame, rect, state);
    }

    fn tree(&self) -> Tree {
        Tree::new([tree_node(self.value, "root", "root")])
    }
}

fn tree_node(value: &Value, path: &str, label: &str) -> TreeNode {
    let children = match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, child)| tree_node(child, &object_path(path, key), key))
            .collect(),
        Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, child)| {
                let label = format!("[{index}]");
                tree_node(child, &format!("{path}[{index}]"), &label)
            })
            .collect(),
        _ => Vec::new(),
    };
    let display = if children.is_empty() {
        format!("{label}: {}", scalar_text(value))
    } else {
        label.to_owned()
    };
    TreeNode::new(path_id(path), Line::from(display)).children(children)
}

fn collect_paths(value: &Value, path: &str, paths: &mut Vec<String>) {
    paths.push(path.to_owned());
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                collect_paths(child, &object_path(path, key), paths);
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                collect_paths(child, &format!("{path}[{index}]"), paths);
            }
        }
        _ => {}
    }
}

fn object_path(parent: &str, key: &str) -> String {
    if key
        .chars()
        .all(|character| character.is_alphanumeric() || character == '_')
    {
        format!("{parent}.{key}")
    } else {
        format!("{parent}[{:?}]", key)
    }
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::Object(_) => "{}".to_owned(),
        Value::Array(_) => "[]".to_owned(),
        _ => value.to_string(),
    }
}

fn path_id(path: &str) -> u64 {
    path.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
