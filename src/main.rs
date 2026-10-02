mod asm;
mod compile;
mod error;
mod llvm;
mod tool;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use error::Error;

fn execute() -> Result<(), Error> {
    let mut args = env::args_os().skip(1);
    let source = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("hip/branch.hip"));
    if let Some(argument) = args.next() {
        return Err(Error::Usage { argument });
    }
    let object = compile::hip(&source)?;
    let disassembly = asm::objdump(&object)?;
    asm::print(&disassembly);
    let instructions = llvm::from_disassembly(&disassembly)?;
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
