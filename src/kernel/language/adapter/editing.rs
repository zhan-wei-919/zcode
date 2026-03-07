#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelimiterRule {
    pub open: char,
    pub close: char,
    pub auto_pair: bool,
    pub electric_enter: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineSuffixIndentRule {
    pub suffix: &'static str,
    pub extra_levels: u8,
}

pub trait LanguageEditingPolicy: Send + Sync {
    fn delimiter_rules(&self) -> &'static [DelimiterRule] {
        &[]
    }

    fn newline_indent_rules(&self) -> &'static [LineSuffixIndentRule] {
        &[]
    }

    fn auto_pair_closing_for(&self, open: char) -> Option<char> {
        self.delimiter_rules()
            .iter()
            .find(|rule| rule.auto_pair && rule.open == open)
            .map(|rule| rule.close)
    }

    fn newline_indent_extra_levels(&self, trimmed_before_cursor: &str) -> u8 {
        self.newline_indent_rules()
            .iter()
            .filter(|rule| trimmed_before_cursor.ends_with(rule.suffix))
            .map(|rule| rule.extra_levels)
            .max()
            .unwrap_or(0)
    }
}

pub(crate) struct StaticLanguageEditingPolicy {
    delimiter_rules: &'static [DelimiterRule],
    newline_indent_rules: &'static [LineSuffixIndentRule],
}

impl StaticLanguageEditingPolicy {
    pub(crate) const fn new(
        delimiter_rules: &'static [DelimiterRule],
        newline_indent_rules: &'static [LineSuffixIndentRule],
    ) -> Self {
        Self {
            delimiter_rules,
            newline_indent_rules,
        }
    }
}

impl LanguageEditingPolicy for StaticLanguageEditingPolicy {
    fn delimiter_rules(&self) -> &'static [DelimiterRule] {
        self.delimiter_rules
    }

    fn newline_indent_rules(&self) -> &'static [LineSuffixIndentRule] {
        self.newline_indent_rules
    }
}

const BRACE_LANGUAGE_DELIMITER_RULES: [DelimiterRule; 5] = [
    DelimiterRule {
        open: '{',
        close: '}',
        auto_pair: true,
        electric_enter: true,
    },
    DelimiterRule {
        open: '(',
        close: ')',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '[',
        close: ']',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '"',
        close: '"',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '\'',
        close: '\'',
        auto_pair: true,
        electric_enter: false,
    },
];

const RUST_DELIMITER_RULES: [DelimiterRule; 5] = [
    DelimiterRule {
        open: '{',
        close: '}',
        auto_pair: true,
        electric_enter: true,
    },
    DelimiterRule {
        open: '(',
        close: ')',
        auto_pair: true,
        electric_enter: true,
    },
    DelimiterRule {
        open: '[',
        close: ']',
        auto_pair: true,
        electric_enter: true,
    },
    DelimiterRule {
        open: '"',
        close: '"',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '\'',
        close: '\'',
        auto_pair: true,
        electric_enter: false,
    },
];

const GO_DELIMITER_RULES: [DelimiterRule; 5] = [
    DelimiterRule {
        open: '{',
        close: '}',
        auto_pair: true,
        electric_enter: true,
    },
    DelimiterRule {
        open: '(',
        close: ')',
        auto_pair: true,
        electric_enter: true,
    },
    DelimiterRule {
        open: '[',
        close: ']',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '"',
        close: '"',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '\'',
        close: '\'',
        auto_pair: true,
        electric_enter: false,
    },
];

const PYTHON_DELIMITER_RULES: [DelimiterRule; 5] = [
    DelimiterRule {
        open: '{',
        close: '}',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '(',
        close: ')',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '[',
        close: ']',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '"',
        close: '"',
        auto_pair: true,
        electric_enter: false,
    },
    DelimiterRule {
        open: '\'',
        close: '\'',
        auto_pair: true,
        electric_enter: false,
    },
];

const BRACE_NEWLINE_RULES: [LineSuffixIndentRule; 1] = [LineSuffixIndentRule {
    suffix: "{",
    extra_levels: 1,
}];

const PYTHON_NEWLINE_RULES: [LineSuffixIndentRule; 1] = [LineSuffixIndentRule {
    suffix: ":",
    extra_levels: 1,
}];

pub(crate) static DEFAULT_EDITING_POLICY: StaticLanguageEditingPolicy =
    StaticLanguageEditingPolicy::new(&[], &[]);
pub(crate) static BRACE_LANGUAGE_EDITING_POLICY: StaticLanguageEditingPolicy =
    StaticLanguageEditingPolicy::new(&BRACE_LANGUAGE_DELIMITER_RULES, &BRACE_NEWLINE_RULES);
pub(crate) static RUST_EDITING_POLICY: StaticLanguageEditingPolicy =
    StaticLanguageEditingPolicy::new(&RUST_DELIMITER_RULES, &BRACE_NEWLINE_RULES);
pub(crate) static GO_EDITING_POLICY: StaticLanguageEditingPolicy =
    StaticLanguageEditingPolicy::new(&GO_DELIMITER_RULES, &BRACE_NEWLINE_RULES);
pub(crate) static PYTHON_EDITING_POLICY: StaticLanguageEditingPolicy =
    StaticLanguageEditingPolicy::new(&PYTHON_DELIMITER_RULES, &PYTHON_NEWLINE_RULES);
