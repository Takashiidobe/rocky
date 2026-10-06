use std::num::ParseIntError;

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use super::lexer::{Token, tokenize};
use crate::span::{Location, Source, Span};

#[derive(Debug, PartialEq, Eq)]
pub struct InstructionLine {
    pub address: Span<u64>,
    pub words: Vec<Span<u32>>,
    pub tokens: Vec<Span<Token>>,
    assembly_len: usize,
}

impl InstructionLine {
    pub fn size_bytes(&self) -> usize {
        self.words.len() * 4
    }

    pub fn assembly_span(&self) -> Location {
        self.tokens[0]
            .span
            .through(&self.tokens[self.assembly_len - 1].span)
    }

    pub fn mnemonic_span(&self) -> Location {
        self.tokens[0].span.clone()
    }

    pub fn operand_tokens(&self) -> &[Span<Token>] {
        &self.tokens[1..self.assembly_len]
    }

    pub fn operands_span(&self) -> Location {
        match (self.operand_tokens().first(), self.operand_tokens().last()) {
            (Some(first), Some(last)) => first.span.through(&last.span),
            _ => self.tokens[0].span.end(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name: Location,
    pub address: Span<u64>,
}

#[derive(Debug)]
pub struct Disassembly {
    pub symbols: Vec<Span<Symbol>>,
    pub instructions: Vec<Span<InstructionLine>>,
}

fn parse_instruction(
    tokens: Vec<Span<Token>>,
    span: Location,
    line_number: usize,
) -> Result<Span<InstructionLine>, ParseError> {
    let error = |kind, error_span: Location| ParseError {
        line_number,
        source_text: error_span.source.clone(),
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
        .ok_or_else(|| error(ParseErrorKind::MissingEncoding, span.end()))?;
    if encoding.get(1).map(|token| token.value) != Some(Token::Colon) {
        return Err(error(
            ParseErrorKind::MissingAddressSeparator,
            address_token.span.clone(),
        ));
    }
    let value = address_token.span.text();
    let address = Span {
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
            let word = token.span.text();
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
            Ok(Span {
                value,
                span: token.span.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if words.is_empty() {
        return Err(error(ParseErrorKind::MissingEncoding, span.end()));
    }
    Ok(Span {
        span,
        value: InstructionLine {
            address,
            words,
            tokens,
            assembly_len: separator,
        },
    })
}

pub fn parse(source: Source) -> Result<Disassembly, ParseError> {
    let mut symbols = Vec::new();
    let mut instructions = Vec::new();
    let mut offset = 0;
    for (index, raw_line) in source.inner().split_inclusive('\n').enumerate() {
        let start = offset + raw_line.len() - raw_line.trim_start().len();
        offset += raw_line.len();
        let line = raw_line.trim();
        let span = Location {
            source: source.clone(),
            range: start..start + line.len(),
        };
        if let Some((address, name)) = line.split_once(" <")
            && let Some(name) = name.strip_suffix(">:")
        {
            let address_span = Location {
                source: source.clone(),
                range: start..start + address.len(),
            };
            let value = u64::from_str_radix(address, 16).map_err(|cause| ParseError {
                line_number: index + 1,
                source_text: source.clone(),
                span: address_span.clone().into(),
                kind: ParseErrorKind::SymbolAddress {
                    value: address.into(),
                    source: cause,
                },
            })?;
            let name_start = address_span.range.end + 2;
            symbols.push(Span {
                span,
                value: Symbol {
                    name: Location {
                        source: source.clone(),
                        range: name_start..name_start + name.len(),
                    },
                    address: Span {
                        value,
                        span: address_span,
                    },
                },
            });
        } else if line.contains("//") {
            let tokens = tokenize(line, start, &source).map_err(|span| ParseError {
                line_number: index + 1,
                source_text: source.clone(),
                span: span.into(),
                kind: ParseErrorKind::UnexpectedCharacter,
            })?;
            let instruction = parse_instruction(tokens, span, index + 1)?;
            instructions.push(instruction);
        }
    }
    Ok(Disassembly {
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
    pub source_text: Source,
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
