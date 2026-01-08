use crate::core::Command;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginPriority {
    High,
    Low,
}

impl Default for PluginPriority {
    fn default() -> Self {
        Self::Low
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusSide {
    Left,
    Right,
}

impl Default for StatusSide {
    fn default() -> Self {
        Self::Left
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCommandDecl {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginStatusItemDecl {
    pub id: String,
    #[serde(default)]
    pub side: StatusSide,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegisterParams {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub commands: Vec<PluginCommandDecl>,
    #[serde(default)]
    pub status_items: Vec<PluginStatusItemDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatusItemPatch {
    pub id: String,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUiPatchParams {
    #[serde(default)]
    pub status_items: Vec<PluginStatusItemPatch>,
}

#[derive(Debug, Clone)]
pub enum PluginAction {
    Discovered {
        id: String,
        priority: PluginPriority,
    },
    Registered {
        id: String,
        name: Option<String>,
        priority: PluginPriority,
        commands: Vec<PluginCommandDecl>,
        status_items: Vec<PluginStatusItemDecl>,
    },
    UiPatch {
        id: String,
        patch: PluginUiPatchParams,
    },
    Online {
        id: String,
    },
    Offline {
        id: String,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct PluginPaletteItem {
    pub label: String,
    pub label_lc: String,
    pub command: Command,
    pub plugin_id: String,
    pub command_id: String,
}

#[derive(Debug, Clone)]
pub struct PluginStatusItem {
    pub id: String,
    pub side: StatusSide,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct PluginState {
    pub id: String,
    pub name: Option<String>,
    pub priority: PluginPriority,
    pub online: bool,
    pub commands: Vec<PluginCommandDecl>,
    pub status_items: FxHashMap<String, PluginStatusItem>,
    pub status_order: Vec<String>,
}

impl PluginState {
    fn empty(id: String, priority: PluginPriority) -> Self {
        Self {
            id,
            name: None,
            priority,
            online: false,
            commands: Vec::new(),
            status_items: FxHashMap::default(),
            status_order: Vec::new(),
        }
    }

    fn rebuild_status_index(&mut self, items: Vec<PluginStatusItemDecl>) {
        let mut status_items = FxHashMap::default();
        status_items.reserve(items.len());
        let mut status_order = Vec::with_capacity(items.len());
        for item in items {
            status_order.push(item.id.clone());
            status_items.insert(
                item.id.clone(),
                PluginStatusItem {
                    id: item.id,
                    side: item.side,
                    text: item.text,
                },
            );
        }
        self.status_items = status_items;
        self.status_order = status_order;
    }
}

#[derive(Debug, Default, Clone)]
pub struct PluginsState {
    by_id: FxHashMap<String, PluginState>,
    order: Vec<String>,
    palette_items: Vec<PluginPaletteItem>,
}

impl PluginsState {
    pub fn palette_items(&self) -> &[PluginPaletteItem] {
        &self.palette_items
    }

    pub fn plugin(&self, id: &str) -> Option<&PluginState> {
        self.by_id.get(id)
    }

    pub fn plugins_in_order(&self) -> impl Iterator<Item = &PluginState> {
        self.order.iter().filter_map(|id| self.by_id.get(id))
    }

    pub fn dispatch(&mut self, action: PluginAction) -> bool {
        match action {
            PluginAction::Discovered { id, priority } => self.discovered(id, priority),
            PluginAction::Registered {
                id,
                name,
                priority,
                commands,
                status_items,
            } => self.registered(id, name, priority, commands, status_items),
            PluginAction::UiPatch { id, patch } => self.ui_patch(&id, patch),
            PluginAction::Online { id } => self.set_online(&id, true, None),
            PluginAction::Offline { id, reason } => self.set_online(&id, false, reason),
        }
    }

    fn discovered(&mut self, id: String, priority: PluginPriority) -> bool {
        if let Some(existing) = self.by_id.get_mut(&id) {
            if existing.priority == priority {
                return false;
            }
            existing.priority = priority;
            return true;
        }

        self.order.push(id.clone());
        self.by_id.insert(id.clone(), PluginState::empty(id, priority));
        true
    }

    fn registered(
        &mut self,
        id: String,
        name: Option<String>,
        priority: PluginPriority,
        commands: Vec<PluginCommandDecl>,
        status_items: Vec<PluginStatusItemDecl>,
    ) -> bool {
        let plugin = self
            .by_id
            .entry(id.clone())
            .or_insert_with(|| {
                self.order.push(id.clone());
                PluginState::empty(id.clone(), priority)
            });

        let prev_online = plugin.online;

        let status_changed = status_decl_differs(plugin, &status_items);
        let commands_changed = plugin.commands != commands;
        let changed = plugin.name != name
            || plugin.priority != priority
            || commands_changed
            || status_changed
            || !prev_online;

        plugin.online = true;
        plugin.name = name;
        plugin.priority = priority;
        plugin.commands = commands;
        plugin.rebuild_status_index(status_items);

        if commands_changed {
            self.rebuild_palette_items();
        }

        changed
    }

    fn ui_patch(&mut self, id: &str, patch: PluginUiPatchParams) -> bool {
        let Some(plugin) = self.by_id.get_mut(id) else {
            return false;
        };

        let mut changed = false;
        for item in patch.status_items {
            if let Some(text) = item.text {
                if let Some(existing) = plugin.status_items.get_mut(&item.id) {
                    if existing.text != text {
                        existing.text = text;
                        changed = true;
                    }
                }
            }
        }

        changed
    }

    fn set_online(&mut self, id: &str, online: bool, _reason: Option<String>) -> bool {
        let Some(plugin) = self.by_id.get_mut(id) else {
            return false;
        };
        if plugin.online == online {
            return false;
        }
        plugin.online = online;
        true
    }

    fn rebuild_palette_items(&mut self) {
        let mut items = Vec::new();
        let estimated = self.by_id.values().map(|p| p.commands.len()).sum::<usize>();
        items.reserve(estimated);

        for plugin in self.plugins_in_order() {
            for cmd in &plugin.commands {
                let command_id = cmd.id.clone();
                let command_name = format!("plugin:{}:{}", plugin.id, command_id);
                let label = cmd.title.clone();
                let label_lc = label.to_ascii_lowercase();

                items.push(PluginPaletteItem {
                    label,
                    label_lc,
                    command: Command::Custom(command_name),
                    plugin_id: plugin.id.clone(),
                    command_id,
                });
            }
        }

        self.palette_items = items;
    }
}

pub fn parse_plugin_command_name(name: &str) -> Option<(&str, &str)> {
    let (prefix, rest) = name.split_once(':')?;
    if prefix != "plugin" {
        return None;
    }
    let (plugin_id, command_id) = rest.split_once(':')?;
    if plugin_id.is_empty() || command_id.is_empty() {
        return None;
    }
    Some((plugin_id, command_id))
}

fn status_decl_differs(plugin: &PluginState, items: &[PluginStatusItemDecl]) -> bool {
    if plugin.status_items.len() != items.len() || plugin.status_order.len() != items.len() {
        return true;
    }

    for (idx, item) in items.iter().enumerate() {
        if plugin.status_order.get(idx).map(String::as_str) != Some(item.id.as_str()) {
            return true;
        }
        let Some(existing) = plugin.status_items.get(&item.id) else {
            return true;
        };
        if existing.side != item.side || existing.text != item.text {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plugin_command_name_accepts_expected_shape() {
        assert_eq!(
            parse_plugin_command_name("plugin:git:refresh"),
            Some(("git", "refresh"))
        );
        assert_eq!(parse_plugin_command_name("plugin::x"), None);
        assert_eq!(parse_plugin_command_name("plugin:x:"), None);
        assert_eq!(parse_plugin_command_name("other:git:refresh"), None);
    }
}
