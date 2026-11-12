use llvm_amdgpu_types::{
    B32, B64, DecodedInstruction, OperandType, ScalarMemoryOffset, ScalarRegister,
    ScalarRegisterOperand, ScalarSourceOperand, SourceOperand, SpecialRegister, U32,
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
    SpecialRegister(SpecialRegister<T>),
    Immediate(u32),
}

impl<T: OperandType<Value = u32>> From<ScalarSourceOperand<T>> for ScalarSource<T> {
    fn from(source: ScalarSourceOperand<T>) -> Self {
        match source {
            ScalarSourceOperand::Register(register) => Self::Register(register),
            ScalarSourceOperand::SpecialRegister(register) => Self::SpecialRegister(register),
            ScalarSourceOperand::Immediate(immediate) => Self::Immediate(immediate.value()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VectorSource<T: OperandType> {
    Register(VectorRegister<T>),
    Immediate(u32),
}

/// The Instructions we currently parse.
#[derive(Debug, PartialEq)]
pub enum Instruction {
    VAndB32E32 {
        dest: VectorRegister<B32>,
        lhs: VectorSource<B32>,
        rhs: VectorRegister<B32>,
    },
    VCmpEqU32 {
        lhs: VectorSource<U32>,
        rhs: VectorRegister<U32>,
    },
    SAndSaveexecB32 {
        src: ScalarSource<B32>,
        dest: ScalarRegister<B32>,
    },
    SXorB32 {
        lhs: ScalarSource<B32>,
        rhs: ScalarSource<B32>,
    },
    SOrB32 {
        dest: ScalarRegisterOperand<B32>,
        lhs: ScalarSource<B32>,
        rhs: ScalarSource<B32>,
    },
    VMulU32U24E32 {
        dest: VectorRegister<U32>,
        lhs: VectorSource<U32>,
        rhs: VectorRegister<U32>,
    },
    SAndn2saveexecB32 {
        src: ScalarSource<B32>,
        dest: ScalarRegister<B32>,
    },
    VLshlrevB32E32 {
        dest: VectorRegister<B32>,
        lhs: VectorSource<B32>,
        rhs: VectorRegister<B32>,
    },
    SLoadDwordx2 {
        dest: ScalarRegister<B64>,
        base: ScalarRegister<B64>,
        offset: ScalarMemoryOffset,
    },
    SWaitcnt,
    GlobalStoreDword {
        vaddr: VectorRegister<B64>,
        data: VectorRegister<B32>,
        saddr: ScalarRegister<B64>,
    },
    SEndpgm,
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
        DecodedInstruction::SAndSaveexecB32(instruction) => Ok(Span {
            value: Instruction::SAndSaveexecB32 {
                src: instruction.src0().into(),
                dest: instruction.destination(),
            },
            span: line.span.clone(),
        }),
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
        DecodedInstruction::VAndB32E32(instruction) => {
            let lhs = match instruction.src0() {
                SourceOperand::VectorRegister(lhs) => VectorSource::Register(lhs),
                SourceOperand::Immediate(lhs) => VectorSource::Immediate(lhs.value()),
                _ => {
                    return Err(InstructionError::new(
                        "only vector registers and immediates are lifted for src0",
                        line.operands_span(),
                    ));
                }
            };
            Ok(Span {
                value: Instruction::VAndB32E32 {
                    dest: instruction.destination(),
                    lhs,
                    rhs: instruction.src1(),
                },
                span: line.span.clone(),
            })
        }
        DecodedInstruction::SXorB32(instruction) => Ok(Span {
            value: Instruction::SXorB32 {
                lhs: instruction.src0().into(),
                rhs: instruction.src1().into(),
            },
            span: line.span.clone(),
        }),
        DecodedInstruction::SOrB32(instruction) => Ok(Span {
            value: Instruction::SOrB32 {
                dest: instruction.destination(),
                lhs: instruction.src0().into(),
                rhs: instruction.src1().into(),
            },
            span: line.span.clone(),
        }),
        DecodedInstruction::SAndn2SaveexecB32(instruction) => Ok(Span {
            value: Instruction::SAndn2saveexecB32 {
                src: instruction.src0().into(),
                dest: instruction.destination(),
            },
            span: line.span.clone(),
        }),
        DecodedInstruction::VLshlrevB32E32(instruction) => {
            let lhs = match instruction.src0() {
                SourceOperand::VectorRegister(lhs) => VectorSource::Register(lhs),
                SourceOperand::Immediate(lhs) => VectorSource::Immediate(lhs.value()),
                _ => {
                    return Err(InstructionError::new(
                        "only vector registers and immediates are lifted for src0",
                        line.operands_span(),
                    ));
                }
            };
            Ok(Span {
                value: Instruction::VLshlrevB32E32 {
                    dest: instruction.destination(),
                    lhs,
                    rhs: instruction.src1(),
                },
                span: line.span.clone(),
            })
        }
        DecodedInstruction::SLoadDwordx2(instruction) => Ok(Span {
            value: Instruction::SLoadDwordx2 {
                dest: instruction.destination(),
                base: instruction.base(),
                offset: instruction.offset(),
            },
            span: line.span.clone(),
        }),
        DecodedInstruction::SWaitcnt(_) => Ok(Span {
            value: Instruction::SWaitcnt,
            span: line.span.clone(),
        }),
        DecodedInstruction::VMulU32U24E32(instruction) => {
            let lhs = match instruction.src0() {
                SourceOperand::VectorRegister(lhs) => VectorSource::Register(lhs),
                SourceOperand::Immediate(lhs) => VectorSource::Immediate(lhs.value()),
                _ => {
                    return Err(InstructionError::new(
                        "only vector registers and immediates are lifted for src0",
                        line.operands_span(),
                    ));
                }
            };
            Ok(Span {
                value: Instruction::VMulU32U24E32 {
                    dest: instruction.destination(),
                    lhs,
                    rhs: instruction.src1(),
                },
                span: line.span.clone(),
            })
        }
        DecodedInstruction::GlobalStoreDword(instruction) => Ok(Span {
            value: Instruction::GlobalStoreDword {
                vaddr: instruction.address(),
                data: instruction.data(),
                saddr: instruction.saddr(),
            },
            span: line.span.clone(),
        }),
        DecodedInstruction::SEndpgm => Ok(Span {
            value: Instruction::SEndpgm,
            span: line.span.clone(),
        }),
        DecodedInstruction::SCodeEnd => Ok(Span {
            value: Instruction::SCodeEnd,
            span: line.span.clone(),
        }),
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
