//! 把 `Settings` 解析为键位 / 编辑器 / LSP 覆盖配置的纯函数。`Workbench::new`（首次注册
//! 服务）与 `tick::reload_settings`（热重载）共消费此结果，各自保留分歧副作用（前者注册
//! 新服务 + env override 优先，后者 dispatch + reconfigure），避免两份解析逐字漂移。

use crate::core::Command;
use crate::kernel::services::adapters::lsp::LspServerCommandOverride;
use crate::kernel::services::adapters::settings::parse_keybinding;
use crate::kernel::services::adapters::{KeybindingContext, KeybindingService};
use crate::kernel::services::ports::{EditorConfig, LspServerKind, Settings};
use rustc_hash::FxHashMap;

/// `Settings` 解析结果：键位绑定、编辑器配置、全局 LSP 覆盖、按 server 覆盖。
pub(super) struct ParsedSettings {
    pub keybindings: KeybindingService,
    pub editor_config: EditorConfig,
    pub lsp_settings_override: Option<(String, Vec<String>, Option<serde_json::Value>)>,
    pub lsp_server_overrides: FxHashMap<LspServerKind, LspServerCommandOverride>,
}

pub(super) fn parse_settings(settings: Settings) -> ParsedSettings {
    let mut keybindings = KeybindingService::new();
    let mut lsp_settings_override = None;
    let mut lsp_server_overrides: FxHashMap<LspServerKind, LspServerCommandOverride> =
        FxHashMap::default();

    for rule in settings.keybindings {
        if let Some(key) = parse_keybinding(&rule.key) {
            let context = rule
                .context
                .as_deref()
                .and_then(KeybindingContext::parse)
                .unwrap_or(KeybindingContext::Global);
            if rule.command.trim().is_empty() {
                let _ = keybindings.unbind(context, &key);
            } else {
                keybindings.bind(context, key, Command::from_name(&rule.command));
            }
        }
    }

    if let Some(command) = settings
        .lsp
        .command
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let args = settings
            .lsp
            .args
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        lsp_settings_override = Some((command.to_string(), args, None));
    }

    for (name, cfg) in &settings.lsp.servers {
        let Some(kind) = LspServerKind::from_settings_key(name) else {
            continue;
        };

        let command = cfg
            .command
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let args = cfg.args.as_ref().map(|args| {
            args.iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        });

        let entry = lsp_server_overrides.entry(kind).or_default();
        if let Some(command) = command {
            entry.command = Some(command);
        }
        if let Some(args) = args {
            entry.args = Some(args);
        }
        if let Some(initialization_options) = cfg.initialization_options.clone() {
            entry.initialization_options = Some(initialization_options);
        }
    }

    ParsedSettings {
        keybindings,
        editor_config: settings.editor,
        lsp_settings_override,
        lsp_server_overrides,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app/workbench/settings_parse.rs"]
mod tests;
