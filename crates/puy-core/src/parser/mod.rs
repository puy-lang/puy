//! Parser (`docs/spec/grammar.md`). Performs error recovery and reports multiple errors together.
//!
//! - Builds a lossless CST (rowan). Trivia between token X and the next token Y goes under their lowest common ancestor
//!   (2.3). In the implementation, trivia is first emitted when a node is started or a token is consumed.
//! - Lexical `ERROR` tokens are treated like trivia (chapter 3).
//! - Expressions use Pratt parsing. The nesting limit is 256 (E0413).

mod recovery;

use rowan::{Checkpoint, GreenNodeBuilder};

use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Fix, Severity, Stage, TextEdit};
use crate::directives::{DirectiveOutput, Placement};
use crate::lexer::{self, Token};
use crate::source::{FileId, Span, TextRange, TextSize};
use crate::syntax::{SyntaxKind, SyntaxNode};
use SyntaxKind::*;

/// Parser input.
pub struct ParseInput<'a> {
    pub file: FileId,
    pub bytes: &'a [u8],
    pub tokens: &'a [Token],
    pub directives: &'a DirectiveOutput,
    /// A file stopped on E0001 (not parsed).
    pub invalid_utf8: bool,
}

/// Parser result.
pub struct ParseOutput {
    pub root: SyntaxNode,
}

/// Nesting limit (chapter 3).
pub const MAX_NESTING: usize = 256;

pub fn parse(input: &ParseInput<'_>, sink: &mut DiagnosticSink) -> ParseOutput {
    let mut p = Parser {
        file: input.file,
        bytes: input.bytes,
        tokens: input.tokens,
        pos: 0,
        last_end: 0,
        builder: GreenNodeBuilder::new(),
        diags: Vec::new(),
        directives: input.directives,
        depth: 0,
        nesting_reported: false,
        open_blocks: Vec::new(),
        closing_all: false,
        in_program_decls: false,
        routine_depth: 0,
        routines_too_deep: false,
        bodyless: Vec::new(),
        last_was_hole: false,
        diag_found_at: Vec::new(),
    };
    if input.invalid_utf8 {
        p.parse_invalid_utf8_file();
    } else {
        p.parse_source_file();
    }
    let green = std::mem::replace(&mut p.builder, GreenNodeBuilder::new()).finish();
    let root = SyntaxNode::new_root(green);
    let mut diags = std::mem::take(&mut p.diags);
    let mut found_at = std::mem::take(&mut p.diag_found_at);
    debug_assert_eq!(diags.len(), found_at.len());
    for d in p.post_pass(&root) {
        let at = d.primary.as_ref().map(|l| l.span.range.start());
        diags.push(d);
        found_at.push(at);
    }
    // When everything up to the end of the file was skipped at the nested-routine limit, do not cascade diagnostics for
    // missing tokens (`begin`, `end`, `.`) at the end of the outer body or program (`grammar.md` chapter 3)
    let eof_start = TextSize::from(input.tokens.last().map_or(0, |t| t.start()));
    let routines_too_deep = p.routines_too_deep;
    for (d, at) in diags.into_iter().zip(found_at) {
        if routines_too_deep && at == Some(eof_start) && d.code != Some(Code::E(413)) {
            continue;
        }
        // When `INACTIVE` from E0203 runs to the end of the file, suppress syntax errors after its start
        // (`grammar.md` chapter 3). The `primary` of a diagnostic that supplied a token is the end of the previous token,
        // so also check the position of the token that was found (`EOF`, etc.).
        if let Some(limit) = input.directives.suppress_after
            && d.severity == Severity::Error
            && (at.is_some_and(|a| a >= limit)
                || d.primary
                    .as_ref()
                    .is_some_and(|l| l.span.range.start() >= limit))
        {
            continue;
        }
        sink.push(d);
    }
    ParseOutput { root }
}

/// `cst` dump (chapter 5).
pub fn dump_cst(root: &SyntaxNode) -> String {
    let mut out = String::new();
    dump_node(root, 0, &mut out);
    out
}

fn dump_node(node: &SyntaxNode, indent: usize, out: &mut String) {
    use rowan::NodeOrToken;
    out.push_str(&format!(
        "{}{}@{}..{}\n",
        "  ".repeat(indent),
        node.kind().name(),
        u32::from(node.text_range().start()),
        u32::from(node.text_range().end())
    ));
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => dump_node(&n, indent + 1, out),
            NodeOrToken::Token(t) => {
                out.push_str(&format!(
                    "{}{}@{}..{} {}\n",
                    "  ".repeat(indent + 1),
                    t.kind().name(),
                    u32::from(t.text_range().start()),
                    u32::from(t.text_range().end()),
                    lexer::debug_text(t.text().as_bytes())
                ));
            }
        }
    }
}

struct Parser<'a> {
    file: FileId,
    bytes: &'a [u8],
    tokens: &'a [Token],
    /// Index of the next token to read (may point at trivia).
    pos: usize,
    /// End of the last consumed (non-trivia) token.
    last_end: u32,
    builder: GreenNodeBuilder<'static>,
    diags: Vec<Diagnostic>,
    directives: &'a DirectiveOutput,
    depth: usize,
    /// The limit overrun has been reported (not repeated at the same depth).
    nesting_reported: bool,
    /// Open blocks (`begin` / `repeat`). Used for diagnostics on unclosed blocks.
    open_blocks: Vec<(SyntaxKind, TextRange)>,
    /// In the middle of closing all blocks at `.` / `EOF`.
    closing_all: bool,
    /// Reading the declaration part of a `program` (guesses a missing `begin`).
    in_program_decls: bool,
    /// Number of routine bodies currently being read (used for E3200 on nested routines and the limit check).
    routine_depth: usize,
    /// Skipped to the end of the file at the nested-routine limit (E0413).
    routines_too_deep: bool,
    /// `ROUTINE_DECL`s with neither a body nor `forward` (to decide the E0411 fix).
    bodyless: Vec<BodylessDecl>,
    /// The last consumed item was a fill-in for a lexical error (a token supplied right after it gets no diagnostic).
    last_was_hole: bool,
    /// Start position of the current token when each diagnostic in `diags` was issued (used to suppress after E0203).
    diag_found_at: Vec<Option<TextSize>>,
}

struct BodylessDecl {
    /// Range of the `ROUTINE_DECL`.
    range: TextRange,
    /// Name (lowercase).
    name: String,
    /// Position of the name.
    name_range: TextRange,
    /// Right after the last `;` (where ` forward;` is inserted).
    insert_at: TextSize,
    /// Whether it is at the top level of a script.
    in_script: bool,
}

/// Synchronizing tokens (chapter 3).
fn is_sync(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SEMICOLON
            | END_KW
            | BEGIN_KW
            | UNTIL_KW
            | ELSE_KW
            | DOT
            | EOF
            | CONST_KW
            | VAR_KW
            | TYPE_KW
            | PROCEDURE_KW
            | FUNCTION_KW
            | IF_KW
            | WHILE_KW
            | REPEAT_KW
            | FOR_KW
            | USES_KW
            | PROGRAM_KW
            | UNIT_KW
    )
}

/// Start of a statement (as defined in chapter 3).
fn is_stmt_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        IDENT
            | BEGIN_KW
            | IF_KW
            | WHILE_KW
            | REPEAT_KW
            | FOR_KW
            | VAR_KW
            | CONST_KW
            | INHERITED_KW
            | CASE_KW
            | TRY_KW
            | RAISE_KW
            | WITH_KW
            | GOTO_KW
            | ASM_KW
    )
}

/// Tokens that can start an expression.
fn can_start_expr(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        IDENT
            | STRING_KW
            | INT_LIT
            | FLOAT_LIT
            | STRING_LIT
            | NIL_KW
            | L_PAREN
            | INHERITED_KW
            | NOT_KW
            | MINUS
            | PLUS
            | AT
            | L_BRACK
    )
}

/// Start of a declaration (including M2-and-later and removed sections).
fn is_decl_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        CONST_KW
            | VAR_KW
            | PROCEDURE_KW
            | FUNCTION_KW
            | TYPE_KW
            | LABEL_KW
            | RESOURCESTRING_KW
            | THREADVAR_KW
            | EXPORTS_KW
            | CONSTRUCTOR_KW
            | DESTRUCTOR_KW
            | CLASS_KW
            | PROPERTY_KW
    )
}

/// Names of directives that can be attached to routines (1.3).
fn routine_directive_kind(lower: &str) -> Option<SyntaxKind> {
    Some(match lower {
        "forward" => FORWARD_KW,
        "overload" => OVERLOAD_KW,
        "deprecated" => DEPRECATED_KW,
        "experimental" => EXPERIMENTAL_KW,
        "platform" => PLATFORM_KW,
        "external" => EXTERNAL_KW,
        "varargs" => VARARGS_KW,
        "cdecl" => CDECL_KW,
        "stdcall" => STDCALL_KW,
        "safecall" => SAFECALL_KW,
        // `assembler` is treated like `asm` (E3008), so re-tag it as `ASM_KW`
        "assembler" => ASM_KW,
        "register" => REGISTER_KW,
        "pascal" => PASCAL_KW,
        "winapi" => WINAPI_KW,
        "export" => EXPORT_KW,
        "near" => NEAR_KW,
        "far" => FAR_KW,
        "local" => LOCAL_KW,
        _ => return None,
    })
}

fn binary_prec(kind: SyntaxKind) -> Option<u8> {
    Some(match kind {
        EQ | NE | LT | GT | LE | GE | IN_KW | IS_KW => 1,
        PLUS | MINUS | OR_KW | XOR_KW => 2,
        STAR | SLASH | DIV_KW | MOD_KW | AND_KW | SHL_KW | SHR_KW | AS_KW => 3,
        _ => return None,
    })
}

impl<'a> Parser<'a> {
    // ---- Reading tokens ----

    fn index_of_nth(&self, n: usize) -> usize {
        let mut i = self.pos;
        let mut seen = 0;
        loop {
            let k = self.tokens[i].kind;
            if k == EOF {
                return i;
            }
            if !k.is_skipped_by_parser() {
                if seen == n {
                    return i;
                }
                seen += 1;
            }
            i += 1;
        }
    }

    fn current(&self) -> SyntaxKind {
        self.tokens[self.index_of_nth(0)].kind
    }

    fn nth(&self, n: usize) -> SyntaxKind {
        self.tokens[self.index_of_nth(n)].kind
    }

    fn current_tok(&self) -> Token {
        self.tokens[self.index_of_nth(0)]
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    fn at_eof(&self) -> bool {
        self.current() == EOF
    }

    fn text_of(&self, tok: Token) -> &'a str {
        std::str::from_utf8(&self.bytes[tok.start() as usize..tok.end() as usize]).unwrap_or("")
    }

    /// Spelling of the current token (lowercase).
    fn current_lower(&self) -> String {
        self.text_of(self.current_tok()).to_ascii_lowercase()
    }

    /// Whether the current token is this identifier (case-insensitive).
    fn at_ident(&self, lower: &str) -> bool {
        self.at(IDENT) && self.current_lower() == lower
    }

    fn span(&self, range: TextRange) -> Span {
        Span::new(self.file, range)
    }

    /// Emits trivia not yet emitted.
    fn emit_pending(&mut self) {
        let upto = self.index_of_nth(0);
        while self.pos < upto {
            let t = self.tokens[self.pos];
            self.emit_token(t, t.kind);
            self.pos += 1;
        }
    }

    fn emit_token(&mut self, tok: Token, kind: SyntaxKind) {
        let text = std::str::from_utf8(&self.bytes[tok.start() as usize..tok.end() as usize]);
        // Invalid UTF-8 only occurs in E0001 files, which are handled on a separate path
        let text = text.unwrap_or("");
        self.builder.token(kind.into(), text);
    }

    fn bump(&mut self) {
        let kind = self.current();
        self.bump_as(kind);
    }

    /// Consumes the current token, re-tagging its kind (context-sensitive directives).
    fn bump_as(&mut self, kind: SyntaxKind) {
        self.emit_pending();
        let tok = self.tokens[self.pos];
        debug_assert!(!tok.kind.is_skipped_by_parser());
        self.emit_token(tok, kind);
        self.last_end = tok.end();
        self.last_was_hole = false;
        if tok.kind != EOF {
            self.pos += 1;
        }
    }

    fn bump_eof(&mut self) {
        self.emit_pending();
        let tok = self.tokens[self.pos];
        debug_assert_eq!(tok.kind, EOF);
        self.emit_token(tok, EOF);
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        self.emit_pending();
        self.builder.start_node(kind.into());
    }

    /// Starts a node without flushing preceding lexical `ERROR` tokens. The `ERROR` tokens stay inside the
    /// node, and `parse_expr_or_missing` turns them into a fill-in `ERROR` node.
    fn start_node_keeping_lex_error(&mut self, kind: SyntaxKind) {
        let upto = self.index_of_nth(0);
        let first = (self.pos..upto)
            .find(|&i| self.tokens[i].kind == ERROR)
            .unwrap_or(upto);
        while self.pos < first {
            let t = self.tokens[self.pos];
            self.emit_token(t, t.kind);
            self.pos += 1;
        }
        self.builder.start_node(kind.into());
    }

    fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    fn checkpoint(&mut self) -> Checkpoint {
        self.emit_pending();
        self.builder.checkpoint()
    }

    /// Places a checkpoint without flushing preceding lexical `ERROR` tokens. Used by `parse_simple_stmt` to put
    /// a fill-in at the start of a statement (the `é` in `é := 1;`) inside the statement node.
    fn checkpoint_keeping_lex_error(&mut self) -> Checkpoint {
        let upto = self.index_of_nth(0);
        let first = (self.pos..upto)
            .find(|&i| self.tokens[i].kind == ERROR)
            .unwrap_or(upto);
        while self.pos < first {
            let t = self.tokens[self.pos];
            self.emit_token(t, t.kind);
            self.pos += 1;
        }
        self.builder.checkpoint()
    }

    fn start_node_at(&mut self, cp: Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(cp, kind.into());
    }

    /// Whether there is a lexical `ERROR` token before the current position.
    fn pending_has_lex_error(&self) -> bool {
        let upto = self.index_of_nth(0);
        self.tokens[self.pos..upto].iter().any(|t| t.kind == ERROR)
    }

    /// Whether, where an expression start is required, the preceding lexical `ERROR` token becomes a fill-in (`grammar.md` chapter 3).
    /// If the `ERROR` token has a literal shape (`0x1F`, `0then`, `$FFG`, `'abc`, `#`), it is always a fill-in.
    /// Otherwise it becomes a fill-in only when the next token cannot start an expression, or when there is a newline
    /// between the `ERROR` token and the next token; when the expression continues on the same line (the `G` in `$G`),
    /// that expression is read normally.
    fn lex_hole_wanted(&self) -> bool {
        if !self.pending_has_lex_error() {
            return false;
        }
        let upto = self.index_of_nth(0);
        let errors = (self.pos..upto).filter(|&i| self.tokens[i].kind == ERROR);
        let mut last = self.pos;
        for i in errors {
            last = i;
            if self.lex_error_is_literal_like(self.tokens[i]) {
                return true;
            }
        }
        if !can_start_expr(self.current()) {
            return true;
        }
        self.tokens[last..upto].iter().any(|t| t.kind == NEWLINE)
    }

    /// Whether a lexical `ERROR` token has the shape of a broken literal (starts with a digit, a digit right after `$` / `%`,
    /// or starts with `'` or `#`).
    fn lex_error_is_literal_like(&self, tok: Token) -> bool {
        let bytes = &self.bytes[tok.start() as usize..tok.end() as usize];
        match bytes.first() {
            Some(b'0'..=b'9' | b'\'' | b'#') => true,
            Some(b'$') => bytes.get(1).is_some_and(|b| b.is_ascii_hexdigit()),
            Some(b'%') => bytes.get(1).is_some_and(|b| matches!(b, b'0' | b'1')),
            _ => false,
        }
    }

    /// Turns lexical `ERROR` tokens into a fill-in expression of the error type (an `ERROR` node).
    fn lex_error_hole(&mut self) {
        let upto = self.index_of_nth(0);
        let Some(first) = (self.pos..upto).find(|&i| self.tokens[i].kind == ERROR) else {
            return;
        };
        while self.pos < first {
            let t = self.tokens[self.pos];
            self.emit_token(t, t.kind);
            self.pos += 1;
        }
        self.builder.start_node(ERROR_NODE.into());
        while self.pos < upto && self.tokens[self.pos].kind == ERROR {
            let t = self.tokens[self.pos];
            self.emit_token(t, t.kind);
            self.last_end = t.end();
            self.pos += 1;
        }
        self.builder.finish_node();
        self.last_was_hole = true;
    }

    // ---- Diagnostics ----

    fn push_diag(&mut self, d: Diagnostic) {
        self.diag_found_at
            .push(Some(TextSize::from(self.current_tok().start())));
        self.diags.push(d);
    }

    fn error_at(
        &mut self,
        code: Code,
        msg: impl Into<String>,
        range: TextRange,
    ) -> &mut Diagnostic {
        let d = Diagnostic::error(code, msg, Stage::Parser).with_primary(self.span(range), "");
        self.diag_found_at
            .push(Some(TextSize::from(self.current_tok().start())));
        self.diags.push(d);
        self.diags.last_mut().unwrap()
    }

    fn found_text(&self, tok: Token) -> String {
        if tok.kind == EOF {
            "end of file".to_string()
        } else {
            format!("`{}`", self.text_of(tok))
        }
    }

    /// Insertion string for a supplied token (the whitespace rule in chapter 3).
    fn insertion_text(&self, at: u32, text: &str) -> String {
        if !text.bytes().next().is_some_and(|b| b.is_ascii_alphabetic()) {
            return text.to_string();
        }
        // Alphabetic words (`then`, `begin`, `end`, etc.) get one space added unless whitespace surrounds them,
        // but not when followed by `;` / `.` / `)` / `,` / `]` (`grammar.md` chapter 3)
        let before = at > 0 && !self.bytes[at as usize - 1].is_ascii_whitespace();
        let after = (at as usize) < self.bytes.len() && {
            let b = self.bytes[at as usize];
            !b.is_ascii_whitespace() && !matches!(b, b';' | b'.' | b')' | b',' | b']')
        };
        format!(
            "{}{}{}",
            if before { " " } else { "" },
            text,
            if after { " " } else { "" }
        )
    }

    /// E0400 for a supplied token (primary is the insertion point, secondary is the token found, the fix is the insertion).
    fn supply_diag(&mut self, code: Code, msg: String, insert: Option<&str>) {
        if self.pending_has_lex_error() || self.last_was_hole {
            return;
        }
        let at = self.last_end;
        let found = self.current_tok();
        let mut d = Diagnostic::error(code, msg, Stage::Parser)
            .with_primary(self.span(TextRange::empty(at.into())), "");
        if found.kind != EOF {
            d = d.with_secondary(self.span(found.range), "found here");
        }
        if let Some(text) = insert {
            let ins = self.insertion_text(at, text);
            d = d.with_fix(Fix::safe(
                format!("insert `{text}`"),
                vec![TextEdit::insert(self.file, at.into(), ins)],
            ));
        }
        self.push_diag(d);
    }

    /// Supplies the expected token when it is missing (`expected `…`, found `…``).
    fn supply(&mut self, text: &str) {
        let msg = format!(
            "expected `{}`, found {}",
            text,
            self.found_text(self.current_tok())
        );
        self.supply_diag(Code::E(400), msg, Some(text));
    }

    /// Skips up to the expected token (an `ERROR` node). Consumes `kind` and returns true if found.
    fn skip_until(&mut self, kind: Option<SyntaxKind>, extra_sync: &[SyntaxKind]) -> bool {
        if self.at_eof() {
            return false;
        }
        let stops = |cur: SyntaxKind| {
            Some(cur) == kind || cur == EOF || is_sync(cur) || extra_sync.contains(&cur)
        };
        // Create an `ERROR` node only when there are tokens to skip (no childless nodes; 2.2)
        if !stops(self.current()) {
            self.start_node(ERROR_NODE);
            while !stops(self.current()) {
                self.bump();
            }
            self.finish_node();
        }
        // If it stopped at a synchronizing token without skipping anything, the caller advances one token (progress guarantee)
        if Some(self.current()) == kind {
            self.bump();
            return true;
        }
        false
    }

    /// Expects `kind`. Consumes it if present. Otherwise supplies it if possible (diagnostic and fix),
    /// else skips to a synchronizing token. Returns whether `kind` was consumed.
    fn expect(
        &mut self,
        kind: SyntaxKind,
        continuation: &[SyntaxKind],
        extra_sync: &[SyntaxKind],
    ) -> bool {
        if self.at(kind) {
            self.bump();
            return true;
        }
        let text = kind.token_text().unwrap_or("?");
        let cur = self.current();
        if is_sync(cur)
            || continuation.contains(&cur)
            || extra_sync.contains(&cur)
            || (continuation.contains(&IDENT) && is_stmt_start(cur))
        {
            self.supply(text);
            return false;
        }
        if !self.pending_has_lex_error() {
            let tok = self.current_tok();
            let msg = format!("expected `{}`, found {}", text, self.found_text(tok));
            self.error_at(Code::E(400), msg, tok.range);
        }
        self.skip_until(Some(kind), extra_sync)
    }

    /// Increments the nesting depth. Past the limit, reports E0413 and returns false.
    fn enter(&mut self) -> bool {
        if self.depth >= MAX_NESTING {
            let tok = self.current_tok();
            // Consecutive overruns at the same depth (expression and statement, condition and body of the same statement) are reported once
            if !self.at_eof() && !self.nesting_reported {
                self.nesting_reported = true;
                self.error_at(Code::E(413), "nesting is too deep (limit 256)", tok.range)
                    .notes
                    .push("split the expression or statement into smaller ones, using variables or routines".into());
            }
            return false;
        }
        self.depth += 1;
        true
    }

    fn leave(&mut self) {
        self.depth -= 1;
        if self.depth < MAX_NESTING {
            self.nesting_reported = false;
        }
    }

    // ---- File ----

    fn parse_invalid_utf8_file(&mut self) {
        self.builder.start_node(SOURCE_FILE.into());
        let n = self.tokens.len();
        if n > 1 {
            self.builder.start_node(ERROR_NODE.into());
            for i in 0..n - 1 {
                let t = self.tokens[i];
                let text =
                    String::from_utf8_lossy(&self.bytes[t.start() as usize..t.end() as usize])
                        .into_owned();
                // Turning invalid bytes into U+FFFD changes the length, so embed the raw bytes as the string
                let raw = unsafe {
                    std::str::from_utf8_unchecked(&self.bytes[t.start() as usize..t.end() as usize])
                };
                let _ = text;
                self.builder.token(t.kind.into(), raw);
            }
            self.builder.finish_node();
        }
        let eof = self.tokens[n - 1];
        self.builder.token(EOF.into(), "");
        let _ = eof;
        self.builder.finish_node();
    }

    fn parse_source_file(&mut self) {
        // Start the root before emitting trivia (leading trivia is a direct child of SOURCE_FILE)
        self.builder.start_node(SOURCE_FILE.into());
        match self.current() {
            PROGRAM_KW => self.parse_program(),
            UNIT_KW | LIBRARY_KW => {
                let tok = self.current_tok();
                let kind = tok.kind;
                self.start_node(SCRIPT);
                if kind == UNIT_KW {
                    self.error_at(
                        Code::E(3200),
                        "`unit` is not supported yet (planned for M2)",
                        tok.range,
                    );
                } else {
                    self.error_at(
                        Code::E(3011),
                        "`library` is not supported in Puy",
                        tok.range,
                    )
                    .notes
                    .push(
                        "Puy has no DLLs; extend it with the Rust extension mechanism instead"
                            .into(),
                    );
                }
                self.start_node(ERROR_NODE);
                while !self.at_eof() {
                    self.bump();
                }
                self.finish_node();
                self.bump_eof();
                self.finish_node();
            }
            _ => self.parse_script(),
        }
        self.finish_node();
    }

    fn parse_program(&mut self) {
        self.start_node(PROGRAM);
        // ProgramHeader
        self.start_node(PROGRAM_HEADER);
        self.bump(); // program
        if self.at(IDENT) {
            self.bump();
        } else {
            self.supply_diag(Code::E(405), "expected an identifier".into(), None);
        }
        if self.at(L_PAREN) {
            self.start_node(PROGRAM_PARAMS);
            self.bump();
            self.parse_ident_list_plain();
            self.expect(R_PAREN, &[SEMICOLON], &[]);
            self.finish_node();
        }
        self.expect(
            SEMICOLON,
            &[
                USES_KW,
                BEGIN_KW,
                CONST_KW,
                VAR_KW,
                PROCEDURE_KW,
                FUNCTION_KW,
                TYPE_KW,
            ],
            &[],
        );
        self.finish_node();
        if self.at(USES_KW) {
            self.parse_uses_clause();
        }
        loop {
            self.in_program_decls = true;
            self.parse_decl_sections();
            self.in_program_decls = false;
            // Main block
            if self.at(BEGIN_KW) {
                self.parse_compound_stmt();
            } else if is_stmt_start(self.current()) {
                // There are statements but no begin
                self.supply("begin");
                self.parse_compound_stmt_without_begin();
            } else if self.at(DOT) && self.nth(1) != EOF {
                // A routine body closed with `end.` by mistake: leave it to E0407
            } else if self.at(DOT) || self.at_eof() {
                self.supply("begin");
            } else {
                // A token that can start neither a declaration nor a statement: skip to a synchronizing token
                // and resume from the declaration part ("supply or skip" in `grammar.md` chapter 3)
                let before = self.pos;
                let tok = self.current_tok();
                if !self.pending_has_lex_error() {
                    let msg = format!("expected `begin`, found {}", self.found_text(tok));
                    self.error_at(Code::E(400), msg, tok.range);
                }
                self.skip_until(Some(SEMICOLON), &[]);
                if self.pos == before {
                    // Progress guarantee: a synchronizing token that cannot be consumed in this context (`end`, `until`, `else`)
                    self.start_node(ERROR_NODE);
                    self.bump();
                    self.finish_node();
                }
                continue;
            }
            break;
        }
        // The final `.`
        if self.at(DOT) {
            self.bump();
        } else if self.at(SEMICOLON) && self.nth(1) == EOF {
            let tok = self.current_tok();
            let mut d = Diagnostic::error(
                Code::E(406),
                "expected `.` after the final `end`, found `;`",
                Stage::Parser,
            )
            .with_primary(self.span(TextRange::empty(self.last_end.into())), "");
            d = d.with_secondary(self.span(tok.range), "found here");
            d = d.with_fix(Fix::safe(
                "replace `;` with `.`",
                vec![TextEdit::replace(self.file, tok.range, ".")],
            ));
            self.push_diag(d);
            self.start_node(ERROR_NODE);
            self.bump();
            self.finish_node();
        } else {
            if !self.pending_has_lex_error() {
                let at = self.last_end;
                let mut d = Diagnostic::error(
                    Code::E(406),
                    "missing `.` after the final `end`",
                    Stage::Parser,
                )
                .with_primary(self.span(TextRange::empty(at.into())), "");
                let tok = self.current_tok();
                if tok.kind != EOF {
                    d = d.with_secondary(self.span(tok.range), "found here");
                }
                d = d.with_fix(Fix::safe(
                    "insert `.`",
                    vec![TextEdit::insert(self.file, at.into(), ".")],
                ));
                self.push_diag(d);
            }
        }
        if !self.at_eof() {
            let tok = self.current_tok();
            self.error_at(
                Code::E(407),
                "unexpected content after the end of the program",
                tok.range,
            )
            .notes
            .push("a program ends at the first `end.`; routines and blocks end with `end;`".into());
            self.start_node(ERROR_NODE);
            while !self.at_eof() {
                self.bump();
            }
            self.finish_node();
        }
        self.bump_eof();
        self.finish_node();
    }

    fn parse_ident_list_plain(&mut self) {
        if self.at(IDENT) {
            self.start_node(IDENT_LIST);
            self.bump();
            while self.at(COMMA) {
                self.bump();
                if self.at(IDENT) {
                    self.bump();
                } else {
                    self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                    break;
                }
            }
            self.finish_node();
        }
    }

    // ---- Script ----

    fn parse_script(&mut self) {
        self.start_node(SCRIPT);
        if self.at(USES_KW) {
            self.parse_uses_clause();
        }
        loop {
            let cur = self.current();
            match cur {
                EOF => break,
                SEMICOLON => {
                    self.bump();
                }
                DOT => {
                    let tok = self.current_tok();
                    let trailing_only = self.nth(1) == EOF;
                    let mut d = Diagnostic::error(Code::E(407), "a script does not end with `.`", Stage::Parser)
                        .with_primary(self.span(tok.range), "")
                        .with_note("start the file with `program` and end it with `end.`, or write nothing after the last statement of a script");
                    if trailing_only {
                        let range = crate::diagnostics::deletion_range(self.bytes, tok.range);
                        d = d.with_fix(Fix::safe(
                            "remove `.`",
                            vec![TextEdit::delete(self.file, range)],
                        ));
                    }
                    self.push_diag(d);
                    self.start_node(ERROR_NODE);
                    self.bump();
                    self.finish_node();
                }
                USES_KW => {
                    self.parse_uses_clause();
                }
                CONST_KW => self.parse_const_section(),
                VAR_KW if self.script_var_is_section() => self.parse_var_section(),
                PROCEDURE_KW | FUNCTION_KW => self.parse_routine_decl(true),
                TYPE_KW | LABEL_KW | RESOURCESTRING_KW | THREADVAR_KW | EXPORTS_KW => {
                    self.skip_unsupported_section()
                }
                _ => {
                    let before = self.pos;
                    self.parse_statement_or_recover(true);
                    // Item separator
                    if self.at(SEMICOLON) {
                        self.bump();
                    } else if self.at_eof() || self.at(DOT) {
                    } else if is_stmt_start(self.current())
                        || is_decl_start(self.current())
                        || self.at(USES_KW)
                    {
                        self.supply(";");
                    } else if self.pos == before {
                        // Progress guarantee
                        self.unexpected_token_progress();
                    } else {
                        let tok = self.current_tok();
                        if !self.pending_has_lex_error() {
                            let msg = format!("expected `;`, found {}", self.found_text(tok));
                            self.error_at(Code::E(400), msg, tok.range);
                        }
                        self.skip_until(Some(SEMICOLON), &[]);
                    }
                }
            }
        }
        self.bump_eof();
        self.finish_node();
    }

    /// Whether a `var` at the top level of a script is a `VarSection` (1.1).
    fn script_var_is_section(&self) -> bool {
        // var IDENT := ... / var IDENT : TypeRef := ... / var IDENT ; is an InlineVarDecl
        if self.nth(1) != IDENT {
            return true;
        }
        match self.nth(2) {
            ASSIGN => false,
            SEMICOLON => false,
            COLON => {
                // Whether `:=` appears before `;`
                let mut i = 3;
                loop {
                    match self.nth(i) {
                        ASSIGN => return false,
                        SEMICOLON | EOF => return true,
                        _ => i += 1,
                    }
                    if i > 64 {
                        return true;
                    }
                }
            }
            _ => true,
        }
    }

    /// Puts one token that cannot start a statement into an `ERROR` node and advances (progress guarantee).
    fn unexpected_token_progress(&mut self) {
        let tok = self.current_tok();
        if tok.kind == EOF {
            return;
        }
        if !self.pending_has_lex_error() {
            self.error_at(Code::E(402), "expected a statement", tok.range);
        }
        self.start_node(ERROR_NODE);
        self.bump();
        self.finish_node();
    }

    // ---- uses ----

    fn parse_uses_clause(&mut self) {
        self.start_node(USES_CLAUSE);
        self.bump(); // uses
        loop {
            if self.at(IDENT) {
                self.start_node(USES_ITEM);
                self.parse_qualified_name();
                if self.at(DOT) && matches!(self.nth(1), SEMICOLON | COMMA | AS_KW | IN_KW | EOF) {
                    // `uses System.;`: no identifier after `.` (E0405)
                    self.bump();
                    self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                }
                if self.at(AS_KW) {
                    self.bump();
                    if self.at(IDENT) {
                        self.bump();
                    } else {
                        self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                    }
                } else if self.at(IN_KW) {
                    let in_tok = self.current_tok();
                    self.bump();
                    let mut end = in_tok.end();
                    if self.at(STRING_LIT) {
                        end = self.current_tok().end();
                        self.bump();
                    } else {
                        self.supply_diag(Code::E(403), "expected an expression".into(), None);
                    }
                    let text = &self.bytes[in_tok.start() as usize..end as usize];
                    let shown = String::from_utf8_lossy(text).into_owned();
                    // Also remove the preceding whitespace
                    let mut del_start = in_tok.start() as usize;
                    while del_start > 0 && matches!(self.bytes[del_start - 1], b' ' | b'\t') {
                        del_start -= 1;
                    }
                    let d = Diagnostic::warning(
                        Code::W(6),
                        format!("`{shown}` is ignored"),
                        Stage::Parser,
                    )
                    .with_primary(
                        self.span(TextRange::new(in_tok.start().into(), end.into())),
                        "",
                    )
                    .with_fix(Fix::safe(
                        format!("remove `{shown}`"),
                        vec![TextEdit::delete(
                            self.file,
                            TextRange::new((del_start as u32).into(), end.into()),
                        )],
                    ));
                    self.push_diag(d);
                }
                self.finish_node();
            } else {
                self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                if !self.at(COMMA) && !self.at(SEMICOLON) {
                    break;
                }
            }
            if self.at(COMMA) {
                self.bump();
                continue;
            }
            break;
        }
        self.expect(
            SEMICOLON,
            &[
                BEGIN_KW,
                CONST_KW,
                VAR_KW,
                PROCEDURE_KW,
                FUNCTION_KW,
                TYPE_KW,
                IDENT,
            ],
            &[],
        );
        self.finish_node();
    }

    fn parse_qualified_name(&mut self) {
        self.start_node(QUALIFIED_NAME);
        self.bump(); // IDENT
        while self.at(DOT) && self.nth(1) == IDENT {
            self.bump();
            self.bump();
        }
        if self.at(DOT) && !matches!(self.nth(1), EOF) && self.nth(1) != IDENT && !self.at_eof() {
            // No identifier after `.`: to tell it apart from the `.` ending a `program`, leave it alone if EOF/trivia follows
            let next = self.nth(1);
            if !is_sync(next) && next != R_PAREN {
                self.bump();
                self.supply_diag(Code::E(405), "expected an identifier".into(), None);
            }
        }
        self.finish_node();
    }

    // ---- Declarations ----

    fn parse_decl_sections(&mut self) {
        loop {
            match self.current() {
                CONST_KW => self.parse_const_section(),
                VAR_KW => self.parse_var_section(),
                PROCEDURE_KW | FUNCTION_KW => self.parse_routine_decl(false),
                USES_KW => self.parse_uses_clause(),
                TYPE_KW | LABEL_KW | RESOURCESTRING_KW | THREADVAR_KW | EXPORTS_KW => {
                    self.skip_unsupported_section()
                }
                _ => break,
            }
        }
    }

    fn parse_const_section(&mut self) {
        self.start_node(CONST_SECTION);
        self.bump(); // const
        let mut first = true;
        loop {
            if (self.at(IDENT) && (first || matches!(self.nth(1), COLON | EQ)))
                || matches!(self.current(), COLON | EQ)
            {
                // A declaration with no identifier starting with `:` / `=` (`const = 1;`) reports E0405 and reads on
                self.parse_const_decl();
            } else if first {
                self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                break;
            } else {
                break;
            }
            first = false;
        }
        self.finish_node();
    }

    fn parse_const_decl(&mut self) {
        let after_lex_error = self.pending_has_lex_error();
        self.start_node(CONST_DECL);
        if self.at(IDENT) {
            self.bump();
        } else if !after_lex_error {
            self.supply_diag(Code::E(405), "expected an identifier".into(), None);
        }
        if self.at(COLON) {
            self.bump();
            self.parse_type_ref(&[EQ, SEMICOLON]);
        }
        if self.expect(EQ, &[], &[])
            || can_start_expr(self.current())
            || self.pending_has_lex_error()
        {
            self.parse_expr_or_missing(&[]);
        }
        self.expect(SEMICOLON, &[IDENT, BEGIN_KW], &[]);
        self.finish_node();
    }

    fn parse_var_section(&mut self) {
        self.start_node(VAR_SECTION);
        self.bump(); // var
        let mut first = true;
        loop {
            if (self.at(IDENT) && (first || matches!(self.nth(1), COMMA | COLON)))
                || matches!(self.current(), COLON | COMMA)
            {
                // A declaration with no identifier starting with `:` / `,` (`var : Integer;`) reports E0405 and reads on
                self.parse_var_decl();
            } else if first {
                self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                break;
            } else {
                break;
            }
            first = false;
        }
        self.finish_node();
    }

    fn parse_var_decl(&mut self) {
        // When supplying an identifier right after a lexical `ERROR` token (`éa: Integer;`), no diagnostic is issued (chapter 3)
        let after_lex_error = self.pending_has_lex_error();
        self.start_node(VAR_DECL);
        let mut count = 0;
        self.start_node(IDENT_LIST);
        if self.at(IDENT) {
            self.bump();
        } else if !after_lex_error {
            self.supply_diag(Code::E(405), "expected an identifier".into(), None);
        }
        count += 1;
        while self.at(COMMA) {
            self.bump();
            if self.at(IDENT) {
                self.bump();
                count += 1;
            } else {
                self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                break;
            }
        }
        self.finish_node();
        let had_colon = self.expect(COLON, &[IDENT, STRING_KW], &[]);
        if had_colon || self.at(IDENT) || self.at(STRING_KW) {
            self.parse_type_ref(&[EQ, SEMICOLON]);
        }
        if self.at(EQ) {
            let eq = self.current_tok();
            self.bump();
            let start = self.last_end;
            self.parse_expr_or_missing(&[]);
            if count > 1 {
                let range = TextRange::new(eq.range.start(), self.last_end.into());
                let _ = start;
                self.error_at(
                    Code::E(409),
                    "only one variable can be declared with an initial value",
                    range,
                );
            }
        } else if self.at_ident("absolute") {
            let abs = self.current_tok();
            self.bump_as(ABSOLUTE_KW);
            let expr_start = self.current_tok().range.start();
            self.parse_expr_or_missing(&[]);
            let expr_range = TextRange::new(expr_start, self.last_end.into());
            let d = Diagnostic::error(
                Code::E(3005),
                "`absolute` is not supported in Puy",
                Stage::Parser,
            )
            .with_primary(self.span(abs.range), "")
            .with_secondary(self.span(expr_range), "")
            .with_note("use an explicit conversion function instead");
            self.push_diag(d);
        }
        self.expect(SEMICOLON, &[IDENT, BEGIN_KW], &[]);
        self.finish_node();
    }

    /// `TypeRef`. `stop` is the token ending this declaration (the skip target).
    fn parse_type_ref(&mut self, stop: &[SyntaxKind]) {
        let cur = self.current();
        match cur {
            IDENT if self.at_ident("reference") && self.nth(1) == TO_KW => {
                // `reference to procedure(...)` (M3): skip to the `;` of the declaration
                let tok = self.current_tok();
                self.error_at(
                    Code::E(3200),
                    "`reference to` is not supported yet (planned for M3)",
                    tok.range,
                );
                self.start_node(ERROR_NODE);
                let mut depth = 0i32;
                while !self.at_eof() {
                    let c = self.current();
                    if depth == 0 && (stop.contains(&c) || c == SEMICOLON) {
                        break;
                    }
                    match c {
                        L_PAREN => depth += 1,
                        R_PAREN => depth -= 1,
                        _ => {}
                    }
                    self.bump();
                }
                self.finish_node();
            }
            IDENT => {
                self.start_node(TYPE_REF);
                self.parse_qualified_name();
                self.finish_node();
            }
            STRING_KW => {
                self.start_node(TYPE_REF);
                self.bump();
                self.finish_node();
                if self.at(L_BRACK) {
                    let tok = self.current_tok();
                    self.error_at(
                        Code::E(3003),
                        "`string[N]` is not supported in Puy",
                        tok.range,
                    )
                    .notes
                    .push(
                        "use `string`; for bytes use `TBytes`, and for UTF-8 data use `UTF8String`"
                            .into(),
                    );
                    self.start_node(ERROR_NODE);
                    while !self.at(R_BRACK) && !self.at_eof() && !is_sync(self.current()) {
                        self.bump();
                    }
                    if self.at(R_BRACK) {
                        self.bump();
                    }
                    self.finish_node();
                }
            }
            ARRAY_KW | SET_KW | RECORD_KW | CLASS_KW | INTERFACE_KW | PACKED_KW | PROCEDURE_KW
            | FUNCTION_KW | INT_LIT | STRING_LIT | MINUS | L_PAREN | CARET | FILE_KW
            | OBJECT_KW => {
                let tok = self.current_tok();
                let text = self.text_of(tok).to_ascii_lowercase();
                let is_reference = self.at_ident("reference");
                let _ = is_reference;
                match cur {
                    CARET => {
                        self.error_at(Code::E(3004), "`^` is not supported in Puy", tok.range)
                            .notes
                            .push(
                                "use a class, a dynamic array or a `var` parameter instead".into(),
                            );
                    }
                    FILE_KW => {
                        self.error_at(Code::E(3006), "`file` is not supported in Puy", tok.range)
                            .notes
                            .push("use streams or `TFile` instead".into());
                    }
                    OBJECT_KW => {
                        self.error_at(Code::E(3002), "`object` is not supported in Puy", tok.range)
                            .notes
                            .push("use a `record` or a `class` instead".into());
                    }
                    _ => {
                        let what = match cur {
                            INT_LIT | STRING_LIT | MINUS => "subrange type".to_string(),
                            L_PAREN => "enumeration type".to_string(),
                            _ => format!("`{text}`"),
                        };
                        let m = match cur {
                            CLASS_KW | INTERFACE_KW => "M3",
                            _ => "M2",
                        };
                        self.error_at(
                            Code::E(3200),
                            format!("{what} is not supported yet (planned for {m})"),
                            tok.range,
                        );
                    }
                }
                self.start_node(ERROR_NODE);
                let mut depth = 0i32;
                while !self.at_eof() {
                    let c = self.current();
                    if depth == 0 && (stop.contains(&c) || c == SEMICOLON) {
                        break;
                    }
                    if depth == 0
                        && matches!(
                            c,
                            BEGIN_KW | CONST_KW | VAR_KW | PROCEDURE_KW | FUNCTION_KW | TYPE_KW
                        )
                        && cur != PROCEDURE_KW
                        && cur != FUNCTION_KW
                    {
                        break;
                    }
                    match c {
                        RECORD_KW | CLASS_KW | OBJECT_KW | INTERFACE_KW | BEGIN_KW => depth += 1,
                        END_KW => {
                            depth -= 1;
                            if depth < 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    self.bump();
                }
                self.finish_node();
            }
            _ => {
                // reference to ... is an IDENT, so it becomes TYPE_REF above (sema reports E3200)
                self.supply_diag(Code::E(404), "expected a type".into(), None);
            }
        }
    }

    fn skip_unsupported_section(&mut self) {
        let tok = self.current_tok();
        let kind = tok.kind;
        match kind {
            TYPE_KW | RESOURCESTRING_KW => {
                let name = self.text_of(tok).to_ascii_lowercase();
                self.error_at(
                    Code::E(3200),
                    format!("`{name}` is not supported yet (planned for M2)"),
                    tok.range,
                );
            }
            LABEL_KW => {
                self.error_at(Code::E(3001), "`label` is not supported in Puy", tok.range)
                    .notes
                    .push("use `Break`, `Continue`, `Exit` or an exception instead".into());
            }
            THREADVAR_KW => {
                self.error_at(
                    Code::E(3012),
                    "`threadvar` is not supported in Puy",
                    tok.range,
                )
                .notes
                .push("use a global variable; each isolate has its own copy".into());
            }
            _ => {
                self.error_at(
                    Code::E(3011),
                    "`exports` is not supported in Puy",
                    tok.range,
                )
                .notes
                .push(
                    "Puy has no DLLs; extend it with the Rust extension mechanism instead".into(),
                );
            }
        }
        recovery::skip_section(self);
    }

    // ---- Routines ----

    fn parse_routine_decl(&mut self, in_script: bool) {
        self.start_node(ROUTINE_DECL);
        let decl_start = self.current_tok().range.start();
        // Heading
        self.start_node(ROUTINE_HEADER);
        let is_function = self.at(FUNCTION_KW);
        self.bump();
        let mut name = String::new();
        let mut name_range = TextRange::empty(self.last_end.into());
        if self.at(IDENT) {
            let tok = self.current_tok();
            name = self.text_of(tok).to_ascii_lowercase();
            name_range = tok.range;
            self.parse_qualified_name();
        } else {
            self.supply_diag(Code::E(405), "expected an identifier".into(), None);
        }
        if self.at(L_PAREN) {
            self.parse_param_list();
        }
        if is_function
            && (self.expect(COLON, &[IDENT, STRING_KW], &[])
                || self.at(IDENT)
                || self.at(STRING_KW))
        {
            self.start_node(RETURN_TYPE);
            self.parse_type_ref(&[SEMICOLON]);
            self.finish_node();
        }
        self.finish_node(); // ROUTINE_HEADER
        self.expect(
            SEMICOLON,
            &[BEGIN_KW, CONST_KW, VAR_KW, IDENT, INLINE_KW, LIBRARY_KW],
            &[],
        );
        // Directives
        let mut has_forward = false;
        loop {
            let kind = if self.at(INLINE_KW) {
                Some(INLINE_KW)
            } else if self.at(LIBRARY_KW) {
                Some(LIBRARY_KW)
            } else if self.at(IDENT) {
                routine_directive_kind(&self.current_lower())
            } else {
                None
            };
            let Some(kind) = kind else { break };
            self.start_node(ROUTINE_DIRECTIVE);
            if kind == INLINE_KW || kind == LIBRARY_KW {
                self.bump();
            } else {
                self.bump_as(kind);
            }
            if kind == FORWARD_KW || kind == EXTERNAL_KW {
                // `external` has no body either (sema reports E3011; `grammar.md` 1.3)
                has_forward = true;
            }
            if matches!(kind, DEPRECATED_KW | EXTERNAL_KW) {
                // Directives that take arguments: everything up to the next `;` is part of the directive
                while !self.at(SEMICOLON)
                    && !self.at_eof()
                    && !self.at(BEGIN_KW)
                    && !is_decl_start(self.current())
                {
                    self.bump();
                }
            }
            self.finish_node();
            self.expect(SEMICOLON, &[BEGIN_KW, CONST_KW, VAR_KW, IDENT], &[]);
        }
        let semi_end = self.last_end;
        // Body
        if has_forward {
            // Do not read a body
        } else if matches!(
            self.current(),
            CONST_KW | VAR_KW | TYPE_KW | LABEL_KW | BEGIN_KW | PROCEDURE_KW | FUNCTION_KW
        ) {
            // A body starting with `procedure` / `function` (nested routine, E3200) or with a local
            // `type` / `label` section (E3200 / E3001) (1.3)
            if self.parse_routine_body(false, in_script) {
                self.expect_semicolon_after_body();
            }
        } else if self.in_program_decls
            && !in_script
            && is_stmt_start(self.current())
            && !is_decl_start(self.current())
        {
            self.supply("begin");
            self.parse_routine_body(true, false);
            self.expect_semicolon_after_body();
        } else if is_decl_start(self.current())
            || self.at(DOT)
            || self.at_eof()
            || self.at(BEGIN_KW)
            || self.at(USES_KW)
            || (in_script && is_stmt_start(self.current()))
        {
            // No body
            if !name.is_empty() {
                self.bodyless.push(BodylessDecl {
                    range: TextRange::new(decl_start, semi_end.into()),
                    name,
                    name_range,
                    insert_at: semi_end.into(),
                    in_script,
                });
            }
        } else {
            // A token not allowed as a continuation after a syntactically complete construct
            let tok = self.current_tok();
            if !self.pending_has_lex_error() {
                let msg = format!("unexpected {}", self.found_text(tok));
                self.error_at(Code::E(401), msg, tok.range);
            }
            // Skip up to and including `;`, then read the body if one follows
            self.skip_until(Some(SEMICOLON), &[]);
            if matches!(self.current(), CONST_KW | VAR_KW | BEGIN_KW) {
                if self.parse_routine_body(false, in_script) {
                    self.expect_semicolon_after_body();
                }
            } else if !name.is_empty() {
                self.bodyless.push(BodylessDecl {
                    range: TextRange::new(decl_start, semi_end.into()),
                    name,
                    name_range,
                    insert_at: semi_end.into(),
                    in_script,
                });
            }
        }
        self.finish_node(); // ROUTINE_DECL
    }

    /// The `;` after a routine body. Closing with `end.` by mistake (next is `.`) is left to E0407.
    fn expect_semicolon_after_body(&mut self) {
        if self.at(DOT) && self.nth(1) != EOF {
            return;
        }
        self.expect(
            SEMICOLON,
            &[IDENT, BEGIN_KW, PROCEDURE_KW, FUNCTION_KW, CONST_KW, VAR_KW],
            &[],
        );
    }

    fn parse_param_list(&mut self) {
        self.start_node(PARAM_LIST);
        self.bump(); // (
        if !self.at(R_PAREN) {
            loop {
                self.parse_param_group();
                if self.at(SEMICOLON) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(R_PAREN, &[COLON, SEMICOLON], &[]);
        self.finish_node();
    }

    fn parse_param_group(&mut self) {
        self.start_node(PARAM_GROUP);
        let mut modifier: Option<SyntaxKind> = None;
        if self.at(CONST_KW) || self.at(VAR_KW) {
            modifier = Some(self.current());
            self.bump();
        } else if self.at_ident("out") {
            modifier = Some(OUT_KW);
            self.bump_as(OUT_KW);
        }
        if self.at(IDENT) {
            self.start_node(IDENT_LIST);
            self.bump();
            while self.at(COMMA) {
                self.bump();
                if self.at(IDENT) {
                    self.bump();
                } else {
                    self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                    break;
                }
            }
            self.finish_node();
        } else {
            self.supply_diag(Code::E(405), "expected an identifier".into(), None);
            self.skip_until(None, &[R_PAREN, COLON]);
        }
        if self.at(COLON) {
            self.bump();
            self.parse_type_ref(&[R_PAREN, SEMICOLON, EQ]);
            if self.at(EQ) {
                let tok = self.current_tok();
                self.error_at(
                    Code::E(3200),
                    "default parameter values are not supported yet (planned for M2)",
                    tok.range,
                );
                self.start_node(ERROR_NODE);
                while !self.at(R_PAREN)
                    && !self.at(SEMICOLON)
                    && !self.at_eof()
                    && !is_sync(self.current())
                {
                    self.bump();
                }
                self.finish_node();
            }
        } else if matches!(self.current(), SEMICOLON | R_PAREN) {
            if modifier.is_some() {
                let range = TextRange::new(
                    self.current_tok().range.start(),
                    self.current_tok().range.start(),
                );
                let _ = range;
                let d = Diagnostic::error(
                    Code::E(3005),
                    "untyped parameters are not supported in Puy",
                    Stage::Parser,
                )
                .with_primary(self.span(TextRange::empty(self.last_end.into())), "")
                .with_note("give the parameter a type such as `TBytes`");
                self.push_diag(d);
            } else {
                self.supply(":");
            }
        } else {
            self.expect(COLON, &[IDENT, STRING_KW], &[R_PAREN]);
            if self.at(IDENT) || self.at(STRING_KW) {
                self.parse_type_ref(&[R_PAREN, SEMICOLON, EQ]);
            }
        }
        self.finish_node();
    }

    /// `RoutineBody`. `without_begin` is set when `begin` was supplied; `in_script` for a routine at the top level
    /// of a script. Returns whether a `CompoundStmt` was read (if not, no trailing `;` is required).
    fn parse_routine_body(&mut self, without_begin: bool, in_script: bool) -> bool {
        self.start_node(ROUTINE_BODY);
        let saved = self.in_program_decls;
        self.in_program_decls = false;
        self.routine_depth += 1;
        loop {
            match self.current() {
                CONST_KW => self.parse_const_section(),
                VAR_KW => self.parse_var_section(),
                PROCEDURE_KW | FUNCTION_KW => {
                    if self.routine_depth >= MAX_NESTING {
                        self.skip_too_deep_routines();
                        break;
                    }
                    // Nested routine: read it, then wrap it in an ERROR node. E3200 is reported only for the outermost
                    // nesting (a nested routine inside a nested routine continues the same error; 1.3)
                    if self.routine_depth == 1 {
                        let tok = self.current_tok();
                        self.error_at(
                            Code::E(3200),
                            "nested routines are not supported yet (planned for M2)",
                            tok.range,
                        );
                    }
                    self.start_node(ERROR_NODE);
                    self.parse_routine_decl(false);
                    self.finish_node();
                }
                TYPE_KW | LABEL_KW | RESOURCESTRING_KW | THREADVAR_KW | EXPORTS_KW => {
                    self.skip_unsupported_section()
                }
                _ => break,
            }
        }
        let mut has_compound = true;
        if without_begin && !self.at(BEGIN_KW) {
            self.parse_compound_stmt_without_begin();
        } else if self.at(BEGIN_KW) {
            self.parse_compound_stmt();
        } else if in_script {
            // At the top level of a script, `begin` is not guessed (1.3). Following statements are top-level statements.
            // No fix is attached (inserting `begin` would swallow the later top-level statements as the body)
            let tok = self.current_tok();
            let msg = format!("expected `begin`, found {}", self.found_text(tok));
            let before = self.diags.len();
            self.supply_diag(Code::E(400), msg, None);
            if self.diags.len() > before {
                let d = self
                    .diags
                    .last_mut()
                    .expect("the diagnostic was just pushed");
                d.notes.push(
                    "a body starts with `begin`; in a script, statements after the declarations are top-level statements".into(),
                );
            }
            has_compound = false;
        } else {
            self.supply("begin");
            if is_stmt_start(self.current()) {
                self.parse_compound_stmt_without_begin();
            }
        }
        self.routine_depth -= 1;
        self.in_program_decls = saved;
        self.finish_node();
        has_compound
    }

    /// When nested routines exceed the limit (E0413): make everything up to the end of the file an `ERROR` node
    /// (the range of a nested body cannot be determined from a chain of headings; `grammar.md` chapter 3).
    fn skip_too_deep_routines(&mut self) {
        let tok = self.current_tok();
        if !self.nesting_reported {
            self.nesting_reported = true;
            self.error_at(Code::E(413), "nesting is too deep (limit 256)", tok.range)
                .notes
                .push("move the nested routines to the top level".into());
        }
        self.routines_too_deep = true;
        self.start_node(ERROR_NODE);
        while !self.at_eof() {
            self.bump();
        }
        self.finish_node();
    }

    // ---- Statements ----

    fn parse_compound_stmt(&mut self) {
        self.start_node(COMPOUND_STMT);
        let begin = self.current_tok();
        self.bump(); // begin
        self.open_blocks.push((BEGIN_KW, begin.range));
        self.parse_stmt_list(END_KW);
        self.open_blocks.pop();
        if self.at(END_KW) {
            self.bump();
        }
        self.finish_node();
    }

    /// Body when `begin` was supplied (up to `end`).
    fn parse_compound_stmt_without_begin(&mut self) {
        self.start_node(COMPOUND_STMT);
        let at = TextRange::empty(self.last_end.into());
        self.open_blocks.push((BEGIN_KW, at));
        self.parse_stmt_list(END_KW);
        self.open_blocks.pop();
        if self.at(END_KW) {
            self.bump();
        }
        self.finish_node();
    }

    /// `StmtList`. `closer` is `end` or `until`.
    fn parse_stmt_list(&mut self, closer: SyntaxKind) {
        loop {
            if self.closing_all {
                return;
            }
            let cur = self.current();
            if cur == closer {
                return;
            }
            match cur {
                EOF | DOT => {
                    self.unclosed_block(closer);
                    return;
                }
                END_KW if closer == UNTIL_KW => {
                    self.repeat_closed_by_end();
                    return;
                }
                END_KW | UNTIL_KW => {
                    // Mismatched closer: `until` where `end` is expected, etc.
                    let tok = self.current_tok();
                    if !self.pending_has_lex_error() {
                        let msg = format!(
                            "expected `{}`, found {}",
                            closer.token_text().unwrap(),
                            self.found_text(tok)
                        );
                        self.error_at(Code::E(400), msg, tok.range);
                    }
                    self.start_node(ERROR_NODE);
                    self.bump();
                    self.finish_node();
                    continue;
                }
                SEMICOLON => {
                    self.bump();
                    continue;
                }
                _ => {}
            }
            let before = self.pos;
            if !self.parse_statement_or_recover(true) {
                // Only put a token that cannot start a statement into an `ERROR` node (progress guarantee).
                // Resume reading statements from the next token (no `;` needed)
                continue;
            }
            if self.closing_all {
                return;
            }
            // After the statement
            if self.at(SEMICOLON) {
                self.bump();
            } else if self.at(closer)
                || self.at_eof()
                || matches!(self.current(), DOT | END_KW | UNTIL_KW)
            {
            } else if is_stmt_start(self.current()) {
                self.supply(";");
            } else if self.pos == before {
                self.unexpected_token_progress();
            } else {
                let tok = self.current_tok();
                if !self.pending_has_lex_error() {
                    let msg = format!("expected `;`, found {}", self.found_text(tok));
                    self.error_at(Code::E(400), msg, tok.range);
                }
                let consumed = self.skip_until(Some(SEMICOLON), &[closer]);
                if !consumed && self.pos == before {
                    self.unexpected_token_progress();
                }
            }
        }
    }

    /// `end` inside `repeat` (chapter 3). Reports one E0400 as if `until` were supplied.
    /// If the `end` is surplus (`end_is_surplus`), it is put into an `ERROR` node as a mistyped `until`;
    /// otherwise it is left to close an outer `begin`.
    fn repeat_closed_by_end(&mut self) {
        let tok = self.current_tok();
        if !self.pending_has_lex_error() {
            let (_, start) = *self
                .open_blocks
                .last()
                .expect("a StmtList closed by `until` was opened by `repeat`");
            let msg = format!("expected `until`, found {}", self.found_text(tok));
            let d = Diagnostic::error(Code::E(400), msg, Stage::Parser)
                .with_primary(self.span(tok.range), "")
                .with_secondary(self.span(start), "block starts here")
                .with_note("write `until` followed by the loop condition");
            self.push_diag(d);
        }
        let outer = &self.open_blocks[..self.open_blocks.len() - 1];
        let open_begins = outer.iter().filter(|(k, _)| *k == BEGIN_KW).count();
        if self.end_is_surplus(open_begins) {
            self.start_node(ERROR_NODE);
            self.bump();
            self.finish_node();
        }
    }

    /// Whether the current `end` (found inside `repeat`) is surplus. Given `open_begins` outer `begin`s still open,
    /// it is surplus if `open_begins` `end`s that close them (unbalanced `end`s) already appear after
    /// this `end` ("`end` inside `repeat`" in chapter 3).
    /// If there is no outer `begin` (top level of a script, etc.), it is always surplus.
    ///
    /// Lookahead goes only to the end of this part: `.`, `EOF`, `until` at depth 0 (belonging to an outer `repeat`),
    /// the start of a section (`type`, `label`, `uses`, etc.), or `procedure` / `function` right after `;`.
    fn end_is_surplus(&self, open_begins: usize) -> bool {
        let mut closers = 0usize;
        // Open constructs. A `case` inside a `record` (variant part) shares the `record`'s `end`, so it is not pushed
        let mut open: Vec<SyntaxKind> = Vec::new();
        let mut prev = END_KW;
        let start = self.index_of_nth(0) + 1;
        for tok in &self.tokens[start..] {
            if closers == open_begins {
                return true;
            }
            let k = tok.kind;
            if k.is_skipped_by_parser() {
                continue;
            }
            match k {
                DOT | EOF => return false,
                TYPE_KW | LABEL_KW | USES_KW | RESOURCESTRING_KW | THREADVAR_KW | EXPORTS_KW
                | IMPLEMENTATION_KW | INITIALIZATION_KW | FINALIZATION_KW
                    if open.is_empty() =>
                {
                    return false;
                }
                PROCEDURE_KW | FUNCTION_KW if open.is_empty() && prev == SEMICOLON => {
                    return false;
                }
                CASE_KW if open.last() == Some(&RECORD_KW) => {}
                BEGIN_KW | CASE_KW | TRY_KW | ASM_KW | RECORD_KW | REPEAT_KW => open.push(k),
                UNTIL_KW => {
                    // `until` at depth 0 belongs to an outer `repeat`. Stop reading here
                    if open.pop().is_none() {
                        return false;
                    }
                }
                END_KW if open.is_empty() => closers += 1,
                END_KW => {
                    open.pop();
                }
                _ => {}
            }
            prev = k;
        }
        closers == open_begins
    }

    /// Blocks not closed at `.` or `EOF` (chapter 3).
    fn unclosed_block(&mut self, closer: SyntaxKind) {
        if self.closing_all {
            return;
        }
        self.closing_all = true;
        let found = self.current_tok();
        let Some(&(kind, start)) = self.open_blocks.last() else {
            return;
        };
        let expected = closer.token_text().unwrap();
        let msg = format!("expected `{}`, found {}", expected, self.found_text(found));
        let mut d = Diagnostic::error(Code::E(400), msg, Stage::Parser)
            .with_primary(self.span(TextRange::empty(self.last_end.into())), "");
        if found.kind != EOF {
            d = d.with_secondary(self.span(found.range), "found here");
        }
        d = d.with_secondary(self.span(start), "block starts here");
        let _ = kind;
        if self.open_blocks.iter().all(|(k, _)| *k == BEGIN_KW) {
            let n = self.open_blocks.len();
            let text = " end".repeat(n);
            d = d.with_fix(Fix::safe(
                format!("insert `{}`", text.trim_start()),
                vec![TextEdit::insert(self.file, self.last_end.into(), text)],
            ));
        } else {
            d = d.with_note("write `until` followed by the loop condition");
        }
        self.push_diag(d);
    }

    /// Reads a statement. If the token cannot start a statement, reports a diagnostic, advances one token, and returns false.
    fn parse_statement_or_recover(&mut self, at_list_level: bool) -> bool {
        // Labeled statement (`Done:`, `1:`): report the label as E3001, drop it, and read the following statement normally
        if matches!(self.current(), IDENT | INT_LIT) && self.nth(1) == COLON {
            let tok = self.current_tok();
            let name = self.text_of(tok).to_string();
            self.error_at(
                Code::E(3001),
                format!("label `{name}` is not supported in Puy"),
                tok.range,
            )
            .notes
            .push("use `Break`, `Continue`, `Exit` or an exception instead".into());
            self.start_node(ERROR_NODE);
            self.bump();
            self.bump();
            self.finish_node();
        }
        let cur = self.current();
        match cur {
            IDENT | STRING_KW | INT_LIT | FLOAT_LIT | STRING_LIT | NIL_KW | L_PAREN
            | INHERITED_KW | NOT_KW | MINUS | PLUS | AT | L_BRACK => self.parse_simple_stmt(),
            BEGIN_KW => {
                // The body of `begin` also counts toward the nesting depth (`grammar.md` chapter 3)
                if self.enter() {
                    self.parse_compound_stmt();
                    self.leave();
                } else {
                    recovery::skip_nested_statement(self);
                }
            }
            IF_KW => self.parse_if_stmt(at_list_level),
            WHILE_KW => self.parse_while_stmt(),
            REPEAT_KW => self.parse_repeat_stmt(),
            FOR_KW => self.parse_for_stmt(),
            VAR_KW => self.parse_inline_var_decl(),
            CONST_KW => self.parse_inline_const_decl(),
            CASE_KW | TRY_KW | RAISE_KW | WITH_KW | GOTO_KW | ASM_KW => {
                let tok = self.current_tok();
                match cur {
                    WITH_KW => {
                        self.error_at(Code::E(3000), "`with` is not supported in Puy", tok.range)
                            .notes
                            .push("use an inline variable (`var R := …;`) and qualify the fields with it".into());
                    }
                    GOTO_KW => {
                        self.error_at(Code::E(3001), "`goto` is not supported in Puy", tok.range)
                            .notes
                            .push("use `Break`, `Continue`, `Exit` or an exception instead".into());
                    }
                    ASM_KW => {
                        self.error_at(Code::E(3008), "`asm` is not supported in Puy", tok.range)
                            .notes
                            .push(
                                "Puy runs on a virtual machine and cannot execute assembly".into(),
                            );
                    }
                    _ => {
                        let name = self.text_of(tok).to_ascii_lowercase();
                        self.error_at(
                            Code::E(3200),
                            format!("`{name}` is not supported yet (planned for M2)"),
                            tok.range,
                        );
                    }
                }
                recovery::skip_statement(self);
            }
            _ => {
                if self.pending_has_lex_error() {
                    // Read as an expression fill-in
                    self.parse_simple_stmt();
                } else {
                    self.unexpected_token_progress();
                    return false;
                }
            }
        }
        true
    }

    /// Assignment or expression statement.
    fn parse_simple_stmt(&mut self) {
        // When the start of a statement is a fill-in for a lexical error (`é := 1;`, `é(1);`), put the hole inside the
        // statement node (`grammar.md` chapter 3; sema gives it the error type so E0707 / E0402 do not cascade)
        let hole_call = self.pending_has_lex_error() && self.at(L_PAREN);
        let cp = if hole_call || self.lex_hole_wanted() {
            self.checkpoint_keeping_lex_error()
        } else {
            self.checkpoint()
        };
        let start = self.current_tok().range.start();
        let shape = if hole_call {
            // An argument list right after a lexical error (`é(1);`) is read as a call of the hole
            self.lex_error_hole();
            self.start_node_at(cp, CALL_EXPR);
            self.parse_arg_list(&[]);
            self.finish_node();
            ExprShape::Callable
        } else {
            self.parse_expr_or_missing(&[ASSIGN])
        };
        if self.at(ASSIGN) {
            self.start_node_at(cp, ASSIGN_STMT);
            self.bump();
            self.parse_expr_or_missing(&[]);
            self.finish_node();
            return;
        }
        self.start_node_at(cp, EXPR_STMT);
        self.finish_node();
        if matches!(shape, ExprShape::NotCallable) {
            let range = TextRange::new(start, self.last_end.into());
            self.error_at(Code::E(402), "expected a statement", range);
        }
    }

    fn parse_inline_var_decl(&mut self) {
        self.start_node(INLINE_VAR_DECL);
        let var_tok = self.current_tok();
        self.bump(); // var
        let mut has_type = false;
        let mut has_init = false;
        if self.at(IDENT) {
            self.bump();
        } else {
            self.supply_diag(Code::E(405), "expected an identifier".into(), None);
        }
        if self.at(COLON) {
            self.bump();
            self.parse_type_ref(&[ASSIGN, SEMICOLON]);
            has_type = true;
        }
        if self.at(ASSIGN) {
            self.bump();
            self.parse_expr_or_missing(&[]);
            has_init = true;
        } else if self.at(EQ) {
            // `var X: T = 1` is a VarSection in a script, but what reaches here is `var X = 1`
            let tok = self.current_tok();
            let msg = format!("expected `:=`, found {}", self.found_text(tok));
            self.error_at(Code::E(400), msg, tok.range);
            self.skip_until(None, &[]);
        }
        if !has_type && !has_init {
            let range = TextRange::new(var_tok.range.start(), self.last_end.into());
            self.error_at(
                Code::E(721),
                "inline variable needs a type or an initial value",
                range,
            );
        }
        self.finish_node();
    }

    fn parse_inline_const_decl(&mut self) {
        self.start_node(INLINE_CONST_DECL);
        self.bump(); // const
        if self.at(IDENT) {
            self.bump();
        } else {
            self.supply_diag(Code::E(405), "expected an identifier".into(), None);
        }
        if self.at(COLON) {
            self.bump();
            self.parse_type_ref(&[EQ, SEMICOLON]);
        }
        if self.expect(EQ, &[], &[])
            || can_start_expr(self.current())
            || self.pending_has_lex_error()
        {
            self.parse_expr_or_missing(&[]);
        }
        self.finish_node();
    }

    /// `;` right after `then` / `else` / `do` (W0018).
    fn check_empty_statement(&mut self, after: &str) {
        if self.at(SEMICOLON) {
            let tok = self.current_tok();
            let d = Diagnostic::warning(
                Code::W(18),
                format!("empty statement after `{after}`"),
                Stage::Parser,
            )
            .with_primary(self.span(tok.range), "")
            .with_note(format!(
                "write a statement after `{after}`, or `begin end` if the body is meant to be empty"
            ));
            self.push_diag(d);
        }
    }

    /// Body statement (after `then` / `do` / `else`). Counts toward nesting.
    fn parse_body_stmt(&mut self) {
        if matches!(
            self.current(),
            SEMICOLON | END_KW | UNTIL_KW | ELSE_KW | EOF | DOT
        ) {
            return;
        }
        if !self.enter() {
            recovery::skip_nested_statement(self);
            return;
        }
        self.parse_statement_or_recover(false);
        self.leave();
    }

    fn parse_if_stmt(&mut self, at_list_level: bool) {
        self.start_node(IF_STMT);
        self.bump(); // if
        self.parse_condition(THEN_KW);
        if self.expect(
            THEN_KW,
            &[
                IDENT, BEGIN_KW, IF_KW, WHILE_KW, REPEAT_KW, FOR_KW, VAR_KW, CONST_KW,
            ],
            &[THEN_KW],
        ) || true
        {
            self.check_empty_statement("then");
            self.parse_body_stmt();
        }
        if self.at(ELSE_KW) {
            self.parse_else_branch();
        } else if at_list_level && self.at(SEMICOLON) && self.nth(1) == ELSE_KW {
            let semi = self.current_tok();
            let d = Diagnostic::error(
                Code::E(410),
                "`;` is not allowed before `else`",
                Stage::Parser,
            )
            .with_primary(self.span(semi.range), "")
            .with_fix(Fix::safe(
                "remove `;`",
                vec![TextEdit::delete(
                    self.file,
                    crate::diagnostics::deletion_range(self.bytes, semi.range),
                )],
            ));
            self.push_diag(d);
            self.bump(); // ; (stray token inside IF_STMT)
            self.parse_else_branch();
        }
        self.finish_node();
    }

    fn parse_else_branch(&mut self) {
        self.start_node(ELSE_BRANCH);
        self.bump(); // else
        self.check_empty_statement("else");
        if self.at(IF_KW) {
            // A chain of else if is not counted
            self.parse_if_stmt(false);
        } else {
            self.parse_body_stmt();
        }
        self.finish_node();
    }

    /// Condition expression (`if` / `while` / `until`). `terminator` is the context's synchronizing token.
    fn parse_condition(&mut self, terminator: SyntaxKind) {
        self.parse_expr_or_missing(&[terminator]);
    }

    fn parse_while_stmt(&mut self) {
        self.start_node(WHILE_STMT);
        self.bump(); // while
        self.parse_condition(DO_KW);
        self.expect(
            DO_KW,
            &[
                IDENT, BEGIN_KW, IF_KW, WHILE_KW, REPEAT_KW, FOR_KW, VAR_KW, CONST_KW,
            ],
            &[DO_KW],
        );
        self.check_empty_statement("do");
        self.parse_body_stmt();
        self.finish_node();
    }

    fn parse_repeat_stmt(&mut self) {
        self.start_node(REPEAT_STMT);
        let tok = self.current_tok();
        self.bump(); // repeat
        self.open_blocks.push((REPEAT_KW, tok.range));
        if self.enter() {
            self.parse_stmt_list(UNTIL_KW);
            self.leave();
        } else {
            recovery::skip_statement(self);
        }
        self.open_blocks.pop();
        if self.at(UNTIL_KW) {
            self.bump();
            self.parse_condition(SEMICOLON);
        }
        self.finish_node();
    }

    fn parse_for_stmt(&mut self) {
        self.start_node(FOR_STMT);
        self.bump(); // for
        self.start_node(FOR_VAR);
        if self.at(VAR_KW) {
            self.bump();
            if self.at(IDENT) {
                self.bump();
            } else {
                self.supply_diag(Code::E(405), "expected an identifier".into(), None);
            }
            if self.at(COLON) {
                self.bump();
                self.parse_type_ref(&[ASSIGN, DO_KW]);
            }
        } else if self.at(IDENT) {
            self.bump();
        } else {
            self.supply_diag(Code::E(405), "expected an identifier".into(), None);
        }
        self.finish_node();
        if self.at(IN_KW) {
            let tok = self.current_tok();
            self.error_at(
                Code::E(3200),
                "`for..in` is not supported yet (planned for M2)",
                tok.range,
            );
            self.start_node(ERROR_NODE);
            while !self.at(DO_KW) && !self.at_eof() && !is_sync(self.current()) {
                self.bump();
            }
            self.finish_node();
        } else {
            if self.expect(ASSIGN, &[], &[DO_KW, TO_KW, DOWNTO_KW])
                || can_start_expr(self.current())
                || self.pending_has_lex_error()
            {
                self.parse_expr_or_missing(&[TO_KW, DOWNTO_KW, DO_KW]);
            }
            if self.at(TO_KW) || self.at(DOWNTO_KW) {
                self.bump();
                self.parse_expr_or_missing(&[DO_KW]);
            } else {
                self.expect(TO_KW, &[], &[DO_KW]);
                if can_start_expr(self.current()) {
                    self.parse_expr_or_missing(&[DO_KW]);
                }
            }
        }
        self.expect(
            DO_KW,
            &[
                IDENT, BEGIN_KW, IF_KW, WHILE_KW, REPEAT_KW, FOR_KW, VAR_KW, CONST_KW,
            ],
            &[DO_KW],
        );
        self.check_empty_statement("do");
        self.parse_body_stmt();
        self.finish_node();
    }

    // ---- Expressions ----

    /// Reads an expression. If there is no expression start, E0403 (or a fill-in for a lexical error).
    /// `extra_sync` is the context's synchronizing tokens. Returns the shape of the expression (for statement checks).
    fn parse_expr_or_missing(&mut self, extra_sync: &[SyntaxKind]) -> ExprShape {
        if self.lex_hole_wanted() {
            self.lex_error_hole();
            // If a binary operator follows, continue with the hole as the left operand
            if binary_prec(self.current()).is_some() {
                // A BINARY_EXPR wrapping the hole cannot be built (there is no checkpoint before the hole),
                // so place the operator and right operand as they are
                let cp = self.checkpoint();
                let _ = cp;
                while let Some(prec) = binary_prec(self.current()) {
                    self.bump();
                    if can_start_expr(self.current()) {
                        self.parse_expr(prec + 1, extra_sync);
                    } else {
                        break;
                    }
                }
            }
            return ExprShape::Hole;
        }
        if can_start_expr(self.current()) {
            return self.parse_expr(1, extra_sync);
        }
        let cur = self.current();
        if is_sync(cur)
            || extra_sync.contains(&cur)
            || matches!(
                cur,
                R_PAREN
                    | R_BRACK
                    | COMMA
                    | COLON
                    | THEN_KW
                    | DO_KW
                    | OF_KW
                    | TO_KW
                    | DOWNTO_KW
                    | ASSIGN
            )
        {
            self.supply_diag(Code::E(403), "expected an expression".into(), None);
        } else {
            let tok = self.current_tok();
            self.error_at(Code::E(403), "expected an expression", tok.range);
            self.skip_until(None, extra_sync);
        }
        ExprShape::Hole
    }

    fn parse_expr(&mut self, min_prec: u8, extra_sync: &[SyntaxKind]) -> ExprShape {
        let cp = self.checkpoint();
        let mut shape = self.parse_unary(extra_sync);
        while let Some(prec) = binary_prec(self.current()) {
            if prec < min_prec {
                break;
            }
            self.start_node_at(cp, BINARY_EXPR);
            self.bump();
            if !self.enter() {
                recovery::skip_expr(self, extra_sync);
                self.finish_node();
                return ExprShape::NotCallable;
            }
            if self.lex_hole_wanted() {
                self.lex_error_hole();
            } else if can_start_expr(self.current()) {
                self.parse_expr(prec + 1, extra_sync);
            } else {
                self.supply_diag(Code::E(403), "expected an expression".into(), None);
            }
            self.leave();
            self.finish_node();
            shape = ExprShape::NotCallable;
        }
        shape
    }

    fn parse_unary(&mut self, extra_sync: &[SyntaxKind]) -> ExprShape {
        match self.current() {
            NOT_KW | MINUS | PLUS | AT => {
                let tok = self.current_tok();
                self.start_node(UNARY_EXPR);
                self.bump();
                if tok.kind == AT {
                    self.error_at(Code::E(3004), "`@` is not supported in Puy", tok.range)
                        .notes
                        .push("use a class, a dynamic array or a `var` parameter instead".into());
                }
                if self.enter() {
                    if self.lex_hole_wanted() {
                        self.lex_error_hole();
                    } else if can_start_expr(self.current()) {
                        self.parse_unary(extra_sync);
                    } else {
                        self.supply_diag(Code::E(403), "expected an expression".into(), None);
                    }
                    self.leave();
                } else {
                    recovery::skip_expr(self, extra_sync);
                }
                self.finish_node();
                ExprShape::NotCallable
            }
            _ => self.parse_postfix(extra_sync),
        }
    }

    fn parse_postfix(&mut self, extra_sync: &[SyntaxKind]) -> ExprShape {
        let cp = self.checkpoint();
        let mut shape = self.parse_primary(extra_sync);
        loop {
            match self.current() {
                L_PAREN => {
                    self.start_node_at(cp, CALL_EXPR);
                    self.parse_arg_list(extra_sync);
                    self.finish_node();
                    shape = ExprShape::Callable;
                }
                L_BRACK => {
                    self.start_node_at(cp, INDEX_EXPR);
                    self.bump();
                    if self.enter() {
                        loop {
                            self.parse_expr_or_missing(&[R_BRACK, COMMA]);
                            if self.at(COMMA) {
                                self.bump();
                                continue;
                            }
                            break;
                        }
                        self.leave();
                    } else {
                        recovery::skip_expr(self, &[R_BRACK]);
                    }
                    self.expect(R_BRACK, &[], &[R_BRACK, R_PAREN, COMMA]);
                    self.finish_node();
                    shape = ExprShape::NotCallable;
                }
                DOT if self.nth(1) == IDENT => {
                    self.start_node_at(cp, FIELD_EXPR);
                    self.bump();
                    self.bump();
                    self.finish_node();
                    shape = ExprShape::Callable;
                }
                DOT if !matches!(self.nth(1), EOF)
                    && !is_sync(self.nth(1))
                    && !matches!(self.nth(1), R_PAREN | COMMA | R_BRACK) =>
                {
                    self.start_node_at(cp, FIELD_EXPR);
                    self.bump();
                    self.supply_diag(Code::E(405), "expected an identifier".into(), None);
                    self.finish_node();
                    shape = ExprShape::NotCallable;
                }
                CARET => {
                    let tok = self.current_tok();
                    self.start_node_at(cp, DEREF_EXPR);
                    self.bump();
                    self.finish_node();
                    self.error_at(Code::E(3004), "`^` is not supported in Puy", tok.range)
                        .notes
                        .push("use a class, a dynamic array or a `var` parameter instead".into());
                    shape = ExprShape::NotCallable;
                }
                _ => break,
            }
        }
        shape
    }

    fn parse_arg_list(&mut self, extra_sync: &[SyntaxKind]) {
        self.start_node(ARG_LIST);
        self.bump(); // (
        // If a lexical error follows `(` right away (`Chr(0x3042)`), read it as a fill-in argument rather than zero
        // arguments (`parse_expr_or_missing` creates an `ERROR` node inside `ARG`)
        if !self.at(R_PAREN) || self.pending_has_lex_error() {
            if self.enter() {
                loop {
                    self.start_node_keeping_lex_error(ARG);
                    self.parse_expr_or_missing(&[R_PAREN, COMMA, COLON]);
                    while self.at(COLON) {
                        self.bump();
                        self.parse_expr_or_missing(&[R_PAREN, COMMA, COLON]);
                    }
                    self.finish_node();
                    if self.at(COMMA) {
                        self.bump();
                        continue;
                    }
                    break;
                }
                self.leave();
            } else {
                recovery::skip_expr(self, &[R_PAREN]);
            }
        }
        let _ = extra_sync;
        self.expect(R_PAREN, &[], &[R_PAREN, SEMICOLON, COMMA]);
        self.finish_node();
    }

    fn parse_primary(&mut self, extra_sync: &[SyntaxKind]) -> ExprShape {
        match self.current() {
            IDENT => {
                self.start_node(NAME_REF);
                self.bump();
                self.finish_node();
                ExprShape::Callable
            }
            STRING_KW => {
                self.start_node(NAME_REF);
                self.bump();
                self.finish_node();
                ExprShape::Callable
            }
            INT_LIT | FLOAT_LIT | STRING_LIT => {
                self.start_node(LITERAL);
                self.bump();
                self.finish_node();
                ExprShape::NotCallable
            }
            NIL_KW => {
                self.start_node(NIL_EXPR);
                self.bump();
                self.finish_node();
                ExprShape::NotCallable
            }
            INHERITED_KW => {
                self.start_node(INHERITED_EXPR);
                self.bump();
                if self.at(IDENT) {
                    self.bump();
                }
                self.finish_node();
                ExprShape::Callable
            }
            L_PAREN => {
                self.start_node(PAREN_EXPR);
                self.bump();
                if self.enter() {
                    self.parse_expr_or_missing(&[R_PAREN]);
                    self.leave();
                } else {
                    recovery::skip_expr(self, &[R_PAREN]);
                }
                self.expect(R_PAREN, &[], &[R_PAREN, SEMICOLON, COMMA]);
                self.finish_node();
                ExprShape::NotCallable
            }
            L_BRACK => {
                let tok = self.current_tok();
                self.error_at(
                    Code::E(3200),
                    "set and array constructors are not supported yet (planned for M2)",
                    tok.range,
                );
                self.start_node(ERROR_NODE);
                let mut depth = 0;
                while !self.at_eof() {
                    match self.current() {
                        L_BRACK => depth += 1,
                        R_BRACK => {
                            depth -= 1;
                            if depth == 0 {
                                self.bump();
                                break;
                            }
                        }
                        c if depth == 0 && (is_sync(c) || extra_sync.contains(&c)) => break,
                        c if is_sync(c) && c != BEGIN_KW && c != IF_KW => break,
                        _ => {}
                    }
                    self.bump();
                }
                self.finish_node();
                ExprShape::NotCallable
            }
            _ => {
                // Unreachable (the caller checks can_start_expr)
                self.supply_diag(Code::E(403), "expected an expression".into(), None);
                ExprShape::Hole
            }
        }
    }

    // ---- Post-processing after building the tree ----

    fn post_pass(&mut self, root: &SyntaxNode) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        out.extend(self.check_placements(root));
        out.extend(self.check_bodyless(root));
        out.extend(self.check_uses_position(root));
        out
    }

    /// W0005: a directive outside a declaration or statement boundary.
    fn check_placements(&self, root: &SyntaxNode) -> Vec<Diagnostic> {
        use rowan::NodeOrToken;
        let mut out = Vec::new();
        let mut reported_blocks: Vec<TextSize> = Vec::new();
        for element in root.descendants_with_tokens() {
            let NodeOrToken::Token(tok) = element else {
                continue;
            };
            if !matches!(tok.kind(), DIRECTIVE | INACTIVE) {
                continue;
            }
            let Some(placement) = self.directives.placement_at(tok.text_range().start()) else {
                continue;
            };
            let Some(parent) = tok.parent() else { continue };
            let boundary = match parent.kind() {
                SOURCE_FILE | PROGRAM | SCRIPT | USES_CLAUSE | CONST_SECTION | VAR_SECTION
                | PARAM_LIST | ROUTINE_DECL | ROUTINE_BODY | COMPOUND_STMT => true,
                REPEAT_STMT => {
                    // Before `until` counts as between statements
                    let until = parent
                        .children_with_tokens()
                        .filter_map(|e| e.into_token())
                        .find(|t| t.kind() == UNTIL_KW);
                    until.is_none_or(|u| tok.text_range().start() < u.text_range().start())
                }
                _ => false,
            };
            if boundary {
                continue;
            }
            match placement {
                Placement::Conditional { block_start } => {
                    if reported_blocks.contains(&block_start) {
                        continue;
                    }
                    reported_blocks.push(block_start);
                    let dir_text = self.directive_text_at(block_start);
                    let d = Diagnostic::warning(
                        Code::W(5),
                        format!(
                            "directive `{dir_text}` is not at a declaration or statement boundary"
                        ),
                        Stage::Parser,
                    )
                    .with_primary(self.span(self.token_range_at(block_start)), "");
                    out.push(d);
                }
                Placement::Switch => {
                    let dir_text = self.directive_text_at(tok.text_range().start());
                    let d = Diagnostic::warning(
                        Code::W(5),
                        format!("directive `{dir_text}` is not at a declaration or statement boundary"),
                        Stage::Parser,
                    )
                    .with_primary(self.span(tok.text_range()), "")
                    .with_note("Puy applies a switch from the next declaration or statement, not from this point");
                    out.push(d);
                }
            }
        }
        out
    }

    fn token_range_at(&self, start: TextSize) -> TextRange {
        self.tokens
            .iter()
            .find(|t| t.range.start() == start)
            .map(|t| t.range)
            .unwrap_or(TextRange::empty(start))
    }

    fn directive_text_at(&self, start: TextSize) -> String {
        let range = self.token_range_at(start);
        let text = &self.bytes[usize::from(range.start())..usize::from(range.end())];
        let text = String::from_utf8_lossy(text);
        let inner = text
            .strip_prefix("{$")
            .and_then(|t| t.strip_suffix('}'))
            .or_else(|| text.strip_prefix("(*$").and_then(|t| t.strip_suffix("*)")))
            .unwrap_or(&text);
        format!("{{${}}}", inner.trim())
    }

    /// E0411: a routine with neither a body nor `forward`.
    fn check_bodyless(&mut self, root: &SyntaxNode) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let decls = std::mem::take(&mut self.bodyless);
        for b in decls {
            let node = root
                .descendants()
                .find(|n| n.kind() == ROUTINE_DECL && n.text_range().start() == b.range.start());
            let name_text = std::str::from_utf8(
                &self.bytes[usize::from(b.name_range.start())..usize::from(b.name_range.end())],
            )
            .unwrap_or("");
            let mut d = Diagnostic::error(
                Code::E(411),
                format!("routine `{name_text}` has no body"),
                Stage::Parser,
            )
            .with_primary(self.span(b.name_range), "");
            let later_same = node.as_ref().is_some_and(|n| {
                let mut found = false;
                let mut sib = n.next_sibling();
                while let Some(s) = sib {
                    if s.kind() == ROUTINE_DECL
                        && let Some(h) = s.children().find(|c| c.kind() == ROUTINE_HEADER)
                        && let Some(q) = h.children().find(|c| c.kind() == QUALIFIED_NAME)
                        && q.text().to_string().to_ascii_lowercase() == b.name
                    {
                        found = true;
                        break;
                    }
                    sib = s.next_sibling();
                }
                found
            });
            if later_same {
                d = d.with_fix(Fix::safe(
                    "insert `forward;`",
                    vec![TextEdit::insert(self.file, b.insert_at, " forward;")],
                ));
            } else {
                d = d.with_note("write the body, or add `forward;` if the body comes later");
                if b.in_script {
                    d = d.with_note("a body starts with `begin`");
                }
            }
            out.push(d);
        }
        out
    }

    /// E0414: `uses` not at the start.
    fn check_uses_position(&self, root: &SyntaxNode) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let Some(container) = root
            .children()
            .find(|n| matches!(n.kind(), PROGRAM | SCRIPT))
        else {
            return out;
        };
        let uses: Vec<SyntaxNode> = container
            .children()
            .filter(|n| n.kind() == USES_CLAUSE)
            .collect();
        if uses.is_empty() {
            return out;
        }
        // The leading uses: in PROGRAM, right after PROGRAM_HEADER; in SCRIPT, the first child
        let first_ok = {
            let first_node = container.children().find(|n| n.kind() != PROGRAM_HEADER);
            first_node.as_ref().is_some_and(|n| n.kind() == USES_CLAUSE)
        };
        let (head, rest): (Option<&SyntaxNode>, &[SyntaxNode]) = if first_ok {
            (Some(&uses[0]), &uses[1..])
        } else {
            (None, &uses[..])
        };
        for u in rest {
            let uses_kw = u.first_token().unwrap();
            let mut d = Diagnostic::error(
                Code::E(414),
                "`uses` must come before declarations and statements",
                Stage::Parser,
            )
            .with_primary(self.span(uses_kw.text_range()), "");
            let head_semi_supplied =
                head.is_some_and(|h| h.last_token().is_some_and(|t| t.kind() != SEMICOLON));
            let in_block = self.directives.in_conditional_block(u.text_range().start());
            let insert_pos = match head {
                Some(h) => h
                    .last_token()
                    .map(|t| t.text_range().start())
                    .unwrap_or(h.text_range().end()),
                None => match container.kind() {
                    PROGRAM => container
                        .children()
                        .find(|n| n.kind() == PROGRAM_HEADER)
                        .map(|h| h.text_range().end())
                        .unwrap_or(container.text_range().start()),
                    _ => self.script_uses_insert_pos(root),
                },
            };
            let insert_in_block = self.directives.in_conditional_block(insert_pos);
            if let Some(h) = head {
                d = d.with_secondary(
                    self.span(h.first_token().unwrap().text_range()),
                    "first `uses` here",
                );
            }
            if head_semi_supplied || in_block || insert_in_block {
                d = d.with_note(
                    "`uses` is only read at the start of the file; move the items there",
                );
            } else {
                // Spelling of the item
                let existing: Vec<String> = head
                    .map(|h| {
                        h.children()
                            .filter(|c| c.kind() == USES_ITEM)
                            .filter_map(|i| i.children().find(|c| c.kind() == QUALIFIED_NAME))
                            .map(|q| q.text().to_string().to_ascii_lowercase())
                            .collect()
                    })
                    .unwrap_or_default();
                let items: Vec<String> = u
                    .children()
                    .filter(|c| c.kind() == USES_ITEM)
                    .filter(|i| {
                        i.children()
                            .find(|c| c.kind() == QUALIFIED_NAME)
                            .is_none_or(|q| {
                                !existing.contains(&q.text().to_string().to_ascii_lowercase())
                            })
                    })
                    .map(|i| i.text().to_string())
                    .collect();
                let del = crate::diagnostics::deletion_range(self.bytes, u.text_range());
                let mut edits = vec![TextEdit::delete(self.file, del)];
                if !items.is_empty() {
                    let text = if head.is_some() {
                        format!(", {}", items.join(", "))
                    } else if container.kind() == PROGRAM {
                        // Placed on a new line after the `program` heading (same form as inserting `uses` for E0700)
                        format!("\nuses {};", items.join(", "))
                    } else {
                        format!("uses {};\n", items.join(", "))
                    };
                    edits.push(TextEdit::insert(self.file, insert_pos, text));
                }
                edits.sort_by_key(|e| e.range.start());
                d = d.with_fix(Fix::safe("move the items to the first `uses`", edits));
            }
            out.push(d);
        }
        out
    }

    /// Position to insert `uses` in a script (start of the file, after BOM, shebang, and inline metadata).
    fn script_uses_insert_pos(&self, _root: &SyntaxNode) -> TextSize {
        let mut i = 0;
        let mut pos: u32 = 0;
        // Also skip the newline right after each of BOM, shebang, and inline metadata
        while i < self.tokens.len() {
            let t = self.tokens[i];
            let is_header = match t.kind {
                BOM | SHEBANG => true,
                BLOCK_COMMENT => {
                    self.bytes[t.start() as usize..t.end() as usize].starts_with(b"(*! puy")
                }
                _ => false,
            };
            if !is_header {
                break;
            }
            pos = t.end();
            i += 1;
            if let Some(n) = self.tokens.get(i)
                && n.kind == NEWLINE
            {
                pos = n.end();
                i += 1;
            }
        }
        pos.into()
    }
}

/// Shape of an expression in statement position (for the E0402 check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExprShape {
    /// `NAME_REF` / `FIELD_EXPR` / `CALL_EXPR` (may be a procedure call).
    Callable,
    /// Literal, binary/unary, parenthesized, index (cannot be a call).
    NotCallable,
    /// No expression (supplied).
    Hole,
}

#[cfg(test)]
mod tests;
