pub mod instruction;
mod ir;
pub mod register;

pub use instruction::{InstructionError, from_disassembly};
pub use ir::emit;
