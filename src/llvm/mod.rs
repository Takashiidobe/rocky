pub mod instruction;
mod ir;

pub use instruction::{InstructionError, from_disassembly};
pub use ir::emit;
