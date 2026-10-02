use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::string::FromUtf8Error;

use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Instruction(#[from] crate::llvm::InstructionError),
    #[error("could not build LLVM IR: {0}")]
    #[diagnostic(code(rocky::ir_builder))]
    IrBuilder(#[from] inkwell::builder::BuilderError),
    #[error("could not configure LLVM instruction: {0}")]
    #[diagnostic(code(rocky::ir_instruction))]
    IrInstruction(#[from] inkwell::values::InstructionValueError),
    #[error("could not construct LLVM IR: {message}")]
    #[diagnostic(code(rocky::ir_construction))]
    IrConstruction { message: String },
    #[error("LLVM IR verification failed: {message}")]
    #[diagnostic(code(rocky::ir_verification))]
    IrVerification { message: String },
    #[error("disassembly contains no v_cmp_eq_u32_e32 instruction to lift")]
    #[diagnostic(code(rocky::no_compare))]
    NoCompare,
    #[error("could not execute {program:?}")]
    #[diagnostic(
        code(rocky::tool_io),
        help("Check that the tool is installed and executable. Arguments: {args:?}")
    )]
    ToolIo {
        program: OsString,
        args: Vec<OsString>,
        #[source]
        source: io::Error,
    },
    #[error("{program:?} failed ({status})")]
    #[diagnostic(
        code(rocky::tool_failed),
        help("Arguments: {args:?}\n{stderr}\n{stdout}")
    )]
    ToolFailed {
        program: OsString,
        args: Vec<OsString>,
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
    #[error("{program:?} produced invalid UTF-8")]
    #[diagnostic(code(rocky::tool_utf8), help("Expected textual LLVM tool output."))]
    ToolUtf8 {
        program: OsString,
        #[source]
        source: FromUtf8Error,
    },
    #[error("could not open input file {path}")]
    #[diagnostic(
        code(rocky::input_file),
        help("Provide an existing, readable HIP source file: cargo run -- hip/branch.hip")
    )]
    InputFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("input path {path} is not a regular file")]
    #[diagnostic(
        code(rocky::input_format),
        help("Provide a HIP source file rather than a directory or device.")
    )]
    InputFormat { path: PathBuf },
    #[error("could not create directory {path}")]
    #[diagnostic(
        code(rocky::create_directory),
        help("Check the output directory permissions and available disk space.")
    )]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not write {path}")]
    #[diagnostic(
        code(rocky::write_file),
        help("Check the output file permissions and available disk space.")
    )]
    WriteFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("expected one HIP target for {arch}, found targets: {targets:?}")]
    #[diagnostic(
        code(rocky::hip_target),
        help("The compiled bundle must contain exactly one HIP image for {arch}.")
    )]
    HipTarget { arch: String, targets: Vec<String> },
    #[error("unexpected argument {argument:?}")]
    #[diagnostic(code(rocky::usage), help("Usage: rocky [kernel.hip]"))]
    Usage { argument: OsString },
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parse(#[from] crate::asm::ParseError),
    #[error("objdump output contains no instructions")]
    #[diagnostic(
        code(rocky::no_instructions),
        help(
            "Expected AMDGPU disassembly with instruction encodings after '// ADDRESS:'. Inspect target/hip/branch.objdump."
        )
    )]
    NoInstructions,
}
