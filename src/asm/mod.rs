mod disassembly;
pub mod lexer;

use std::fs;
use std::path::Path;
use std::process::Command;

pub use disassembly::{Disassembly, InstructionLine, ParseError};

use crate::compile::GPU_ARCH;
use crate::error::Error;
use crate::tool::run;

pub fn objdump(object: &Path) -> Result<Disassembly, Error> {
    let text = run(Command::new("llvm-objdump")
        .arg("-d")
        .arg(format!("--mcpu={GPU_ARCH}"))
        .arg(object))?;
    let path = object.with_extension("objdump");
    fs::write(&path, &text).map_err(|source| Error::WriteFile { path, source })?;
    let disassembly = disassembly::parse(text)?;
    if disassembly.instructions.is_empty() {
        return Err(Error::NoInstructions);
    }
    Ok(disassembly)
}

pub fn print(disassembly: &Disassembly) {
    for instruction in &disassembly.instructions {
        for symbol in disassembly
            .symbols
            .iter()
            .filter(|symbol| symbol.address.value == instruction.address.value)
        {
            println!(
                "{:016x} <{}>:",
                symbol.address.value,
                &disassembly.source[symbol.name.clone()]
            );
        }
        let words = instruction
            .words
            .iter()
            .map(|word| format!("{:08X}", word.value))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "  {:012x}: {:<24} {:<40} // {} ({} bytes)",
            instruction.address.value,
            &disassembly.source[instruction.mnemonic_span()],
            &disassembly.source[instruction.operands_span()],
            words,
            instruction.size_bytes()
        );
    }
}
