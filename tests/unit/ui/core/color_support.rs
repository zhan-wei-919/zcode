use super::*;

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct EnvRestore {
    zcode_color_support: Option<std::ffi::OsString>,
    colorterm: Option<std::ffi::OsString>,
    term: Option<std::ffi::OsString>,
}

impl EnvRestore {
    fn capture() -> Self {
        Self {
            zcode_color_support: std::env::var_os("ZCODE_COLOR_SUPPORT"),
            colorterm: std::env::var_os("COLORTERM"),
            term: std::env::var_os("TERM"),
        }
    }

    fn restore_var(key: &str, value: Option<std::ffi::OsString>) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        Self::restore_var("ZCODE_COLOR_SUPPORT", self.zcode_color_support.take());
        Self::restore_var("COLORTERM", self.colorterm.take());
        Self::restore_var("TERM", self.term.take());
    }
}

#[test]
fn explicit_override_takes_precedence() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _restore = EnvRestore::capture();

    std::env::set_var("COLORTERM", "truecolor");
    std::env::set_var("TERM", "xterm-256color");

    std::env::set_var("ZCODE_COLOR_SUPPORT", "16");
    assert_eq!(
        detect_terminal_color_support(),
        TerminalColorSupport::Ansi16
    );

    std::env::set_var("ZCODE_COLOR_SUPPORT", "ansi256");
    assert_eq!(
        detect_terminal_color_support(),
        TerminalColorSupport::Ansi256
    );

    std::env::set_var("ZCODE_COLOR_SUPPORT", "truecolor");
    assert_eq!(
        detect_terminal_color_support(),
        TerminalColorSupport::TrueColor
    );
}

#[test]
fn detects_truecolor_from_colorterm() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _restore = EnvRestore::capture();

    std::env::remove_var("ZCODE_COLOR_SUPPORT");
    std::env::set_var("COLORTERM", "truecolor");
    std::env::set_var("TERM", "xterm");

    assert_eq!(
        detect_terminal_color_support(),
        TerminalColorSupport::TrueColor
    );
}

#[test]
fn falls_back_to_ansi256_from_term() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _restore = EnvRestore::capture();

    std::env::remove_var("ZCODE_COLOR_SUPPORT");
    std::env::remove_var("COLORTERM");
    std::env::set_var("TERM", "xterm-256color");

    assert_eq!(
        detect_terminal_color_support(),
        TerminalColorSupport::Ansi256
    );
}
