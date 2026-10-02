use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use super::register::{MaskRegister, VectorRegister};
use crate::asm::lexer::{Span, Spanned, Token};
use crate::asm::{Disassembly, InstructionLine};
use crate::error::Error;

#[derive(Debug, PartialEq, Eq)]
pub enum VectorSource {
    Register(VectorRegister),
    Immediate(u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction {
    VCmpEqU32 {
        span: Span,
        address: Spanned<u64>,
        destination: Spanned<MaskRegister>,
        lhs: Spanned<VectorSource>,
        rhs: Spanned<VectorRegister>,
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
    disassembly: &Disassembly,
    instruction: &InstructionLine,
) -> Result<Instruction, InstructionError> {
    let text = &disassembly.source;
    let error = |kind, span: Span| InstructionError {
        source_text: NamedSource::new("llvm-objdump output", text.clone()),
        span: span.into(),
        kind,
    };
    let mnemonic = &text[instruction.mnemonic_span()];
    if mnemonic != "v_cmp_eq_u32_e32" {
        return Err(error(
            InstructionErrorKind::UnsupportedInstruction {
                mnemonic: mnemonic.into(),
            },
            instruction.mnemonic_span(),
        ));
    }
    let [destination, comma1, lhs, comma2, rhs] = instruction.operand_tokens() else {
        return Err(error(
            InstructionErrorKind::OperandFormat,
            instruction.operands_span(),
        ));
    };
    for comma in [comma1, comma2] {
        if comma.value != Token::Comma {
            return Err(error(
                InstructionErrorKind::OperandFormat,
                comma.span.clone(),
            ));
        }
    }
    if &text[destination.span.clone()] != "vcc_lo" {
        return Err(error(
            InstructionErrorKind::MaskRegister,
            destination.span.clone(),
        ));
    }
    let lhs_text = &text[lhs.span.clone()];
    let lhs_value = VectorRegister::parse(lhs_text)
        .map(VectorSource::Register)
        .or_else(|| immediate(lhs_text).map(VectorSource::Immediate))
        .ok_or_else(|| error(InstructionErrorKind::SourceOperand, lhs.span.clone()))?;
    let rhs_value = VectorRegister::parse(&text[rhs.span.clone()])
        .ok_or_else(|| error(InstructionErrorKind::VectorRegister, rhs.span.clone()))?;
    Ok(Instruction::VCmpEqU32 {
        span: instruction.span.clone(),
        address: Spanned {
            value: instruction.address.value,
            span: instruction.address.span.clone(),
        },
        destination: Spanned {
            value: MaskRegister::VccLo,
            span: destination.span.clone(),
        },
        lhs: Spanned {
            value: lhs_value,
            span: lhs.span.clone(),
        },
        rhs: Spanned {
            value: rhs_value,
            span: rhs.span.clone(),
        },
    })
}

pub fn from_disassembly(disassembly: &Disassembly) -> Result<Vec<Instruction>, Error> {
    let instruction = disassembly
        .instructions
        .iter()
        .find(|instruction| &disassembly.source[instruction.mnemonic_span()] == "v_cmp_eq_u32_e32")
        .ok_or(Error::NoCompare)?;
    Ok(vec![parse_instruction(disassembly, instruction)?])
}

#[derive(Debug, Error, Diagnostic)]
#[error("cannot parse instruction")]
#[diagnostic(
    code(rocky::instruction),
    help(
        "Supported form: v_cmp_eq_u32_e32 vcc_lo, <vector register or integer>, <vector register>. Only wave32 comparisons are supported."
    )
)]
pub struct InstructionError {
    #[source_code]
    source_text: NamedSource<String>,
    #[label("{kind}")]
    span: SourceSpan,
    #[source]
    kind: InstructionErrorKind,
}

#[derive(Debug, Error)]
enum InstructionErrorKind {
    #[error("unsupported instruction {mnemonic}")]
    UnsupportedInstruction { mnemonic: String },
    #[error("expected three operands separated by commas")]
    OperandFormat,
    #[error("expected wave32 mask register vcc_lo")]
    MaskRegister,
    #[error("expected numbered vector register v0 through v255")]
    VectorRegister,
    #[error("expected numbered vector register or 32-bit decimal/hex integer")]
    SourceOperand,
}
