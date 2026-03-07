use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::LspServerKind;
use std::path::Path;

#[test]
fn from_path_maps_all_supported_extensions() {
    let cases = [
        ("a.rs", Some(LanguageId::Rust)),
        ("a.go", Some(LanguageId::Go)),
        ("a.py", Some(LanguageId::Python)),
        ("a.pyi", Some(LanguageId::Python)),
        ("a.js", Some(LanguageId::JavaScript)),
        ("a.mjs", Some(LanguageId::JavaScript)),
        ("a.cjs", Some(LanguageId::JavaScript)),
        ("a.jsx", Some(LanguageId::Jsx)),
        ("a.ts", Some(LanguageId::TypeScript)),
        ("a.mts", Some(LanguageId::TypeScript)),
        ("a.cts", Some(LanguageId::TypeScript)),
        ("a.tsx", Some(LanguageId::Tsx)),
        ("a.c", Some(LanguageId::C)),
        ("a.cpp", Some(LanguageId::Cpp)),
        ("a.cc", Some(LanguageId::Cpp)),
        ("a.cxx", Some(LanguageId::Cpp)),
        ("a.hpp", Some(LanguageId::Cpp)),
        ("a.hh", Some(LanguageId::Cpp)),
        ("a.hxx", Some(LanguageId::Cpp)),
        ("a.h", Some(LanguageId::Cpp)),
        ("a.java", Some(LanguageId::Java)),
        ("a.json", Some(LanguageId::Json)),
        ("a.yaml", Some(LanguageId::Yaml)),
        ("a.yml", Some(LanguageId::Yaml)),
        ("a.html", Some(LanguageId::Html)),
        ("a.htm", Some(LanguageId::Html)),
        ("a.xml", Some(LanguageId::Xml)),
        ("a.xsl", Some(LanguageId::Xml)),
        ("a.svg", Some(LanguageId::Xml)),
        ("a.css", Some(LanguageId::Css)),
        ("a.toml", Some(LanguageId::Toml)),
        ("a.sql", Some(LanguageId::Sql)),
        ("a.sh", Some(LanguageId::Bash)),
        ("a.bash", Some(LanguageId::Bash)),
        ("a.zsh", Some(LanguageId::Bash)),
        ("a.txt", None),
    ];

    for (path, expected) in cases {
        assert_eq!(LanguageId::from_path(Path::new(path)), expected);
    }
}

#[test]
fn lsp_language_id_mapping_is_correct() {
    let cases = [
        (LanguageId::Rust, "rust"),
        (LanguageId::Go, "go"),
        (LanguageId::Python, "python"),
        (LanguageId::JavaScript, "javascript"),
        (LanguageId::TypeScript, "typescript"),
        (LanguageId::Jsx, "javascriptreact"),
        (LanguageId::Tsx, "typescriptreact"),
        (LanguageId::C, "c"),
        (LanguageId::Cpp, "cpp"),
        (LanguageId::Java, "java"),
    ];

    for (language, expected) in cases {
        assert_eq!(language.language_id(), expected);
    }
}

#[test]
fn server_kind_mapping_is_correct() {
    let cases = [
        (LanguageId::Rust, Some(LspServerKind::RustAnalyzer)),
        (LanguageId::Go, Some(LspServerKind::Gopls)),
        (LanguageId::Python, Some(LspServerKind::Pyright)),
        (
            LanguageId::JavaScript,
            Some(LspServerKind::TypeScriptLanguageServer),
        ),
        (
            LanguageId::TypeScript,
            Some(LspServerKind::TypeScriptLanguageServer),
        ),
        (
            LanguageId::Jsx,
            Some(LspServerKind::TypeScriptLanguageServer),
        ),
        (
            LanguageId::Tsx,
            Some(LspServerKind::TypeScriptLanguageServer),
        ),
        (LanguageId::C, Some(LspServerKind::Clangd)),
        (LanguageId::Cpp, Some(LspServerKind::Clangd)),
        (LanguageId::Java, Some(LspServerKind::Jdtls)),
    ];

    for (language, expected) in cases {
        assert_eq!(language.server_kind(), expected);
    }
}

#[test]
fn highlight_only_languages_have_no_server_kind() {
    let highlight_only = [
        LanguageId::Json,
        LanguageId::Yaml,
        LanguageId::Html,
        LanguageId::Xml,
        LanguageId::Css,
        LanguageId::Toml,
        LanguageId::Sql,
        LanguageId::Bash,
    ];

    for language in highlight_only {
        assert_eq!(
            language.server_kind(),
            None,
            "{:?} should not have a server kind",
            language
        );
        assert!(
            language.markers().is_empty(),
            "{:?} should have empty markers",
            language
        );
    }
}

#[test]
fn markers_mapping_is_correct() {
    assert_eq!(
        LanguageId::Rust.markers(),
        ["Cargo.toml", "rust-project.json"]
    );
    assert_eq!(LanguageId::Go.markers(), ["go.work", "go.mod"]);
    assert_eq!(
        LanguageId::Python.markers(),
        [
            "pyproject.toml",
            "pyrightconfig.json",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
        ]
    );
    assert_eq!(
        LanguageId::JavaScript.markers(),
        ["tsconfig.json", "jsconfig.json", "package.json"]
    );
    assert_eq!(
        LanguageId::TypeScript.markers(),
        ["tsconfig.json", "jsconfig.json", "package.json"]
    );
    assert_eq!(
        LanguageId::Jsx.markers(),
        ["tsconfig.json", "jsconfig.json", "package.json"]
    );
    assert_eq!(
        LanguageId::Tsx.markers(),
        ["tsconfig.json", "jsconfig.json", "package.json"]
    );
    assert_eq!(
        LanguageId::C.markers(),
        [
            "compile_commands.json",
            "compile_flags.txt",
            "CMakeLists.txt",
            "meson.build",
            "Makefile",
        ]
    );
    assert_eq!(
        LanguageId::Cpp.markers(),
        [
            "compile_commands.json",
            "compile_flags.txt",
            "CMakeLists.txt",
            "meson.build",
            "Makefile",
        ]
    );
    assert_eq!(
        LanguageId::Java.markers(),
        [
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "gradlew",
            ".project",
        ]
    );
}

use crate::kernel::editor::TabId;
use crate::kernel::services::ports::{EditorConfig, LspCompletionItem, LspInsertTextFormat};

fn test_tab(path: &str, content: &str, col: usize) -> crate::kernel::editor::EditorTabState {
    let config = EditorConfig::default();
    let mut tab = crate::kernel::editor::EditorTabState::from_file(
        TabId::new(1),
        path.into(),
        content,
        &config,
    );
    tab.buffer.set_cursor(0, col);
    tab
}

fn callable_item(label: &str) -> LspCompletionItem {
    LspCompletionItem {
        id: 1,
        label: label.to_string(),
        detail: None,
        kind: Some(3),
        documentation: None,
        insert_text: label.to_string(),
        insert_text_format: LspInsertTextFormat::PlainText,
        insert_range: None,
        replace_range: None,
        sort_text: None,
        filter_text: None,
        additional_text_edits: Vec::new(),
        command: None,
        data: None,
    }
}

fn normalize_plan(
    tab: &crate::kernel::editor::EditorTabState,
    item: &LspCompletionItem,
) -> crate::kernel::language::TextEditPlan {
    let adapter = crate::kernel::language::adapter_for(tab.language());
    adapter.completion_protocol().normalize_completion_text(
        &crate::kernel::language::CompletionContext::live(
            crate::kernel::language::LanguageRuntimeContext::new(
                tab.language(),
                tab,
                adapter.syntax().syntax_facts(tab),
            ),
            item,
        ),
    )
}

fn normalize_snapshot_plan(
    tab: &crate::kernel::editor::EditorTabState,
    item: &LspCompletionItem,
) -> crate::kernel::language::TextEditPlan {
    let adapter = crate::kernel::language::adapter_for(tab.language());
    let runtime = crate::kernel::language::LanguageRuntimeContext::new(
        tab.language(),
        tab,
        adapter.syntax().syntax_facts(tab),
    );
    let snapshot = runtime.completion_snapshot();
    adapter.completion_protocol().normalize_completion_text(
        &crate::kernel::language::CompletionContext::snapshot(snapshot, item),
    )
}

#[test]
fn default_adapter_does_not_synthesize_callable_fallback() {
    let tab = test_tab("Main.java", "pri", 3);
    let plan = normalize_plan(&tab, &callable_item("print"));

    assert_eq!(plan.text, "print");
    assert_eq!(plan.cursor, None);
    assert_eq!(
        plan.strategy,
        crate::kernel::language::TextEditStrategy::PlainText
    );
}

#[test]
fn rust_adapter_does_not_synthesize_callable_fallback() {
    let tab = test_tab("main.rs", "pri", 3);
    let plan = normalize_plan(&tab, &callable_item("println"));

    assert_eq!(plan.text, "println");
    assert_eq!(plan.cursor, None);
    assert_eq!(
        plan.strategy,
        crate::kernel::language::TextEditStrategy::PlainText
    );
}

#[test]
fn c_family_adapter_applies_callable_fallback_in_normal_context() {
    let tab = test_tab("main.cpp", "pri", 3);
    let plan = normalize_plan(&tab, &callable_item("printf"));

    assert_eq!(plan.text, "printf()");
    assert_eq!(plan.cursor, Some("printf(".chars().count()));
    assert_eq!(
        plan.strategy,
        crate::kernel::language::TextEditStrategy::CallableTemplate
    );
}

#[test]
fn c_family_adapter_disables_callable_fallback_in_special_contexts() {
    let cases = [
        ("main.cpp", "obj->", "push_back"),
        ("main.cpp", "std::", "vector"),
        ("main.cpp", "#include <vec", "header"),
    ];

    for (path, content, insert_text) in cases {
        let tab = test_tab(path, content, content.chars().count());
        let plan = normalize_plan(&tab, &callable_item(insert_text));
        assert_eq!(plan.text, insert_text, "case: {content}");
        assert!(plan.cursor.is_none(), "case: {content}");
        assert_eq!(
            plan.strategy,
            crate::kernel::language::TextEditStrategy::PlainText,
            "case: {content}"
        );
    }
}

#[test]
fn c_family_snapshot_matches_live_callable_fallback() {
    let tab = test_tab("main.cpp", "pri", 3);
    let live = normalize_plan(&tab, &callable_item("printf"));
    let snapshot = normalize_snapshot_plan(&tab, &callable_item("printf"));

    assert_eq!(snapshot, live);
    assert_eq!(snapshot.text, "printf()");
    assert_eq!(snapshot.cursor, Some("printf(".chars().count()));
}

#[test]
fn c_family_snapshot_preserves_special_context_suppression() {
    let tab = test_tab("main.cpp", "obj->", "obj->".chars().count());
    let live = normalize_plan(&tab, &callable_item("push_back"));
    let snapshot = normalize_snapshot_plan(&tab, &callable_item("push_back"));

    assert_eq!(snapshot, live);
    assert_eq!(snapshot.text, "push_back");
    assert!(snapshot.cursor.is_none());
}

#[test]
fn syntax_bridge_reports_string_and_comment_context() {
    let string_tab = test_tab(
        "main.rs",
        "let value = \"hi\";",
        "let value = \"h".chars().count(),
    );
    let string_adapter = crate::kernel::language::adapter_for(string_tab.language());
    let string_facts = string_adapter.syntax().syntax_facts(&string_tab);
    assert!(string_facts.in_string);
    assert!(!string_facts.in_comment);

    let comment_tab = test_tab("main.rs", "// note", 3);
    let comment_adapter = crate::kernel::language::adapter_for(comment_tab.language());
    let comment_facts = comment_adapter.syntax().syntax_facts(&comment_tab);
    assert!(comment_facts.in_comment);
    assert!(!comment_facts.in_string);
}

#[test]
fn syntax_bridge_reports_identifier_bounds_and_member_access() {
    let tab = test_tab("main.rs", "foo::bar", "foo::bar".chars().count());
    let adapter = crate::kernel::language::adapter_for(tab.language());
    let facts = adapter.syntax().syntax_facts(&tab);

    assert_eq!(facts.identifier_bounds, Some((5, 8)));
    assert_eq!(
        facts.member_access_kind,
        Some(crate::kernel::language::MemberAccessKind::Scope)
    );
}

#[test]
fn markdown_adapter_still_provides_fallback_syntax_facts() {
    let tab = test_tab("README.md", "hello world", 5);
    let adapter = crate::kernel::language::adapter_for(tab.language());
    let facts = adapter.syntax().syntax_facts(&tab);

    assert_eq!(adapter.features().lsp_server, None);
    assert!(!adapter.features().has_syntax);
    assert_eq!(facts.identifier_bounds, Some((0, 5)));
    assert!(!facts.in_string);
    assert!(!facts.in_comment);
}
