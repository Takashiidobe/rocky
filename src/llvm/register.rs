use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VectorRegister(u32);

impl VectorRegister {
    pub fn new(index: u32) -> Option<Self> {
        (index < 256).then_some(Self(index))
    }

    pub fn index(self) -> u32 {
        self.0
    }

    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('v')?;
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        Self::new(digits.parse().ok()?)
    }
}

impl fmt::Display for VectorRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskRegister {
    VccLo,
}
