use llvm_amdgpu_types::{
    B32, DecodedInstruction, OperandType, ScalarRegister, ScalarSourceOperand, SourceOperand, U32,
    VectorRegister,
};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use crate::asm::{Disassembly, InstructionLine};
use crate::error::Error;
use crate::span::{Location, Source, Span};
use std::fmt::Debug;

#[derive(Debug, PartialEq, Eq)]
pub enum ScalarSource<T: OperandType> {
    Register(ScalarRegister<T>),
    Immediate(u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum VectorSource<T: OperandType> {
    Register(VectorRegister<T>),
    Immediate(u32),
}

/// The Instructions we currently parse.
#[derive(Debug, PartialEq, Eq)]
pub enum Instruction {
    VAndB32E32 {
        dest: VectorRegister<U32>,
        lhs: VectorSource<U32>,
        rhs: VectorRegister<U32>,
    },
    VCmpEqU32 {
        lhs: VectorSource<U32>,
        rhs: VectorRegister<U32>,
    },
    SAndSaveexecB32 {
        src: ScalarSource<B32>,
        dest: ScalarSource<B32>,
    },
    SXorB32 {
        lhs: ScalarRegister<U32>,
        rhs: ScalarSource<U32>,
    },
    VMulU32U24E32 {
        dest: VectorRegister<U32>,
        lhs: VectorSource<U32>,
        rhs: VectorRegister<U32>,
    },
    SAndn2saveexecB32 {
        lhs: ScalarSource<U32>,
        rhs: ScalarSource<U32>,
    },
    VLshlrevB32E32 {
        dest: VectorRegister<U32>,
        lhs: VectorSource<U32>,
        rhs: VectorRegister<U32>,
    },
    SLoadDwordx2 {
        dest: ScalarRegister<U32>,
        base: Option<ScalarRegister<U32>>,
        offset: Option<U32>,
    },
    SWaitcnt,
    GlobalStoreDword {
        vaddr: VectorRegister<U32>,
        data: VectorRegister<U32>,
        saddr: ScalarRegister<U32>,
        offset: Option<i32>,
    },
    SCodeEnd,
}

fn parse_instruction(line: &Span<InstructionLine>) -> Result<Span<Instruction>, InstructionError> {
    let location = line.assembly_span();
    let decoded = llvm_amdgpu_types::parse(location.text()).map_err(|error| {
        InstructionError::new(
            error.reason,
            Location {
                source: location.source.clone(),
                range: location.range.start + error.range.start
                    ..location.range.start + error.range.end,
            },
        )
    })?;
    match decoded {
        DecodedInstruction::SAndSaveexecB32(instruction) => {
            let (src, dest) = match instruction.src0() {
                ScalarSourceOperand::Register(lhs) => {
                    (ScalarSource::Register(lhs), instruction.destination())
                }
                ScalarSourceOperand::Immediate(lhs) => (
                    ScalarSource::Immediate(lhs.value()),
                    instruction.destination(),
                ),
                _ => {
                    return Err(InstructionError::new(
                        "only vector registers and immediates are lifted for src0",
                        line.operands_span(),
                    ));
                }
            };
            Ok(Span {
                value: Instruction::SAndSaveexecB32 {
                    src,
                    dest: ScalarSource::Register(dest),
                },
                span: line.span.clone(),
            })
        }
        DecodedInstruction::VCmpEqU32E32(instruction) => {
            let (lhs, rhs) = match instruction.src0() {
                SourceOperand::VectorRegister(lhs) => {
                    (VectorSource::Register(lhs), instruction.src1())
                }
                SourceOperand::Immediate(lhs) => {
                    (VectorSource::Immediate(lhs.value()), instruction.src1())
                }
                _ => {
                    return Err(InstructionError::new(
                        "only vector registers and immediates are lifted for src0",
                        line.operands_span(),
                    ));
                }
            };
            Ok(Span {
                value: Instruction::VCmpEqU32 { lhs, rhs },
                span: line.span.clone(),
            })
        }
        _ => Err(InstructionError::new(
            "instruction is not supported by the LLVM lifter",
            location,
        )),
    }
}

pub fn from_disassembly(
    disassembly: &Disassembly,
    strict: bool,
) -> Result<Vec<Instruction>, Error> {
    let mut instructions = Vec::new();
    for line in &disassembly.instructions {
        match parse_instruction(line) {
            Ok(instruction) => instructions.push(instruction.into_inner()),
            Err(error) if strict => return Err(error.into()),
            Err(error) => eprintln!(
                "skipped instruction {}: {}",
                line.assembly_span().text(),
                error.reason
            ),
        }
    }
    if instructions.is_empty() {
        return Err(Error::NoSupportedInstructions);
    }
    Ok(instructions)
}

#[derive(Debug, Error, Diagnostic)]
#[error("unsupported instruction")]
#[diagnostic(
    code(rocky::instruction),
    help("The lifter doesn't support instruction: {source_text:?}")
)]
pub struct InstructionError {
    #[source_code]
    source_text: Source,
    #[label("{reason}")]
    span: SourceSpan,
    reason: String,
}

impl InstructionError {
    fn new(reason: impl Into<String>, location: Location) -> Self {
        Self {
            reason: reason.into(),
            source_text: location.source.clone(),
            span: location.into(),
        }
    }
}
