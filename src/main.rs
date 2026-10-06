mod asm;
mod compile;
mod error;
mod llvm;
mod span;
mod tool;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use error::Error;

fn execute() -> Result<(), Error> {
    let mut strict = false;
    let mut source = None;
    for argument in env::args_os().skip(1) {
        if argument == "--strict" {
            strict = true;
        } else if argument.to_string_lossy().starts_with('-') || source.is_some() {
            return Err(Error::Usage { argument });
        } else {
            source = Some(PathBuf::from(argument));
        }
    }
    let source =
        source.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("hip/branch.hip"));
    let object = compile::hip(&source)?;
    let disassembly = asm::objdump(&object)?;
    asm::print(&disassembly);
    let instructions = llvm::from_disassembly(&disassembly, strict)?;
    let count = instructions.len();
    let ir = llvm::emit(instructions)?;
    let path = source.with_extension("ll");
    fs::write(&path, &ir).map_err(|source| Error::WriteFile {
        path: path.clone(),
        source,
    })?;
    eprintln!("Emitted {count} instruction(s) to {}", path.display());
    println!("{ir}");
    Ok(())
}

fn main() -> miette::Result<()> {
    execute()?;
    Ok(())
}
