use std::ops::Range;

use logos::Logos;

pub type Span = Range<usize>;

#[derive(Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

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

pub fn tokenize(text: &str, offset: usize) -> Result<Vec<Spanned<Token>>, Span> {
    Token::lexer(text)
        .spanned()
        .map(|(token, span)| {
            let span = span.start + offset..span.end + offset;
            token
                .map(|value| Spanned {
                    value,
                    span: span.clone(),
                })
                .map_err(|_| span)
        })
        .collect()
}
