use logos::Logos;

use crate::span::{Location, Source, Span};

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
#[logos(skip r"\s+")]
pub enum Token {
    #[regex(r"[A-Za-z0-9_.$-]+")]
    Atom,
    #[token("//")]
    EncodingSeparator,
    #[token(":")]
    Colon,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,
    #[token(",")]
    Comma,
    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,
    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[regex(r"[^\sA-Za-z0-9_.$\-:<>,\[\]()]", priority = 0)]
    Punctuation,
}

pub fn tokenize(text: &str, offset: usize, source: &Source) -> Result<Vec<Span<Token>>, Location> {
    Token::lexer(text)
        .spanned()
        .map(|(token, span)| {
            let span = Location {
                source: source.clone(),
                range: span.start + offset..span.end + offset,
            };
            token
                .map(|value| Span {
                    value,
                    span: span.clone(),
                })
                .map_err(|_| span)
        })
        .collect()
}
