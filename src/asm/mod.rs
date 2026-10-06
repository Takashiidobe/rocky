mod disassembly;
pub mod lexer;

use miette::NamedSource;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

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
    fs::write(&path, &text).map_err(|source| Error::WriteFile {
        path: path.clone(),
        source,
    })?;
    let disassembly =
        disassembly::parse(Arc::new(NamedSource::new(path.display().to_string(), text)))?;
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
            println!("{:016x} <{}>:", symbol.address.value, symbol.name.text());
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
            instruction.mnemonic_span().text(),
            instruction.operands_span().text(),
            words,
            instruction.size_bytes()
        );
    }
}
