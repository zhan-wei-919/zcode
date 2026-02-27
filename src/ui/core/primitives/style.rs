use std::ops::{BitOr, BitOrAssign};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Reset,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Mod(u16);

impl Mod {
    pub const NONE: Self = Self(0);
    pub const BOLD: Self = Self(1 << 0);
    pub const DIM: Self = Self(1 << 1);
    pub const UNDERLINE: Self = Self(1 << 2);
    pub const REVERSE: Self = Self(1 << 3);
    pub const ITALIC: Self = Self(1 << 4);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Mod {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Mod {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub mods: Mod,
}

impl Style {
    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn add_mod(mut self, m: Mod) -> Self {
        self.mods |= m;
        self
    }

    /// Merge `other` on top of `self` (like ratatui's `Style::patch`).
    pub fn patch(mut self, other: Style) -> Self {
        if let Some(fg) = other.fg {
            self.fg = Some(fg);
        }
        if let Some(bg) = other.bg {
            self.bg = Some(bg);
        }
        self.mods |= other.mods;
        self
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/ui/core/style.rs"]
mod tests;
