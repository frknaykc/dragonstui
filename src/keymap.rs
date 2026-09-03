use crate::{KeyCode, KeyEvent, KeyModifiers};

/// Stable application-defined command identifier used by [`KeyMap`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CommandId(String);

impl CommandId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One exact key/modifier to command mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub command: CommandId,
}

impl KeyBinding {
    pub fn new(code: KeyCode, modifiers: KeyModifiers, command: CommandId) -> Self {
        Self {
            code,
            modifiers,
            command,
        }
    }
}

/// Application-owned exact keybinding resolver.
///
/// Rebinding replaces an exact key/modifier pair; dispatch remains application policy.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyMap {
    bindings: Vec<KeyBinding>,
}

impl KeyMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebinding an exact key/modifier pair replaces it and returns its old command.
    pub fn bind(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        command: CommandId,
    ) -> Option<CommandId> {
        if let Some(binding) = self
            .bindings
            .iter_mut()
            .find(|binding| binding.code == code && binding.modifiers == modifiers)
        {
            return Some(std::mem::replace(&mut binding.command, command));
        }
        self.bindings
            .push(KeyBinding::new(code, modifiers, command));
        None
    }

    pub fn resolve(&self, key: KeyEvent) -> Option<&CommandId> {
        self.bindings
            .iter()
            .find(|binding| binding.code == key.code && binding.modifiers == key.modifiers)
            .map(|binding| &binding.command)
    }
}
