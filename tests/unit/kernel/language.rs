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
        (LanguageId::Rust, LspServerKind::RustAnalyzer),
        (LanguageId::Go, LspServerKind::Gopls),
        (LanguageId::Python, LspServerKind::Pyright),
        (
            LanguageId::JavaScript,
            LspServerKind::TypeScriptLanguageServer,
        ),
        (
            LanguageId::TypeScript,
            LspServerKind::TypeScriptLanguageServer,
        ),
        (LanguageId::Jsx, LspServerKind::TypeScriptLanguageServer),
        (LanguageId::Tsx, LspServerKind::TypeScriptLanguageServer),
        (LanguageId::C, LspServerKind::Clangd),
        (LanguageId::Cpp, LspServerKind::Clangd),
        (LanguageId::Java, LspServerKind::Jdtls),
    ];

    for (language, expected) in cases {
        assert_eq!(language.server_kind(), expected);
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
