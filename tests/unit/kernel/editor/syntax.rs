use super::*;
use crate::models::{EditOp, OpId};
use compact_str::CompactString;
use ropey::Rope;
use std::path::Path;
use std::time::Instant;

fn highlight_lines(
    doc: &SyntaxDocument,
    rope: &Rope,
    start_line: usize,
    end_line_exclusive: usize,
) -> Vec<Vec<HighlightSpan>> {
    compute_highlight_patches(
        doc.language(),
        doc.tree(),
        rope,
        &[(start_line, end_line_exclusive)],
    )
    .into_iter()
    .next()
    .map(|p| p.lines)
    .unwrap_or_default()
}

#[test]
fn highlight_kind_is_leaf_matches_expected() {
    assert!(HighlightKind::Comment.is_leaf());
    assert!(HighlightKind::String.is_leaf());
    assert!(HighlightKind::Regex.is_leaf());
    assert!(HighlightKind::Attribute.is_leaf());

    assert!(!HighlightKind::Keyword.is_leaf());
    assert!(!HighlightKind::KeywordControl.is_leaf());
    assert!(!HighlightKind::KeywordOperator.is_leaf());
    assert!(!HighlightKind::Type.is_leaf());
    assert!(!HighlightKind::TypeBuiltin.is_leaf());
    assert!(!HighlightKind::Number.is_leaf());
    assert!(!HighlightKind::Boolean.is_leaf());
    assert!(!HighlightKind::Lifetime.is_leaf());
    assert!(!HighlightKind::Function.is_leaf());
    assert!(!HighlightKind::Method.is_leaf());
    assert!(!HighlightKind::Macro.is_leaf());
    assert!(!HighlightKind::Namespace.is_leaf());
    assert!(!HighlightKind::Variable.is_leaf());
    assert!(!HighlightKind::Parameter.is_leaf());
    assert!(!HighlightKind::Property.is_leaf());
    assert!(!HighlightKind::Constant.is_leaf());
    assert!(!HighlightKind::EnumMember.is_leaf());
    assert!(!HighlightKind::Operator.is_leaf());
    assert!(!HighlightKind::Tag.is_leaf());
    assert!(!HighlightKind::TagAttribute.is_leaf());
}

#[test]
fn highlight_kind_is_opaque_matches_expected() {
    assert!(HighlightKind::Comment.is_opaque());
    assert!(HighlightKind::String.is_opaque());
    assert!(HighlightKind::Regex.is_opaque());

    assert!(!HighlightKind::Attribute.is_opaque());
    assert!(!HighlightKind::Keyword.is_opaque());
    assert!(HighlightKind::KeywordControl.is_opaque());
    assert!(!HighlightKind::KeywordOperator.is_opaque());
    assert!(!HighlightKind::Type.is_opaque());
    assert!(!HighlightKind::TypeBuiltin.is_opaque());
    assert!(!HighlightKind::Number.is_opaque());
    assert!(!HighlightKind::Boolean.is_opaque());
    assert!(!HighlightKind::Lifetime.is_opaque());
    assert!(!HighlightKind::Function.is_opaque());
    assert!(!HighlightKind::Method.is_opaque());
    assert!(!HighlightKind::Macro.is_opaque());
    assert!(!HighlightKind::Namespace.is_opaque());
    assert!(!HighlightKind::Variable.is_opaque());
    assert!(!HighlightKind::Parameter.is_opaque());
    assert!(!HighlightKind::Property.is_opaque());
    assert!(!HighlightKind::Constant.is_opaque());
    assert!(!HighlightKind::EnumMember.is_opaque());
    assert!(!HighlightKind::Operator.is_opaque());
    assert!(!HighlightKind::Tag.is_opaque());
    assert!(!HighlightKind::TagAttribute.is_opaque());
}

#[test]
fn test_highlight_comment_range_rust() {
    let rope = Rope::from_str("fn main() { // hi\n}\n");
    let doc = SyntaxDocument::for_path(Path::new("test.rs"), &rope).expect("rust syntax");

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let spans = highlight_lines(&doc, &rope, 1, 2);
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
fn test_highlight_rust_richer_symbols() {
    let src = r#"struct User { name: String }
impl User {
    fn greet(&self, other: &str) -> String {
        self.name.clone()
    }
}

fn main() {
    let user = User { name: String::from("x") };
    user.greet("y");
}
"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.rs"), &rope).expect("rust syntax");

    let spans = highlight_lines(&doc, &rope, 0, 1);
    let line = "struct User { name: String }";
    let idx_field_name = line.find("name").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Property && s.start <= idx_field_name && idx_field_name < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 2, 3);
    let line = "    fn greet(&self, other: &str) -> String {";
    let idx_param_other = line.find("other").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Parameter && s.start <= idx_param_other && idx_param_other < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 3, 4);
    let line = "        self.name.clone()";
    let idx_prop_name = line.find("name").unwrap();
    let idx_method_clone = line.find("clone").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Property && s.start <= idx_prop_name && idx_prop_name < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Method && s.start <= idx_method_clone && idx_method_clone < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 9, 10);
    let line = "    user.greet(\"y\");";
    let idx_method_call = line.find("greet").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Method && s.start <= idx_method_call && idx_method_call < s.end
    }));
}

#[test]
fn test_highlight_go_comment_string_keyword_and_in_string_or_comment() {
    let src = "package main\n// hi\nfunc main() { println(\"x\") }\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.go"), &rope).expect("go syntax");

    let spans = highlight_lines(&doc, &rope, 1, 2);
    let line = "// hi";
    let idx = line.find("//").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Comment && s.start <= idx && idx < s.end));

    let spans = highlight_lines(&doc, &rope, 2, 3);
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
    let line = "package main";
    let idx_package_name = line.find("main").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Attribute
            && s.start <= idx_package_name
            && idx_package_name < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 1, 2);
    let line = "type Counter struct { value int }";
    let idx_type_name = line.find("Counter").unwrap();
    let idx_field_name = line.find("value").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Type
        && s.start <= idx_type_name
        && idx_type_name < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Property && s.start <= idx_field_name && idx_field_name < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 2, 3);
    let line = "func add(x int, y int) int { return x + y }";
    let idx_function_name = line.find("add").unwrap();
    let idx_param_x = line.find("x").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function
            && s.start <= idx_function_name
            && idx_function_name < s.end
    }));
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Parameter
        && s.start <= idx_param_x
        && idx_param_x < s.end));

    let spans = highlight_lines(&doc, &rope, 3, 4);
    let line = "func (c *Counter) Inc(delta int) int { return c.value + delta }";
    let idx_method_name = line.find("Inc").unwrap();
    let idx_param_delta = line.find("delta").unwrap();
    let idx_member_value = line.find("value").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function && s.start <= idx_method_name && idx_method_name < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Parameter && s.start <= idx_param_delta && idx_param_delta < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Property && s.start <= idx_member_value && idx_member_value < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 4, 5);
    let line = "func Map[T any](x T) T { return x }";
    let idx_type_param = line.find("[T").unwrap() + 1;
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Type
        && s.start <= idx_type_param
        && idx_type_param < s.end));

    let spans = highlight_lines(&doc, &rope, 8, 9);
    let line = "    m := c.Inc(n)";
    let idx_call_method = line.find("Inc").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Method && s.start <= idx_call_method && idx_call_method < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 9, 10);
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

    let spans = highlight_lines(&doc, &rope, 2, 3);
    let line = "label:";
    let idx_label_def = line.find("label").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Attribute
        && s.start <= idx_label_def
        && idx_label_def < s.end));

    let spans = highlight_lines(&doc, &rope, 3, 4);
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
    let line = "# hi";
    let idx = line.find('#').unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Comment && s.start <= idx && idx < s.end));

    let spans = highlight_lines(&doc, &rope, 1, 2);
    let line = "def f():";
    let idx_def = line.find("def").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_def && idx_def < s.end));

    let spans = highlight_lines(&doc, &rope, 2, 3);
    let line = "    return \"x\"";
    let idx_return = line.find("return").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::KeywordControl
            && s.start <= idx_return
            && idx_return < s.end));
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
    let line = "class User:";
    let idx_class_name = line.find("User").unwrap();
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Type
        && s.start <= idx_class_name
        && idx_class_name < s.end));

    let spans = highlight_lines(&doc, &rope, 1, 2);
    let line = "    def greet(self, name: str) -> str:";
    let idx_method_name = line.find("greet").unwrap();
    let idx_self = line.find("self").unwrap();
    let idx_param_name = line.find("name").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function && s.start <= idx_method_name && idx_method_name < s.end
    }));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Parameter && s.start <= idx_self && idx_self < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Parameter && s.start <= idx_param_name && idx_param_name < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 4, 5);
    let line = "def use(value):";
    let idx_function_name = line.find("use").unwrap();
    let idx_param_value = line.find("value").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Function
            && s.start <= idx_function_name
            && idx_function_name < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Parameter && s.start <= idx_param_value && idx_param_value < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 5, 6);
    let line = "    user = User()";
    let idx_local_var = line.find("user").unwrap();
    let idx_constructor = line.find("User").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_local_var && idx_local_var < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Type && s.start <= idx_constructor && idx_constructor < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 6, 7);
    let line = "    result = user.greet(name=value)";
    let idx_result = line.find("result").unwrap();
    let idx_method_call = line.find("greet").unwrap();
    let idx_keyword_arg = line.find("name").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Variable && s.start <= idx_result && idx_result < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Method && s.start <= idx_method_call && idx_method_call < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Variable && s.start <= idx_keyword_arg && idx_keyword_arg < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 7, 8);
    let line = "    if (alias := result):";
    let idx_if = line.find("if").unwrap();
    let idx_alias = line.find("alias").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::KeywordControl && s.start <= idx_if && idx_if < s.end
    }));
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

    let spans = highlight_lines(&doc, &rope, 2, 3);
    let line = "MAX_RETRIES = 3";
    let idx_constant = line.find("MAX_RETRIES").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Constant && s.start <= idx_constant && idx_constant < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 3, 4);
    let line = "user = User()";
    let idx_constructor = line.find("User").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Type && s.start <= idx_constructor && idx_constructor < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 4, 5);
    let line = "matcher = re.compile(r\"[A-Z_]+\")";
    let idx_regex = line.find("[A-Z_]+").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| { s.kind == HighlightKind::Regex && s.start <= idx_regex && idx_regex < s.end }));

    let spans = highlight_lines(&doc, &rope, 5, 6);
    let line = "found = re.search(\"[0-9]+\", text)";
    let idx_regex = line.find("[0-9]+").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| { s.kind == HighlightKind::Regex && s.start <= idx_regex && idx_regex < s.end }));
}

#[test]
fn test_highlight_python_boolean_keyword_operator_and_builtin_types() {
    let src = r#"def f(value: int) -> bool:
    return True and (value in (1, 2)) and (value is not None)
    return False
"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.py"), &rope).expect("python syntax");

    let spans = highlight_lines(&doc, &rope, 0, 1);
    let line = "def f(value: int) -> bool:";
    let idx_int = line.find("int").unwrap();
    let idx_bool = line.find("bool").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::TypeBuiltin && s.start <= idx_int && idx_int < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::TypeBuiltin && s.start <= idx_bool && idx_bool < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 1, 2);
    let line = "    return True and (value in (1, 2)) and (value is not None)";
    let idx_true = line.find("True").unwrap();
    let idx_and = line.find("and").unwrap();
    let idx_in = line.find("in").unwrap();
    let idx_is = line.rfind("is").unwrap();
    let idx_not = line.rfind("not").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Boolean && s.start <= idx_true && idx_true < s.end));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::KeywordOperator && s.start <= idx_and && idx_and < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::KeywordOperator && s.start <= idx_in && idx_in < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::KeywordOperator && s.start <= idx_is && idx_is < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::KeywordOperator && s.start <= idx_not && idx_not < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 2, 3);
    let line = "    return False";
    let idx_false = line.find("False").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Boolean && s.start <= idx_false && idx_false < s.end
    }));
}

#[test]
fn test_highlight_javascript_comment_string_keyword_and_in_string_or_comment() {
    let src = "function f() { return \"x\" } // hi\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.js"), &rope).expect("js syntax");

    let spans = highlight_lines(&doc, &rope, 0, 1);
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
        .any(|s| s.kind == HighlightKind::KeywordControl
            && s.start <= idx_return
            && idx_return < s.end));
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
fn test_highlight_javascript_richer_symbols() {
    let src = r#"class Counter {
  inc(delta) { return this.value + delta; }
}
const c = new Counter();
c.inc(2);
c.value;
"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.js"), &rope).expect("js syntax");

    let spans = highlight_lines(&doc, &rope, 1, 2);
    let line = "  inc(delta) { return this.value + delta; }";
    let idx_param_delta = line.find("delta").unwrap();
    let idx_prop_value = line.find("value").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Parameter && s.start <= idx_param_delta && idx_param_delta < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Property && s.start <= idx_prop_value && idx_prop_value < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 4, 5);
    let line = "c.inc(2);";
    let idx_method_call = line.find("inc").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Method && s.start <= idx_method_call && idx_method_call < s.end
    }));

    let spans = highlight_lines(&doc, &rope, 5, 6);
    let line = "c.value;";
    let idx_prop_access = line.find("value").unwrap();
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::Property && s.start <= idx_prop_access && idx_prop_access < s.end
    }));
}

#[test]
fn test_highlight_jsx_string_and_in_string_or_comment() {
    let src = "const x = <div>{\"x\"}</div>\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.jsx"), &rope).expect("jsx syntax");

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
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
fn test_highlight_c_keyword_control() {
    let src = "int main() { if (1) { return 0; } goto done; done: return 1; }\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.c"), &rope).expect("c syntax");

    let spans = highlight_lines(&doc, &rope, 0, 1);
    let line = "int main() { if (1) { return 0; } goto done; done: return 1; }";
    let idx_if = line.find("if").unwrap();
    let idx_return = line.find("return").unwrap();
    let idx_goto = line.find("goto").unwrap();

    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::KeywordControl && s.start <= idx_if && idx_if < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::KeywordControl && s.start <= idx_return && idx_return < s.end
    }));
    assert!(spans[0].iter().any(|s| {
        s.kind == HighlightKind::KeywordControl && s.start <= idx_goto && idx_goto < s.end
    }));
}

#[test]
fn test_highlight_cpp_comment_string_keyword_and_in_string_or_comment() {
    let src = "class A { public: bool ok = true; };\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.cpp"), &rope).expect("cpp syntax");

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let lines = highlight_lines(&doc, &rope, 0, rope.len_lines());
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

    let first = highlight_lines(&doc, &rope, 0, 2);
    let second = highlight_lines(&doc, &rope, 0, 2);
    assert_eq!(first, second);
}

#[test]
fn test_apply_edit_invalidates_highlight_cache_and_updates_result() {
    let src = "fn main() { let value = 1; }\n";
    let mut rope = Rope::from_str(src);
    let mut doc = SyntaxDocument::for_path(Path::new("cache_edit.rs"), &rope).expect("rust syntax");

    let before = highlight_lines(&doc, &rope, 0, 1);
    assert!(!before[0]
        .iter()
        .any(|span| span.kind == HighlightKind::Comment));

    let insert_at = src.find("let").expect("insert position");
    let op = EditOp::insert(
        OpId::root(),
        insert_at,
        CompactString::new("// note "),
        (0, 0),
        (0, 0),
    );
    op.apply(&mut rope);
    let delta = doc.apply_edit(&rope, &op);
    assert!(!delta.reparsed);
    assert!(delta.input_edit.is_some());
    assert!(!delta.changed_ranges.is_empty());

    let after = highlight_lines(&doc, &rope, 0, 1);
    assert!(after[0]
        .iter()
        .any(|span| span.kind == HighlightKind::Comment));
}

#[test]
fn test_apply_edit_batch_falls_back_to_reparse() {
    let src = "fn main() { let value = 1; }\n";
    let mut rope = Rope::from_str(src);
    let mut doc =
        SyntaxDocument::for_path(Path::new("cache_edit_batch.rs"), &rope).expect("rust syntax");

    let insert_at = src.find("let").expect("insert position");
    let op = EditOp {
        id: OpId::new(),
        parent: OpId::root(),
        kind: crate::models::OpKind::Batch {
            edits: vec![crate::models::edit_op::BatchEdit {
                start: insert_at,
                end: insert_at,
                deleted: CompactString::new(""),
                inserted: CompactString::new("/* note */ "),
            }],
        },
        cursor_before: (0, 0),
        cursor_after: (0, 0),
        extra_cursors_before: None,
        extra_cursors_after: None,
    };

    op.apply(&mut rope);
    let delta = doc.apply_edit(&rope, &op);
    assert!(delta.reparsed);
    assert!(delta.input_edit.is_none());
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

    let first = highlight_lines(&doc, &rope, start, end);
    let second = highlight_lines(&doc, &rope, start, end);
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
    let loops = 20usize;
    let mut src = String::new();
    for i in 0..lines {
        src.push_str(&format!(
            "fn item_{i:04}(x: i32) -> i32 {{ let s = \"v{i}\"; if x > {i} {{ x + {i} }} else {{ x - {i} }} }}\n"
        ));
    }
    let rope = Rope::from_str(&src);
    let doc = SyntaxDocument::for_path(Path::new("baseline.rs"), &rope).expect("rust syntax");

    let _ = highlight_lines(&doc, &rope, 0, window);

    let mut total_spans = 0usize;
    let start = Instant::now();
    for i in 0..loops {
        let start_line = (i * 17) % (lines - window);
        let spans = highlight_lines(&doc, &rope, start_line, start_line + window);
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let spans = highlight_lines(&doc, &rope, 0, rope.len_lines());
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
    assert!(!spans[0].is_empty());

    let idx_div = src.find("div").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Tag && s.start <= idx_div && idx_div < s.end));

    let idx_class = src.find("class").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::TagAttribute
            && s.start <= idx_class
            && idx_class < s.end));

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

    let spans = highlight_lines(&doc, &rope, 0, 1);
    assert!(!spans[0].is_empty());
}

#[test]
fn test_highlight_xml_text_content_is_not_keyword() {
    let src = "<root>hello</root>";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.xml"), &rope).expect("xml syntax");

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let spans = highlight_lines(&doc, &rope, 0, 1);
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

    let spans = highlight_lines(&doc, &rope, 0, rope.len_lines());
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

    let spans = highlight_lines(&doc, &rope, 0, rope.len_lines());
    let all_spans: usize = spans.iter().map(Vec::len).sum();
    assert!(all_spans > 0);

    let idx_comment = 0;
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Comment
        && s.start <= idx_comment
        && idx_comment < s.end));

    let line = "SELECT name, age FROM users WHERE id = 42 AND status = 'ok';";
    let idx_keyword = line.find("SELECT").unwrap();
    let idx_where = line.find("WHERE").unwrap();
    let idx_number = line.find("42").unwrap();
    let idx_string = line.find("'ok'").unwrap() + 1;
    assert!(spans[1].iter().any(|s| s.kind == HighlightKind::Keyword
        && s.start <= idx_keyword
        && idx_keyword < s.end));
    assert!(spans[1].iter().any(|s| {
        s.kind == HighlightKind::KeywordControl && s.start <= idx_where && idx_where < s.end
    }));
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
    let spans = highlight_lines(&doc, &rope, 0, lines.len());

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

    let spans = highlight_lines(&doc, &rope, 0, rope.len_lines());
    let all_spans: usize = spans.iter().map(Vec::len).sum();
    assert!(all_spans > 0);

    let line2 = "  echo \"hello\"";
    let idx_str = line2.find("\"hello\"").unwrap();
    assert!(spans[2]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));
}

#[test]
fn test_highlight_go_operators_are_not_classified() {
    let src = r#"package main

func f(a, b, c, d, e, y int) {
    if a == b || a != c && d == e {
        x := y
        x = 1
    }
}
"#;

    let spans = highlight_snippet(LanguageId::Go, src);
    let lines: Vec<&str> = src.lines().collect();

    let assert_no_span_at = |line: usize, needle: &str| {
        let idx = lines[line].find(needle).expect("needle must exist");
        for off in 0..needle.len() {
            let pos = idx + off;
            assert!(
                !spans[line].iter().any(|s| s.start <= pos && pos < s.end),
                "operator `{}` in line {} should not be highlighted; spans: {:?}",
                needle,
                line,
                spans[line]
            );
        }
    };

    // if a == b || a != c && d == e {
    assert_no_span_at(3, "==");
    assert_no_span_at(3, "||");
    assert_no_span_at(3, "!=");
    assert_no_span_at(3, "&&");

    // x := y
    assert_no_span_at(4, ":=");

    // x = 1
    assert_no_span_at(5, "=");
}

#[test]
fn test_go_import_paths_are_strings() {
    let src = r#"package main

import (
	"log/slog"
	"net/http"
	"strings"
)

func main() {}
"#;
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.go"), &rope).expect("go syntax");
    let spans = highlight_lines(&doc, &rope, 0, rope.len_lines());

    // Helper: find the byte range of `needle` in `line_str`
    let find_in_line = |line: usize, needle: &str| -> (usize, usize) {
        let line_str = rope.line(line).to_string();
        let start = line_str
            .find(needle)
            .unwrap_or_else(|| panic!("'{}' not found in line {}", needle, line));
        (start, start + needle.len())
    };

    // Line 3: \t"log/slog"
    let (start, end) = find_in_line(3, "\"log/slog\"");
    let line_spans = &spans[3];
    // The entire import path including quotes should be a single String span
    let covering = line_spans.iter().find(|s| s.start <= start && s.end >= end);
    assert!(
        covering.is_some_and(|s| s.kind == HighlightKind::String),
        "\"log/slog\" should be a single String span, got: {:?}",
        line_spans
    );

    // Line 4: \t"net/http"
    let (start, end) = find_in_line(4, "\"net/http\"");
    let covering = spans[4].iter().find(|s| s.start <= start && s.end >= end);
    assert!(
        covering.is_some_and(|s| s.kind == HighlightKind::String),
        "\"net/http\" should be a single String span, got: {:?}",
        spans[4]
    );

    // Line 5: \t"strings"
    let (start, end) = find_in_line(5, "\"strings\"");
    let covering = spans[5].iter().find(|s| s.start <= start && s.end >= end);
    assert!(
        covering.is_some_and(|s| s.kind == HighlightKind::String),
        "\"strings\" should be a single String span, got: {:?}",
        spans[5]
    );

    // Line 2: "import" should be KeywordControl
    let (start, _end) = find_in_line(2, "import");
    let covering = spans[2].iter().find(|s| s.start <= start && start < s.end);
    assert!(
        covering.is_some_and(|s| s.kind == HighlightKind::KeywordControl),
        "import should be KeywordControl, got: {:?}",
        spans[2]
    );
}
