use super::*;
use crate::kernel::services::ports::settings::{LspServerConfig, LspSettings};
use std::collections::BTreeMap;

#[test]
fn parses_global_lsp_override_trimming_whitespace() {
    let settings = Settings {
        lsp: LspSettings {
            command: Some("  rust-analyzer  ".to_string()),
            args: vec!["".to_string(), "  --foo  ".to_string(), "   ".to_string()],
            ..Default::default()
        },
        ..Default::default()
    };
    let parsed = parse_settings(settings);
    assert_eq!(
        parsed.lsp_settings_override,
        Some(("rust-analyzer".to_string(), vec!["--foo".to_string()], None))
    );
}

#[test]
fn blank_global_lsp_command_yields_no_override() {
    let settings = Settings {
        lsp: LspSettings {
            command: Some("   ".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(parse_settings(settings).lsp_settings_override.is_none());
}

#[test]
fn parses_per_server_override_by_kind_and_ignores_unknown() {
    let mut servers = BTreeMap::new();
    servers.insert(
        "gopls".to_string(),
        LspServerConfig {
            command: Some(" /bin/gopls ".to_string()),
            args: Some(vec!["-x".to_string(), "  ".to_string()]),
            initialization_options: None,
        },
    );
    servers.insert(
        "not-a-real-server".to_string(),
        LspServerConfig {
            command: Some("nope".to_string()),
            ..Default::default()
        },
    );
    let settings = Settings {
        lsp: LspSettings {
            servers,
            ..Default::default()
        },
        ..Default::default()
    };

    let parsed = parse_settings(settings);
    let gopls = parsed
        .lsp_server_overrides
        .get(&LspServerKind::Gopls)
        .expect("gopls override present");
    assert_eq!(gopls.command.as_deref(), Some("/bin/gopls"));
    assert_eq!(gopls.args.as_deref(), Some(&["-x".to_string()][..]));
    // Unknown server keys are dropped, so only gopls remains.
    assert_eq!(parsed.lsp_server_overrides.len(), 1);
}

#[test]
fn empty_settings_produce_no_overrides() {
    let parsed = parse_settings(Settings::default());
    assert!(parsed.lsp_settings_override.is_none());
    assert!(parsed.lsp_server_overrides.is_empty());
}
