use std::num::ParseIntError;

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use super::lexer::{Span, Spanned, Token, tokenize};

#[derive(Debug, PartialEq, Eq)]
pub struct InstructionLine {
    pub span: Span,
    pub address: Spanned<u64>,
    pub words: Vec<Spanned<u32>>,
    pub tokens: Vec<Spanned<Token>>,
    assembly_len: usize,
}

impl InstructionLine {
    pub fn size_bytes(&self) -> usize {
        self.words.len() * 4
    }

    pub fn mnemonic_span(&self) -> Span {
        self.tokens[0].span.clone()
    }

    pub fn operand_tokens(&self) -> &[Spanned<Token>] {
        &self.tokens[1..self.assembly_len]
    }

    pub fn operands_span(&self) -> Span {
        match (self.operand_tokens().first(), self.operand_tokens().last()) {
            (Some(first), Some(last)) => first.span.start..last.span.end,
            _ => self.tokens[0].span.end..self.tokens[0].span.end,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Symbol {
    pub span: Span,
    pub name: Span,
    pub address: Spanned<u64>,
}

#[derive(Debug)]
pub struct Disassembly {
    pub source: String,
    pub symbols: Vec<Symbol>,
    pub instructions: Vec<InstructionLine>,
}

fn parse_instruction(
    text: &str,
    tokens: Vec<Spanned<Token>>,
    span: Span,
    line_number: usize,
) -> Result<InstructionLine, ParseError> {
    let error = |kind, error_span: Span| ParseError {
        line_number,
        source_text: NamedSource::new("llvm-objdump output", text.to_owned()),
        span: error_span.into(),
        kind,
    };
    let separator = tokens
        .iter()
        .position(|token| token.value == Token::EncodingSeparator)
        .ok_or_else(|| error(ParseErrorKind::MissingEncoding, span.clone()))?;
    if separator == 0 {
        return Err(error(
            ParseErrorKind::MissingMnemonic,
            tokens[0].span.clone(),
        ));
    }
    let encoding = &tokens[separator + 1..];
    let address_token = encoding
        .first()
        .ok_or_else(|| error(ParseErrorKind::MissingEncoding, span.end..span.end))?;
    if encoding.get(1).map(|token| token.value) != Some(Token::Colon) {
        return Err(error(
            ParseErrorKind::MissingAddressSeparator,
            address_token.span.clone(),
        ));
    }
    let value = &text[address_token.span.clone()];
    let address = Spanned {
        value: u64::from_str_radix(value, 16).map_err(|source| {
            error(
                ParseErrorKind::InstructionAddress {
                    value: value.into(),
                    source,
                },
                address_token.span.clone(),
            )
        })?,
        span: address_token.span.clone(),
    };
    let words = encoding[2..]
        .iter()
        .take_while(|token| token.value != Token::Less)
        .map(|token| {
            let word = &text[token.span.clone()];
            if word.len() != 8 {
                return Err(error(
                    ParseErrorKind::WordWidth { word: word.into() },
                    token.span.clone(),
                ));
            }
            let value = u32::from_str_radix(word, 16).map_err(|source| {
                error(
                    ParseErrorKind::InstructionWord {
                        word: word.into(),
                        source,
                    },
                    token.span.clone(),
                )
            })?;
            Ok(Spanned {
                value,
                span: token.span.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if words.is_empty() {
        return Err(error(ParseErrorKind::MissingEncoding, span.end..span.end));
    }
    Ok(InstructionLine {
        span,
        address,
        words,
        tokens,
        assembly_len: separator,
    })
}

pub fn parse(source: String) -> Result<Disassembly, ParseError> {
    let mut symbols = Vec::new();
    let mut instructions = Vec::new();
    let mut offset = 0;
    for (index, raw_line) in source.split_inclusive('\n').enumerate() {
        let start = offset + raw_line.len() - raw_line.trim_start().len();
        offset += raw_line.len();
        let line = raw_line.trim();
        let span = start..start + line.len();
        if let Some((address, name)) = line.split_once(" <")
            && let Some(name) = name.strip_suffix(">:")
        {
            let address_span = start..start + address.len();
            let value = u64::from_str_radix(address, 16).map_err(|cause| ParseError {
                line_number: index + 1,
                source_text: NamedSource::new("llvm-objdump output", source.clone()),
                span: address_span.clone().into(),
                kind: ParseErrorKind::SymbolAddress {
                    value: address.into(),
                    source: cause,
                },
            })?;
            let name_start = address_span.end + 2;
            symbols.push(Symbol {
                span,
                name: name_start..name_start + name.len(),
                address: Spanned {
                    value,
                    span: address_span,
                },
            });
        } else if line.contains("//") {
            let tokens = tokenize(line, start).map_err(|span| ParseError {
                line_number: index + 1,
                source_text: NamedSource::new("llvm-objdump output", source.clone()),
                span: span.into(),
                kind: ParseErrorKind::UnexpectedCharacter,
            })?;
            let instruction = parse_instruction(&source, tokens, span, index + 1)?;
            if &source[instruction.mnemonic_span()] != "s_code_end" {
                instructions.push(instruction);
            }
        }
    }
    Ok(Disassembly {
        source,
        symbols,
        instructions,
    })
}

#[derive(Debug, Error, Diagnostic)]
#[error("invalid disassembly on line {line_number}")]
#[diagnostic(
    code(rocky::disassembly_format),
    help("Expected 'MNEMONIC OPERANDS // HEX_ADDRESS: WORD [WORD ...]' or a symbol header.")
)]
pub struct ParseError {
    pub line_number: usize,
    #[source_code]
    pub source_text: NamedSource<String>,
    #[label("{kind}")]
    pub span: SourceSpan,
    #[source]
    pub kind: ParseErrorKind,
}

#[derive(Debug, Error)]
pub enum ParseErrorKind {
    #[error("unexpected character")]
    UnexpectedCharacter,
    #[error("invalid symbol address {value:?}: {source}")]
    SymbolAddress {
        value: String,
        #[source]
        source: ParseIntError,
    },
    #[error("missing instruction address separator")]
    MissingAddressSeparator,
    #[error("invalid instruction address {value:?}: {source}")]
    InstructionAddress {
        value: String,
        #[source]
        source: ParseIntError,
    },
    #[error("expected 32-bit instruction word, found {word:?}")]
    WordWidth { word: String },
    #[error("invalid instruction word {word:?}: {source}")]
    InstructionWord {
        word: String,
        #[source]
        source: ParseIntError,
    },
    #[error("missing instruction encoding")]
    MissingEncoding,
    #[error("missing instruction mnemonic")]
    MissingMnemonic,
}
