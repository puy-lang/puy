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

/// The name an identifier stands for: its spelling without the leading `&` that lets a reserved
/// word be used as an identifier (`docs/spec/lexical.md` 3.2). Other text is returned as is.
pub fn ident_name(text: &str) -> &str {
    match text.strip_prefix('&') {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') => rest,
        _ => text,
    }
}

/// Whether an identifier is written with the `&` that escapes a reserved word or name
/// (`docs/spec/lexical.md` 3.2).
pub fn is_escaped_ident(text: &str) -> bool {
    ident_name(text).len() != text.len()
}

/// [`ident_name`] for a token.
pub trait TokenName {
    /// The name the token stands for (its text without the escaping `&`).
    fn name(&self) -> &str;
    /// The name with the token's position (a part of a qualified name, a type parameter).
    fn name_and_range(&self) -> (String, rowan::TextRange);
}

impl TokenName for SyntaxToken {
    fn name(&self) -> &str {
        ident_name(self.text())
    }
    fn name_and_range(&self) -> (String, rowan::TextRange) {
        (self.name().to_string(), self.text_range())
    }
}

// ---- Searches over the green tree ----
//
// Walking a subtree with the cursor API (`descendants`, `descendants_with_tokens`) allocates a
// cursor for every node and token it passes. These searches walk the green tree instead, and
// make cursors only for what they return.

fn green_kind(raw: rowan::SyntaxKind) -> SyntaxKind {
    SyntaxKind::from_raw(raw.0)
}

/// Whether the node or one of its descendants is a node of `kind` (as
/// `node.descendants().any(|n| n.kind() == kind)`).
pub fn has_node_of_kind(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    fn walk(green: &rowan::GreenNodeData, kind: SyntaxKind) -> bool {
        green.children().any(|c| match c {
            rowan::NodeOrToken::Node(n) => green_kind(n.kind()) == kind || walk(n, kind),
            rowan::NodeOrToken::Token(_) => false,
        })
    }
    node.kind() == kind || walk(&node.green(), kind)
}

/// The node and its descendants that are nodes of `kind`, in preorder (as
/// `node.descendants().filter(|n| n.kind() == kind)`).
pub fn nodes_of_kind(node: &SyntaxNode, kind: SyntaxKind) -> Vec<SyntaxNode> {
    /// `path` holds the indexes among the child nodes from `root` to `green`.
    fn walk(
        root: &SyntaxNode,
        green: &rowan::GreenNodeData,
        path: &mut Vec<usize>,
        kind: SyntaxKind,
        out: &mut Vec<SyntaxNode>,
    ) {
        let child_nodes = green.children().filter_map(|c| c.into_node());
        for (i, n) in child_nodes.enumerate() {
            path.push(i);
            if green_kind(n.kind()) == kind {
                let mut cur = root.clone();
                for &j in path.iter() {
                    cur = cur
                        .children()
                        .nth(j)
                        .expect("the path follows the green tree of the same node");
                }
                out.push(cur);
            }
            walk(root, n, path, kind, out);
            path.pop();
        }
    }
    let mut out = Vec::new();
    if node.kind() == kind {
        out.push(node.clone());
    }
    walk(node, &node.green(), &mut Vec::new(), kind, &mut out);
    out
}

/// The range from the start of the node's first non-trivia token to the end of its last, or
/// `None` when it has only trivia (or no tokens).
pub(crate) fn non_trivia_range(node: &SyntaxNode) -> Option<rowan::TextRange> {
    fn first_start(
        green: &rowan::GreenNodeData,
        mut at: rowan::TextSize,
    ) -> Option<rowan::TextSize> {
        for c in green.children() {
            match c {
                rowan::NodeOrToken::Token(t) if !green_kind(t.kind()).is_trivia() => {
                    return Some(at);
                }
                rowan::NodeOrToken::Token(_) => {}
                rowan::NodeOrToken::Node(n) => {
                    if let Some(s) = first_start(n, at) {
                        return Some(s);
                    }
                }
            }
            at += c.text_len();
        }
        None
    }
    fn last_end(green: &rowan::GreenNodeData, mut end: rowan::TextSize) -> Option<rowan::TextSize> {
        for c in green.children().rev() {
            match c {
                rowan::NodeOrToken::Token(t) if !green_kind(t.kind()).is_trivia() => {
                    return Some(end);
                }
                rowan::NodeOrToken::Token(_) => {}
                rowan::NodeOrToken::Node(n) => {
                    if let Some(e) = last_end(n, end) {
                        return Some(e);
                    }
                }
            }
            end -= c.text_len();
        }
        None
    }
    let range = node.text_range();
    let green = node.green();
    let start = first_start(&green, range.start())?;
    let end = last_end(&green, range.end()).expect("a node with a non-trivia token has a last one");
    Some(rowan::TextRange::new(start, end))
}
