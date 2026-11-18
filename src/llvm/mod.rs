pub mod instruction;
pub mod ir;

pub use instruction::{InstructionError, from_disassembly};
pub use ir::emit;
