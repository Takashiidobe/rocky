use std::process::Command;

use crate::error::Error;

pub fn run(command: &mut Command) -> Result<String, Error> {
    let program = command.get_program().to_owned();
    let args: Vec<_> = command.get_args().map(|arg| arg.to_owned()).collect();
    let output = command.output().map_err(|source| Error::ToolIo {
        program: program.clone(),
        args: args.clone(),
        source,
    })?;
    if !output.status.success() {
        return Err(Error::ToolFailed {
            program,
            args,
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    String::from_utf8(output.stdout).map_err(|source| Error::ToolUtf8 { program, source })
}
