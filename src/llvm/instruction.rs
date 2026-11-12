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
pub enum ScalarSourceU32 {
    Register(ScalarRegister<U32>),
    Immediate(u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScalarSourceB32 {
    Register(ScalarRegister<B32>),
    Immediate(u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum VectorSourceU32 {
    Register(VectorRegister<U32>),
    Immediate(u32),
}

// Not sure all of these make sense with just LHS RHS
#[derive(Debug, PartialEq, Eq)]
pub enum Instruction {
    VAndB32E32 {
        dest: VectorRegister<U32>,
        lhs: VectorSourceU32,
        rhs: VectorRegister<U32>,
    },
    VCmpEqU32 {
        lhs: VectorSourceU32,
        rhs: VectorRegister<U32>,
    },
    SAndSaveexecB32 {
        src: ScalarSourceB32,
        dest: ScalarSourceB32,
    },
    SXorB32 {
        lhs: ScalarRegister<U32>,
        rhs: ScalarSourceU32,
    },
    VMulU32U24E32 {
        dest: VectorRegister<U32>,
        lhs: VectorSourceU32,
        rhs: VectorRegister<U32>,
    },
    SAndn2saveexecB32 {
        lhs: ScalarSourceU32,
        rhs: ScalarSourceU32,
    },
    VLshlrevB32E32 {
        dest: VectorRegister<U32>,
        lhs: VectorSourceU32,
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
                    (ScalarSourceB32::Register(lhs), instruction.destination())
                }
                ScalarSourceOperand::Immediate(lhs) => {
                    (ScalarSourceB32::Register(lhs), instruction.destination())
                }
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
                    dest: ScalarSourceB32::Register(dest),
                },
                span: line.span.clone(),
            })
        }
        DecodedInstruction::VCmpEqU32E32(instruction) => {
            let (lhs, rhs) = match instruction.src0() {
                SourceOperand::VectorRegister(lhs) => {
                    (VectorSourceU32::Register(lhs), instruction.src1())
                }
                SourceOperand::Immediate(lhs) => {
                    (VectorSourceU32::Immediate(lhs.value()), instruction.src1())
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
    help(
        "The lifter supports wave32 v_cmp_eq_u32_e32 with vector-register or integer src0 and vector-register src1."
    )
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
