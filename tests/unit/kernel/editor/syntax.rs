use super::*;
use ropey::Rope;
use std::path::Path;

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
