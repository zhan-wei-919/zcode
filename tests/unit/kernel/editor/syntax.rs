use super::*;
use crate::models::{EditOp, OpId};
use ropey::Rope;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_highlight_comment_range_rust() {
    let rope = Rope::from_str("fn main() { // hi\n}\n");
    let doc = SyntaxDocument::for_path(Path::new("test.rs"), &rope).expect("rust syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    assert_eq!(spans.len(), 1);

    let line = "fn main() { // hi";
    let idx = line.find("//").unwrap();

    assert!(spans[0]
        .iter()
        .any(|s| { s.kind == HighlightKind::Comment && s.start <= idx && idx < s.end }));
}

#[test]
fn test_highlight_rust_macro_string_prefers_string_over_macro() {
    let src = "fn main() {\n    tracing::info!(settings_path = %path.display(), \"settings ready\");\n}\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.rs"), &rope).expect("rust syntax");

    let spans = doc.highlight_lines(&rope, 1, 2);
    let line = "    tracing::info!(settings_path = %path.display(), \"settings ready\");";
    let in_string = line.find("settings ready").unwrap();

    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= in_string && in_string < s.end));

    assert!(!spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Macro && s.start <= in_string && in_string < s.end));

    let mut cursor = 0usize;
    while cursor < spans[0].len() && spans[0][cursor].end <= in_string {
        cursor += 1;
    }
    let chosen_kind = spans[0]
        .get(cursor)
        .filter(|span| span.start <= in_string && in_string < span.end)
        .map(|span| span.kind);

    assert_eq!(chosen_kind, Some(HighlightKind::String));
}

#[test]
fn test_highlight_go_comment_string_keyword_and_in_string_or_comment() {
    let src = "package main\n// hi\nfunc main() { println(\"x\") }\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.go"), &rope).expect("go syntax");

    let spans = doc.highlight_lines(&rope, 1, 2);
    let line = "// hi";
    let idx = line.find("//").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Comment && s.start <= idx && idx < s.end));

    let spans = doc.highlight_lines(&rope, 2, 3);
    let line = "func main() { println(\"x\") }";
    let idx_func = line.find("func").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_func && idx_func < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));

    let in_comment = src.find("// hi").unwrap() + 3;
    assert!(doc.is_in_string_or_comment(in_comment));
    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("package").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}

#[test]
fn test_highlight_go_richer_symbols() {
    let src = "package main\ntype Counter struct { value int }\nfunc add(x int, y int) int { return x + y }\nfunc (c *Counter) Inc(delta int) int { return c.value + delta }\nfunc Map[T any](x T) T { return x }\nfunc main() {\n    c := &Counter{value: 1}\n    n := add(1, 2)\n    m := c.Inc(n)\n    println(m)\n}\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.go"), &rope).expect("go syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "package main";
    let idx_package_name = line.find("main").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Attribute
            && s.start <= idx_package_name
            && idx_package_name < s.end
    }));

    let spans = doc.highlight_lines(&rope, 1, 2);
    let line = "type Counter struct { value int }";
    let idx_type_name = line.find("Counter").unwrap();
    let idx_field_name = line.find("value").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Type
        && s.start <= idx_type_name
        && idx_type_name < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_field_name && idx_field_name < s.end
    }));

    let spans = doc.highlight_lines(&rope, 2, 3);
    let line = "func add(x int, y int) int { return x + y }";
    let idx_function_name = line.find("add").unwrap();
    let idx_param_x = line.find("x").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function
            && s.start <= idx_function_name
            && idx_function_name < s.end
    }));
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Variable
        && s.start <= idx_param_x
        && idx_param_x < s.end));

    let spans = doc.highlight_lines(&rope, 3, 4);
    let line = "func (c *Counter) Inc(delta int) int { return c.value + delta }";
    let idx_method_name = line.find("Inc").unwrap();
    let idx_param_delta = line.find("delta").unwrap();
    let idx_member_value = line.find("value").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function && s.start <= idx_method_name && idx_method_name < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_param_delta && idx_param_delta < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_member_value && idx_member_value < s.end
    }));

    let spans = doc.highlight_lines(&rope, 4, 5);
    let line = "func Map[T any](x T) T { return x }";
    let idx_type_param = line.find("[T").unwrap() + 1;
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Type
        && s.start <= idx_type_param
        && idx_type_param < s.end));

    let spans = doc.highlight_lines(&rope, 8, 9);
    let line = "    m := c.Inc(n)";
    let idx_call_method = line.find("Inc").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function && s.start <= idx_call_method && idx_call_method < s.end
    }));

    let spans = doc.highlight_lines(&rope, 9, 10);
    let line = "    println(m)";
    let idx_builtin_call = line.find("println").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function && s.start <= idx_builtin_call && idx_builtin_call < s.end
    }));
}

#[test]
fn test_highlight_go_label_name_as_attribute() {
    let src = "package main\nfunc f() {\nlabel:\n    goto label\n}\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.go"), &rope).expect("go syntax");

    let spans = doc.highlight_lines(&rope, 2, 3);
    let line = "label:";
    let idx_label_def = line.find("label").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Attribute
        && s.start <= idx_label_def
        && idx_label_def < s.end));

    let spans = doc.highlight_lines(&rope, 3, 4);
    let line = "    goto label";
    let idx_label_ref = line.rfind("label").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Attribute
        && s.start <= idx_label_ref
        && idx_label_ref < s.end));
}

#[test]
fn test_highlight_python_comment_string_keyword_and_in_string_or_comment() {
    let src = "# hi\ndef f():\n    return \"x\"\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.py"), &rope).expect("python syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "# hi";
    let idx = line.find('#').unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Comment && s.start <= idx && idx < s.end));

    let spans = doc.highlight_lines(&rope, 1, 2);
    let line = "def f():";
    let idx_def = line.find("def").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_def && idx_def < s.end));

    let spans = doc.highlight_lines(&rope, 2, 3);
    let line = "    return \"x\"";
    let idx_return = line.find("return").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_return && idx_return < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));

    let in_comment = src.find("# hi").unwrap() + 2;
    assert!(doc.is_in_string_or_comment(in_comment));
    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("def").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}

#[test]
fn test_highlight_python_richer_symbols() {
    let src = r#"class User:
    def greet(self, name: str) -> str:
        return f"hi {name}"

def use(value):
    user = User()
    result = user.greet(name=value)
    if (alias := result):
        return alias
    return result
"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.py"), &rope).expect("python syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "class User:";
    let idx_class_name = line.find("User").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Type
        && s.start <= idx_class_name
        && idx_class_name < s.end));

    let spans = doc.highlight_lines(&rope, 1, 2);
    let line = "    def greet(self, name: str) -> str:";
    let idx_method_name = line.find("greet").unwrap();
    let idx_self = line.find("self").unwrap();
    let idx_param_name = line.find("name").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function && s.start <= idx_method_name && idx_method_name < s.end
    }));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Variable && s.start <= idx_self && idx_self < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_param_name && idx_param_name < s.end
    }));

    let spans = doc.highlight_lines(&rope, 4, 5);
    let line = "def use(value):";
    let idx_function_name = line.find("use").unwrap();
    let idx_param_value = line.find("value").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function
            && s.start <= idx_function_name
            && idx_function_name < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_param_value && idx_param_value < s.end
    }));

    let spans = doc.highlight_lines(&rope, 5, 6);
    let line = "    user = User()";
    let idx_local_var = line.find("user").unwrap();
    let idx_constructor = line.find("User").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_local_var && idx_local_var < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Type && s.start <= idx_constructor && idx_constructor < s.end
    }));

    let spans = doc.highlight_lines(&rope, 6, 7);
    let line = "    result = user.greet(name=value)";
    let idx_result = line.find("result").unwrap();
    let idx_method_call = line.find("greet").unwrap();
    let idx_keyword_arg = line.find("name").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Variable && s.start <= idx_result && idx_result < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function && s.start <= idx_method_call && idx_method_call < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_keyword_arg && idx_keyword_arg < s.end
    }));

    let spans = doc.highlight_lines(&rope, 7, 8);
    let line = "    if (alias := result):";
    let idx_alias = line.find("alias").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Variable && s.start <= idx_alias && idx_alias < s.end));
}

#[test]
fn test_highlight_python_constant_and_regex_heuristics() {
    let src = r#"import re

MAX_RETRIES = 3
user = User()
matcher = re.compile(r"[A-Z_]+")
found = re.search("[0-9]+", text)
"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.py"), &rope).expect("python syntax");

    let spans = doc.highlight_lines(&rope, 2, 3);
    let line = "MAX_RETRIES = 3";
    let idx_constant = line.find("MAX_RETRIES").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Constant && s.start <= idx_constant && idx_constant < s.end
    }));

    let spans = doc.highlight_lines(&rope, 3, 4);
    let line = "user = User()";
    let idx_constructor = line.find("User").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Type && s.start <= idx_constructor && idx_constructor < s.end
    }));

    let spans = doc.highlight_lines(&rope, 4, 5);
    let line = "matcher = re.compile(r\"[A-Z_]+\")";
    let idx_regex = line.find("[A-Z_]+").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| { s.kind == HighlightKind::Regex && s.start <= idx_regex && idx_regex < s.end }));

    let spans = doc.highlight_lines(&rope, 5, 6);
    let line = "found = re.search(\"[0-9]+\", text)";
    let idx_regex = line.find("[0-9]+").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| { s.kind == HighlightKind::Regex && s.start <= idx_regex && idx_regex < s.end }));
}

#[test]
fn test_highlight_javascript_comment_string_keyword_and_in_string_or_comment() {
    let src = "function f() { return \"x\" } // hi\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.js"), &rope).expect("js syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "function f() { return \"x\" } // hi";
    let idx_func = line.find("function").unwrap();
    let idx_return = line.find("return").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    let idx_comment = line.find("//").unwrap();

    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_func && idx_func < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_return && idx_return < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Comment
        && s.start <= idx_comment
        && idx_comment < s.end));

    let in_comment = src.find("// hi").unwrap() + 3;
    assert!(doc.is_in_string_or_comment(in_comment));
    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("function").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}

#[test]
fn test_highlight_jsx_string_and_in_string_or_comment() {
    let src = "const x = <div>{\"x\"}</div>\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.jsx"), &rope).expect("jsx syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "const x = <div>{\"x\"}</div>";
    let idx_const = line.find("const").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;

    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_const && idx_const < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));

    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("const").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}

#[test]
fn test_highlight_tsx_string_and_in_string_or_comment() {
    let src = "const x = <div>{\"x\"}</div>\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.tsx"), &rope).expect("tsx syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "const x = <div>{\"x\"}</div>";
    let idx_const = line.find("const").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;

    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_const && idx_const < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));

    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("const").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}

#[test]
fn test_highlight_c_comment_string_keyword_and_in_string_or_comment() {
    let src = "int main() { const char* s = \"x\"; } // hi\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.c"), &rope).expect("c syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "int main() { const char* s = \"x\"; } // hi";
    let idx_const = line.find("const").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    let idx_comment = line.find("//").unwrap();

    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Keyword && s.start <= idx_const && idx_const < s.end
    }));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Comment && s.start <= idx_comment && idx_comment < s.end
    }));

    let in_comment = src.find("// hi").unwrap() + 3;
    assert!(doc.is_in_string_or_comment(in_comment));
    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
}

#[test]
fn test_highlight_cpp_comment_string_keyword_and_in_string_or_comment() {
    let src = "class A { public: bool ok = true; };\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.cpp"), &rope).expect("cpp syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "class A { public: bool ok = true; };";
    let idx_class = line.find("class").unwrap();
    let idx_public = line.find("public").unwrap();
    let idx_true = line.find("true").unwrap();

    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_class && idx_class < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Keyword && s.start <= idx_public && idx_public < s.end
    }));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_true && idx_true < s.end));
}

#[test]
fn test_highlight_header_defaults_to_cpp() {
    let src = "class A {};\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.h"), &rope).expect("h syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "class A {};";
    let idx_class = line.find("class").unwrap();

    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_class && idx_class < s.end));
}

#[test]
fn test_highlight_java_comment_string_keyword_and_in_string_or_comment() {
    let src = "public class A { String s = \"x\"; } // hi\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("Test.java"), &rope).expect("java syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "public class A { String s = \"x\"; } // hi";
    let idx_public = line.find("public").unwrap();
    let idx_class = line.find("class").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    let idx_comment = line.find("//").unwrap();

    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Keyword && s.start <= idx_public && idx_public < s.end
    }));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_class && idx_class < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Comment && s.start <= idx_comment && idx_comment < s.end
    }));

    let in_comment = src.find("// hi").unwrap() + 3;
    assert!(doc.is_in_string_or_comment(in_comment));
    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
}

#[test]
fn test_normalize_overlapping_highlight_spans_stable_tiebreak_by_seq() {
    let first = normalize_overlapping_highlight_spans(
        vec![
            AbsHighlightSpan {
                start: 3,
                end: 11,
                kind: HighlightKind::Keyword,
                depth: 4,
            },
            AbsHighlightSpan {
                start: 3,
                end: 11,
                kind: HighlightKind::String,
                depth: 4,
            },
        ],
        0,
        20,
    );
    assert_eq!(
        first,
        vec![AbsHighlightSpan {
            start: 3,
            end: 11,
            kind: HighlightKind::Keyword,
            depth: 4,
        }]
    );

    let second = normalize_overlapping_highlight_spans(
        vec![
            AbsHighlightSpan {
                start: 3,
                end: 11,
                kind: HighlightKind::String,
                depth: 4,
            },
            AbsHighlightSpan {
                start: 3,
                end: 11,
                kind: HighlightKind::Keyword,
                depth: 4,
            },
        ],
        0,
        20,
    );
    assert_eq!(
        second,
        vec![AbsHighlightSpan {
            start: 3,
            end: 11,
            kind: HighlightKind::String,
            depth: 4,
        }]
    );
}

#[test]
fn test_normalize_overlapping_highlight_spans_priority_uses_original_bounds() {
    let spans = normalize_overlapping_highlight_spans(
        vec![
            AbsHighlightSpan {
                start: 0,
                end: 3,
                kind: HighlightKind::Keyword,
                depth: 2,
            },
            AbsHighlightSpan {
                start: 2,
                end: 4,
                kind: HighlightKind::String,
                depth: 2,
            },
        ],
        2,
        3,
    );

    assert_eq!(
        spans,
        vec![AbsHighlightSpan {
            start: 2,
            end: 3,
            kind: HighlightKind::String,
            depth: 2,
        }]
    );
}

#[test]
fn test_highlight_line_spans_are_sorted_and_non_overlapping() {
    let src = r#"fn main() {
    let v = format!("item-{}", 42);
    tracing::info!("done {}", v);
}
"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("ordered.rs"), &rope).expect("rust syntax");

    let lines = doc.highlight_lines(&rope, 0, rope.len_lines());
    for (line_index, line) in lines.iter().enumerate() {
        let line_len = rope.line(line_index).len_bytes();
        for span in line {
            assert!(span.start < span.end);
            assert!(span.end <= line_len);
        }
        for pair in line.windows(2) {
            let left = pair[0];
            let right = pair[1];
            assert!(left.start <= right.start);
            assert!(left.end <= right.start);
        }
    }
}

#[test]
fn test_highlight_lines_cache_hit_returns_same_spans() {
    let src = "fn alpha() { let x = 1; }\nfn beta() { let y = 2; }\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("cache_hit.rs"), &rope).expect("rust syntax");

    let first = doc.highlight_lines(&rope, 0, 2);
    let second = doc.highlight_lines(&rope, 0, 2);
    assert_eq!(first, second);
}

#[test]
fn test_highlight_lines_shared_reuses_cached_window_for_same_range() {
    let src = "fn alpha() { let x = 1; }\nfn beta() { let y = 2; }\n";
    let rope = Rope::from_str(src);
    let doc =
        SyntaxDocument::for_path(Path::new("cache_hit_shared.rs"), &rope).expect("rust syntax");

    let first = doc.highlight_lines_shared(&rope, 0, 2);
    let second = doc.highlight_lines_shared(&rope, 0, 2);

    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn test_apply_edit_invalidates_highlight_cache_and_updates_result() {
    let src = "fn main() { let value = 1; }\n";
    let mut rope = Rope::from_str(src);
    let mut doc = SyntaxDocument::for_path(Path::new("cache_edit.rs"), &rope).expect("rust syntax");

    let before = doc.highlight_lines(&rope, 0, 1);
    assert!(!before[0]
        .iter()
        .any(|span| span.kind == HighlightKind::Comment));

    let insert_at = src.find("let").expect("insert position");
    let op = EditOp::insert(
        OpId::root(),
        insert_at,
        "// note ".to_string(),
        (0, 0),
        (0, 0),
    );
    op.apply(&mut rope);
    doc.apply_edit(&rope, &op);

    let after = doc.highlight_lines(&rope, 0, 1);
    assert!(after[0]
        .iter()
        .any(|span| span.kind == HighlightKind::Comment));
}

#[test]
fn test_large_file_range_highlight_preserves_sorted_non_overlapping_spans() {
    let lines = 21_000usize;
    let mut src = String::new();
    for i in 0..lines {
        src.push_str(&format!(
            "fn item_{i:05}() {{ let value_{i} = {i}; if value_{i} > 0 {{ println!(\"{}\", value_{i}); }} }}\n",
            i
        ));
    }

    let rope = Rope::from_str(&src);
    let doc = SyntaxDocument::for_path(Path::new("large_range.rs"), &rope).expect("rust syntax");
    let start = 10_000usize;
    let end = start + 220usize;

    let first = doc.highlight_lines(&rope, start, end);
    let second = doc.highlight_lines(&rope, start, end);
    assert_eq!(first, second);

    for (line_index, line) in first.iter().enumerate() {
        let line_len = rope.line(start + line_index).len_bytes();
        for span in line {
            assert!(span.start < span.end);
            assert!(span.end <= line_len);
        }
        for pair in line.windows(2) {
            assert!(pair[0].start <= pair[1].start);
            assert!(pair[0].end <= pair[1].start);
        }
    }
}

#[test]
fn experiment_highlight_lines_scale_baseline() {
    let lines = 1400usize;
    let window = 220usize;
    let loops = 100usize;
    let mut src = String::new();
    for i in 0..lines {
        src.push_str(&format!(
            "fn item_{i:04}(x: i32) -> i32 {{ let s = \"v{i}\"; if x > {i} {{ x + {i} }} else {{ x - {i} }} }}\n"
        ));
    }
    let rope = Rope::from_str(&src);
    let doc = SyntaxDocument::for_path(Path::new("baseline.rs"), &rope).expect("rust syntax");

    let _ = doc.highlight_lines(&rope, 0, window);

    let mut total_spans = 0usize;
    let start = Instant::now();
    for i in 0..loops {
        let start_line = (i * 17) % (lines - window);
        let spans = doc.highlight_lines(&rope, start_line, start_line + window);
        total_spans += spans.iter().map(Vec::len).sum::<usize>();
    }
    let elapsed = start.elapsed();
    let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / loops as f64;

    eprintln!(
        "[experiment] syntax_highlight_lines loops={} lines={} window={} elapsed_ms={} avg_us={:.2} total_spans={}",
        loops,
        lines,
        window,
        elapsed.as_millis(),
        avg_us,
        total_spans
    );

    assert!(total_spans > 0);
}

#[test]
fn test_highlight_json_string_number_keyword() {
    let src = r#"{"name": "zcode", "version": 1, "active": true, "data": null}"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.json"), &rope).expect("json syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    assert!(!spans[0].is_empty());

    let idx_name_key = src.find("\"name\"").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::String
        && s.start <= idx_name_key
        && idx_name_key < s.end));

    let idx_true = src.find("true").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_true && idx_true < s.end));

    let idx_null = src.find("null").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_null && idx_null < s.end));
}

#[test]
fn test_highlight_yaml_string_keyword() {
    let src = "name: zcode\nactive: true\ncount: 42\n# comment\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.yaml"), &rope).expect("yaml syntax");

    let spans = doc.highlight_lines(&rope, 0, rope.len_lines());
    let all_spans: usize = spans.iter().map(Vec::len).sum();
    assert!(all_spans > 0);

    let comment_line = 3;
    let idx_comment = 0;
    assert!(spans[comment_line]
        .iter()
        .any(|s| s.kind == HighlightKind::Comment
            && s.start <= idx_comment
            && idx_comment < s.end));
}

#[test]
fn test_highlight_html_tags_and_attributes() {
    let src = r#"<div class="main"><p>Hello</p></div>"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.html"), &rope).expect("html syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    assert!(!spans[0].is_empty());

    let idx_div = src.find("div").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_div && idx_div < s.end));

    let idx_class = src.find("class").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Attribute && s.start <= idx_class && idx_class < s.end));

    let idx_attr_val = src.find("\"main\"").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::String
        && s.start <= idx_attr_val
        && idx_attr_val < s.end));
}

#[test]
fn test_highlight_xml_tags_and_attributes() {
    let src = r#"<root attr="val"><child/></root>"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.xml"), &rope).expect("xml syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    assert!(!spans[0].is_empty());
}

#[test]
fn test_highlight_xml_text_content_is_not_keyword() {
    let src = "<root>hello</root>";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.xml"), &rope).expect("xml syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let idx_text = src.find("hello").expect("text content");
    assert!(!spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_text && idx_text < s.end));

    let snippet_spans = highlight_snippet(LanguageId::Xml, src);
    assert_eq!(snippet_spans.len(), 1);
    assert!(!snippet_spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_text && idx_text < s.end));
}

#[test]
fn test_highlight_css_selectors_and_properties() {
    let src = ".main { color: red; font-size: 14px; }\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.css"), &rope).expect("css syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    assert!(!spans[0].is_empty());

    let idx_class = src.find(".main").unwrap() + 1;
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Type && s.start <= idx_class && idx_class < s.end));

    let idx_color = src.find("color").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Variable && s.start <= idx_color && idx_color < s.end));
}

#[test]
fn test_highlight_toml_string_and_number() {
    let src = "[package]\nname = \"zcode\"\nversion = 1\nenabled = true\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.toml"), &rope).expect("toml syntax");

    let spans = doc.highlight_lines(&rope, 0, rope.len_lines());
    let all_spans: usize = spans.iter().map(Vec::len).sum();
    assert!(all_spans > 0);

    let line1_spans = &spans[1];
    let line = "name = \"zcode\"";
    let idx_str = line.find("\"zcode\"").unwrap();
    assert!(line1_spans
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));
}

#[test]
fn test_highlight_sql_comment_string_number_and_keyword() {
    let src =
        "-- list active users\nSELECT name, age FROM users WHERE id = 42 AND status = 'ok';\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.sql"), &rope).expect("sql syntax");

    let spans = doc.highlight_lines(&rope, 0, rope.len_lines());
    let all_spans: usize = spans.iter().map(Vec::len).sum();
    assert!(all_spans > 0);

    let idx_comment = 0;
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Comment
        && s.start <= idx_comment
        && idx_comment < s.end));

    let line = "SELECT name, age FROM users WHERE id = 42 AND status = 'ok';";
    let idx_keyword = line.find("SELECT").unwrap();
    let idx_number = line.find("42").unwrap();
    let idx_string = line.find("'ok'").unwrap() + 1;
    assert!(spans[1].iter().any(|s| s.kind == HighlightKind::Keyword
        && s.start <= idx_keyword
        && idx_keyword < s.end));
    assert!(spans[1]
        .iter()
        .any(|s| s.kind == HighlightKind::Number && s.start <= idx_number && idx_number < s.end));
    assert!(spans[1]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_string && idx_string < s.end));

    let snippet_spans = highlight_snippet(LanguageId::Sql, "SELECT 1;");
    assert_eq!(snippet_spans.len(), 1);
    assert!(snippet_spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start == 0));
    assert!(snippet_spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Number && s.start <= 7 && 7 < s.end));
}

#[test]
fn test_highlight_sql_create_table_ddl_does_not_degrade_on_constraints() {
    let src = "CREATE TABLE IF NOT EXISTS auth_account (\n  id UUID PRIMARY KEY,\n  user_id UUID NOT NULL UNIQUE,\n  login_name VARCHAR(64) NOT NULL UNIQUE,\n  email VARCHAR(128) NOT NULL UNIQUE,\n  password_hash VARCHAR(255) NOT NULL,\n  password_algo VARCHAR(32) NOT NULL,\n  status SMALLINT NOT NULL,\n  failed_login_count INT NOT NULL DEFAULT 0,\n  locked_until TIMESTAMPTZ NULL,\n  last_login_at TIMESTAMPTZ NULL,\n  created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,\n  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,\n  CONSTRAINT chk_auth_account_status CHECK (status IN (1, 2, 3))\n);\n\nCREATE INDEX IF NOT EXISTS idx_auth_account_status\n  ON auth_account(status);\n\nCREATE TABLE IF NOT EXISTS auth_refresh_token (\n  id UUID PRIMARY KEY,\n  account_id UUID NOT NULL REFERENCES auth_account(id),\n  token_hash VARCHAR(128) NOT NULL UNIQUE,\n  client_id VARCHAR(64) NULL,\n  expires_at TIMESTAMPTZ NOT NULL,\n  revoked_at TIMESTAMPTZ NULL,\n  created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP\n);\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("ddl.sql"), &rope).expect("sql syntax");
    let lines: Vec<&str> = src.lines().collect();
    let spans = doc.highlight_lines(&rope, 0, lines.len());

    let assert_kind = |line_index: usize, token: &str, expected: HighlightKind| {
        let line = lines[line_index];
        let idx = line.find(token).expect("token exists in line");
        assert!(
            spans[line_index]
                .iter()
                .any(|s| s.kind == expected && s.start <= idx && idx < s.end),
            "expected {:?} at token `{}` in line {}: `{}`; spans: {:?}",
            expected,
            token,
            line_index,
            line,
            spans[line_index]
        );
    };

    let assert_not_keyword = |line_index: usize, token: &str| {
        let line = lines[line_index];
        let idx = line.find(token).expect("token exists in line");
        assert!(
            !spans[line_index]
                .iter()
                .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx && idx < s.end),
            "token `{}` in line {} should not be highlighted as keyword; line: `{}`; spans: {:?}",
            token,
            line_index,
            line,
            spans[line_index]
        );
    };

    assert_kind(2, "UNIQUE", HighlightKind::Keyword);
    assert_kind(3, "VARCHAR", HighlightKind::Type);
    assert_kind(3, "64", HighlightKind::Number);
    assert_kind(8, "0", HighlightKind::Number);
    assert_kind(9, "TIMESTAMPTZ", HighlightKind::Type);
    assert_kind(11, "TIMESTAMPTZ", HighlightKind::Type);
    assert_kind(16, "IF", HighlightKind::Keyword);

    assert_not_keyword(17, "auth_account");
    assert_not_keyword(19, "auth_refresh_token");
    assert_not_keyword(21, "auth_account");
}

#[test]
fn test_highlight_bash_commands_and_keywords() {
    let src = "#!/bin/bash\nif [ -f file ]; then\n  echo \"hello\"\nfi\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.sh"), &rope).expect("bash syntax");

    let spans = doc.highlight_lines(&rope, 0, rope.len_lines());
    let all_spans: usize = spans.iter().map(Vec::len).sum();
    assert!(all_spans > 0);

    let line2 = "  echo \"hello\"";
    let idx_str = line2.find("\"hello\"").unwrap();
    assert!(spans[2]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));
}
