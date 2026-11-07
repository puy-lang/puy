//! CST node kinds and the rowan tree types (`docs/spec/grammar.md` chapter 2).

pub mod ast;
mod kind;

pub use kind::SyntaxKind;

/// rowan `Language`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PuyLanguage {}

impl rowan::Language for PuyLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_raw(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<PuyLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<PuyLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<PuyLanguage>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<PuyLanguage>;
