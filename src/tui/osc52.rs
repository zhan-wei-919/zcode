use std::io::{self, Write};

const OSC52_PREFIX: &str = "\x1b]52;c;";
const OSC52_SUFFIX_BEL: &str = "\x07";

const TMUX_PREFIX: &str = "\x1bPtmux;\x1b\x1b]52;c;";
const TMUX_SUFFIX: &str = "\x07\x1b\\";

// Many terminals apply fairly small OSC52 length limits; keep this conservative.
pub const OSC52_MAX_BYTES: usize = 100 * 1024; // 100KB

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Osc52Error {
    TooLarge { bytes: usize },
    Io(String),
}

impl std::fmt::Display for Osc52Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Osc52Error::TooLarge { bytes } => write!(
                f,
                "text too large for OSC52 ({} KB, limit {} KB)",
                bytes / 1024,
                OSC52_MAX_BYTES / 1024
            ),
            Osc52Error::Io(msg) => write!(f, "io error: {}", msg),
        }
    }
}

impl std::error::Error for Osc52Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Osc52Env {
    pub is_tmux: bool,
}

impl Osc52Env {
    pub fn detect() -> Self {
        Self {
            is_tmux: std::env::var_os("TMUX").is_some(),
        }
    }
}

pub fn build_sequence(text: &str, env: Osc52Env) -> Result<String, Osc52Error> {
    let bytes = text.as_bytes();
    if bytes.len() > OSC52_MAX_BYTES {
        return Err(Osc52Error::TooLarge { bytes: bytes.len() });
    }

    let b64 = base64_encode(bytes);
    if env.is_tmux {
        Ok(format!("{TMUX_PREFIX}{b64}{TMUX_SUFFIX}"))
    } else {
        Ok(format!("{OSC52_PREFIX}{b64}{OSC52_SUFFIX_BEL}"))
    }
}

pub fn write_sequence<W: Write>(mut w: W, text: &str, env: Osc52Env) -> Result<(), Osc52Error> {
    let seq = build_sequence(text, env)?;
    w.write_all(seq.as_bytes())
        .and_then(|_| w.flush())
        .map_err(|e| Osc52Error::Io(e.to_string()))
}

pub fn copy_to_clipboard(text: &str) -> Result<(), Osc52Error> {
    write_sequence(io::stdout(), text, Osc52Env::detect())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0usize;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).copied().unwrap_or(0);
        let b2 = bytes.get(i + 2).copied().unwrap_or(0);

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        let c0 = TABLE[((n >> 18) & 0x3f) as usize] as char;
        let c1 = TABLE[((n >> 12) & 0x3f) as usize] as char;
        let c2 = TABLE[((n >> 6) & 0x3f) as usize] as char;
        let c3 = TABLE[(n & 0x3f) as usize] as char;

        out.push(c0);
        out.push(c1);

        if i + 1 < bytes.len() {
            out.push(c2);
        } else {
            out.push('=');
        }

        if i + 2 < bytes.len() {
            out.push(c3);
        } else {
            out.push('=');
        }

        i = i.saturating_add(3);
    }

    out
}

#[cfg(test)]
#[path = "../../tests/unit/tui/osc52.rs"]
mod tests;
