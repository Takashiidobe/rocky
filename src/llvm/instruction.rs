use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use super::register::{MaskRegister, VectorRegister};
use crate::asm::lexer::Token;
use crate::asm::{Disassembly, InstructionLine};
use crate::error::Error;
use crate::span::{Location, Source, Span};

#[derive(Debug, PartialEq, Eq)]
pub enum VectorSource {
    Register(VectorRegister),
    Immediate(u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction {
    VCmpEqU32 {
        address: u64,
        destination: MaskRegister,
        lhs: VectorSource,
        rhs: VectorRegister,
    },
}

fn immediate(text: &str) -> Option<u32> {
    let value = if let Some(hex) = text.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()?
    } else {
        text.parse::<i64>().ok()?
    };
    (i32::MIN as i64..=u32::MAX as i64)
        .contains(&value)
        .then_some(value as u32)
}

fn parse_instruction(
    instruction: &Span<InstructionLine>,
) -> Result<Span<Instruction>, InstructionError> {
    let error = |kind, span: Location| InstructionError {
        source_text: span.source.clone(),
        span: span.into(),
        kind,
    };
    let mnemonic_span = instruction.mnemonic_span();
    let mnemonic = mnemonic_span.text();
    if mnemonic != "v_cmp_eq_u32_e32" {
        return Err(error(
            InstructionErrorKind::UnsupportedInstruction {
                reason: format!("unsupported mnemonic {mnemonic}"),
            },
            instruction.mnemonic_span(),
        ));
    }
    let [destination, comma1, lhs, comma2, rhs] = instruction.operand_tokens() else {
        return Err(error(
            InstructionErrorKind::UnsupportedInstruction {
                reason: "expected three operands separated by commas".into(),
            },
            instruction.operands_span(),
        ));
    };
    for comma in [comma1, comma2] {
        if comma.value != Token::Comma {
            return Err(error(
                InstructionErrorKind::UnsupportedInstruction {
                    reason: "expected three operands separated by commas".into(),
                },
                comma.span.clone(),
            ));
        }
    }
    if destination.span.text() != "vcc_lo" {
        return Err(error(
            InstructionErrorKind::UnsupportedInstruction {
                reason: "expected wave32 mask register vcc_lo".into(),
            },
            destination.span.clone(),
        ));
    }
    let lhs_text = lhs.span.text();
    let lhs_value = VectorRegister::parse(lhs_text)
        .map(VectorSource::Register)
        .or_else(|| immediate(lhs_text).map(VectorSource::Immediate))
        .ok_or_else(|| {
            error(
                InstructionErrorKind::UnsupportedInstruction {
                    reason: "expected numbered vector register or 32-bit decimal/hex integer"
                        .into(),
                },
                lhs.span.clone(),
            )
        })?;
    let rhs_value = VectorRegister::parse(rhs.span.text()).ok_or_else(|| {
        error(
            InstructionErrorKind::UnsupportedInstruction {
                reason: "expected numbered vector register v0 through v255".into(),
            },
            rhs.span.clone(),
        )
    })?;
    Ok(Span {
        span: instruction.span.clone(),
        value: Instruction::VCmpEqU32 {
            address: instruction.address.value,
            destination: MaskRegister::VccLo,
            lhs: lhs_value,
            rhs: rhs_value,
        },
    })
}

pub fn from_disassembly(
    disassembly: &Disassembly,
    strict: bool,
) -> Result<Vec<Instruction>, Error> {
    let mut instructions = Vec::new();
    for instruction in &disassembly.instructions {
        match parse_instruction(instruction) {
            Ok(instruction) => instructions.push(instruction.into_inner()),
            Err(error) if strict => return Err(error.into()),
            Err(error) => match error.kind {
                InstructionErrorKind::UnsupportedInstruction { reason } => {
                    eprintln!(
                        "skipped instruction {}: {reason}",
                        instruction.mnemonic_span().text()
                    );
                }
            },
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
        "Supported form: v_cmp_eq_u32_e32 vcc_lo, <vector register or integer>, <vector register>. Only wave32 comparisons are supported."
    )
)]
pub struct InstructionError {
    #[source_code]
    source_text: Source,
    #[label("{kind}")]
    span: SourceSpan,
    #[source]
    kind: InstructionErrorKind,
}

#[derive(Debug, Error)]
enum InstructionErrorKind {
    #[error("{reason}")]
    UnsupportedInstruction { reason: String },
}
