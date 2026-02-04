#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id(pub u64);

impl Id {
    pub const fn raw(v: u64) -> Self {
        Self(v)
    }
}

/// A stable (build-independent) id builder based on FNV-1a 64-bit hashing.
///
/// We avoid `std` hashers here because their output is not guaranteed to be
/// stable across Rust versions/platforms.
#[derive(Clone, Copy, Debug)]
pub struct IdPath {
    h: u64,
}

impl IdPath {
    pub fn root(ns: &'static str) -> Self {
        Self {
            h: fnv1a64(ns.as_bytes()),
        }
    }

    pub fn push_str(mut self, s: &str) -> Self {
        self.h = fnv1a64_continue(self.h, s.as_bytes());
        // Add a separator to reduce accidental concatenation collisions.
        self.h = fnv1a64_continue(self.h, &[0xff]);
        self
    }

    pub fn push_u64(mut self, v: u64) -> Self {
        self.h = fnv1a64_continue(self.h, &v.to_le_bytes());
        self.h = fnv1a64_continue(self.h, &[0xff]);
        self
    }

    pub fn finish(self) -> Id {
        Id(self.h)
    }
}

const FNV_OFFSET_BASIS_64: u64 = 0xcbf29ce484222325;
const FNV_PRIME_64: u64 = 0x100000001b3;

fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv1a64_continue(FNV_OFFSET_BASIS_64, bytes)
}

fn fnv1a64_continue(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME_64);
    }
    h
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/core/id.rs"]
mod tests;
