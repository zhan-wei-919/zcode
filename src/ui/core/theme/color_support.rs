#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColorSupport {
    TrueColor,
    Ansi256,
    Ansi16,
}

pub fn detect_terminal_color_support() -> TerminalColorSupport {
    if let Ok(value) = std::env::var("ZCODE_COLOR_SUPPORT") {
        let value = value.trim().to_ascii_lowercase();
        match value.as_str() {
            "truecolor" | "24bit" | "rgb" => return TerminalColorSupport::TrueColor,
            "256" | "ansi256" => return TerminalColorSupport::Ansi256,
            "16" | "ansi16" | "basic" => return TerminalColorSupport::Ansi16,
            _ => {}
        }
    }

    let colorterm = std::env::var("COLORTERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();

    if colorterm.contains("truecolor")
        || colorterm.contains("24bit")
        || colorterm.contains("direct")
        || term.contains("truecolor")
        || term.contains("24bit")
        || term.contains("direct")
    {
        return TerminalColorSupport::TrueColor;
    }

    if term.contains("256color") {
        return TerminalColorSupport::Ansi256;
    }

    TerminalColorSupport::Ansi16
}

#[cfg(test)]
#[path = "../../../../tests/unit/ui/core/color_support.rs"]
mod tests;
