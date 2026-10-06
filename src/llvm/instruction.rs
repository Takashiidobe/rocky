use llvm_amdgpu_types::{DecodedInstruction, SourceOperand, U32, VectorRegister};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use crate::asm::{Disassembly, InstructionLine};
use crate::error::Error;
use crate::span::{Location, Source, Span};

#[derive(Debug, PartialEq, Eq)]
pub enum VectorSource {
    Register(VectorRegister<U32>),
    Immediate(u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction {
    VCmpEqU32 {
        lhs: VectorSource,
        rhs: VectorRegister<U32>,
    },
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
    let (lhs, rhs) = match decoded {
        DecodedInstruction::VCmpEqU32E32(instruction) => match instruction.src0() {
            SourceOperand::VectorRegister(lhs) => (VectorSource::Register(lhs), instruction.src1()),
            SourceOperand::Immediate(lhs) => {
                (VectorSource::Immediate(lhs.value()), instruction.src1())
            }
            _ => {
                return Err(InstructionError::new(
                    "only vector registers and immediates are lifted for src0",
                    line.operands_span(),
                ));
            }
        },
        _ => {
            return Err(InstructionError::new(
                "instruction is not supported by the LLVM lifter",
                location,
            ));
        }
    };
    Ok(Span {
        value: Instruction::VCmpEqU32 { lhs, rhs },
        span: line.span.clone(),
    })
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
