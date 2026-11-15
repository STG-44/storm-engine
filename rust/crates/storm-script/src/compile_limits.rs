//! Explicit finite compilation budgets, independent of VM execution budgets.
use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileLimits {
    pub file_bytes: usize,
    pub total_bytes: usize,
    pub files: usize,
    pub tokens: usize,
    pub functions: usize,
    pub globals: usize,
    pub constants: usize,
    pub locals: usize,
}
impl Default for CompileLimits {
    fn default() -> Self {
        Self {
            file_bytes: crate::source::MAX_FILE,
            total_bytes: crate::source::MAX_TOTAL,
            files: crate::source::MAX_FILES,
            tokens: crate::compile::MAX_TOKENS,
            functions: 128,
            globals: 128,
            constants: 1024,
            locals: crate::compile::MAX_LOCALS,
        }
    }
}
impl CompileLimits {
    /// Finite installed-game input profile. Grammar, native APIs and runtime
    /// allocation/execution limits remain separate compatibility boundaries.
    pub const fn game() -> Self {
        Self {
            file_bytes: 2 * 1024 * 1024,
            total_bytes: 32 * 1024 * 1024,
            files: 2048,
            tokens: 2 * 1024 * 1024,
            functions: 32768,
            globals: 8192,
            constants: 16384,
            locals: 1024,
        }
    }
    pub(crate) fn validate(self) -> Result<()> {
        let max = Self::game();
        for (name, value, maximum) in [
            ("file bytes", self.file_bytes, max.file_bytes),
            ("total bytes", self.total_bytes, max.total_bytes),
            ("files", self.files, max.files),
            ("tokens", self.tokens, max.tokens),
            ("functions", self.functions, max.functions),
            ("globals", self.globals, max.globals),
            ("constants", self.constants, max.constants),
            ("locals", self.locals, max.locals),
        ] {
            ensure!(
                value > 0 && value <= maximum,
                "unsupported compile limit for {name}"
            );
        }
        Ok(())
    }
}
