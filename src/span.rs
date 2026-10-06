use std::ops::{Deref, Range};
use std::sync::Arc;

use miette::{NamedSource, SourceSpan};

pub type Source = Arc<NamedSource<String>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub source: Source,
    pub range: Range<usize>,
}

impl Location {
    pub fn text(&self) -> &str {
        &self.source.inner()[self.range.clone()]
    }

    pub fn through(&self, last: &Self) -> Self {
        assert!(Arc::ptr_eq(&self.source, &last.source));
        Self {
            source: self.source.clone(),
            range: self.range.start..last.range.end,
        }
    }

    pub fn end(&self) -> Self {
        Self {
            source: self.source.clone(),
            range: self.range.end..self.range.end,
        }
    }
}

impl From<Location> for SourceSpan {
    fn from(location: Location) -> Self {
        location.range.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span<T> {
    pub value: T,
    pub span: Location,
}

impl<T> Span<T> {
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Deref for Span<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}
