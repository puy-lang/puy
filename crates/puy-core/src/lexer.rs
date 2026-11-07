//! Lexical analysis (`docs/spec/lexical.md`). Tokenizes everything, including trivia (whitespace, comments, directives).
//!
//! - The lexer never stops. Erroneous characters become `ERROR` tokens with a diagnostic, and lexing continues.
//!   The only exception is invalid UTF-8 (E0001), where lexing stops at the first invalid byte (chapter 1).
//! - The token sequence covers the input bytes completely (chapter 4).
//! - It is context-free. The contents of directives are read by a separate small lexer in `directives`.

use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Stage};
use crate::source::{FileId, SourceFile, Span, TextRange, TextSize};
use crate::syntax::SyntaxKind;
use SyntaxKind::*;

/// A token. The position is a half-open range of byte offsets in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub range: TextRange,
}

impl Token {
    pub fn new(kind: SyntaxKind, start: u32, end: u32) -> Self {
        Token {
            kind,
            range: TextRange::new(start.into(), end.into()),
        }
    }

    pub fn start(&self) -> u32 {
        self.range.start().into()
    }

    pub fn end(&self) -> u32 {
        self.range.end().into()
    }
}

/// Result of lexical analysis.
#[derive(Debug, Clone)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    /// Position of the first invalid byte when stopped on E0001.
    pub stopped_at: Option<TextSize>,
}

impl LexOutput {
    /// Whether the file was stopped on E0001.
    pub fn is_invalid_utf8(&self) -> bool {
        self.stopped_at.is_some()
    }
}

/// Runs lexical analysis. `is_include` is set for files included with `{$I}` (for the W0015 variant).
pub fn lex(
    file: FileId,
    src: &SourceFile,
    sink: &mut DiagnosticSink,
    is_include: bool,
) -> LexOutput {
    let bytes = &src.bytes;
    let (limit, invalid) = match std::str::from_utf8(bytes) {
        Ok(_) => (bytes.len(), false),
        Err(e) => (e.valid_up_to(), true),
    };
    let mut lx = Lexer {
        file,
        bytes,
        limit,
        pos: 0,
        tokens: Vec::new(),
        diags: Vec::new(),
        is_include,
        head_comment_allowed: !is_include,
        shebang_allowed: true,
    };
    if bytes[..limit].starts_with(&[0xEF, 0xBB, 0xBF]) {
        lx.push(BOM, 3, Vec::new());
    }
    while lx.pos < limit {
        let start = lx.pos;
        let mut scratch = Vec::new();
        let (kind, end, unterminated) = lx.lex_token(&mut scratch);
        debug_assert!(end > start, "lexer must make progress");
        if invalid && end == limit && (unterminated || is_open_ended(kind)) {
            // Discard the half-read token and make everything from its start to the end of the file an ERROR
            lx.pos = start;
            break;
        }
        lx.push(kind, end, scratch);
    }
    if invalid {
        let start = lx.pos;
        lx.tokens
            .push(Token::new(ERROR, start as u32, bytes.len() as u32));
        lx.diags.push(
            Diagnostic::error(Code::E(1), "source is not valid UTF-8", Stage::Lexer)
                .with_primary(lx.span(limit, limit + 1), "")
                .with_note("Puy reads source files as UTF-8 only")
                .with_note("UTF-8 files saved by Delphi 2009 or later (with a BOM) need no change")
                .with_note(
                    "convert an older ANSI source (such as Shift_JIS) to UTF-8 before compiling",
                ),
        );
    }
    lx.tokens
        .push(Token::new(EOF, bytes.len() as u32, bytes.len() as u32));
    let diags = std::mem::take(&mut lx.diags);
    sink.extend(diags);
    LexOutput {
        tokens: lx.tokens,
        stopped_at: invalid.then(|| TextSize::from(limit as u32)),
    }
}

/// Tokens that end at the end of input (treated as half-read when stopped on E0001).
fn is_open_ended(kind: SyntaxKind) -> bool {
    matches!(kind, LINE_COMMENT | DOC_COMMENT | SHEBANG)
}

struct Lexer<'a> {
    file: FileId,
    bytes: &'a [u8],
    /// End of the readable part (the position of invalid UTF-8, if any).
    limit: usize,
    pos: usize,
    tokens: Vec<Token>,
    diags: Vec<Diagnostic>,
    is_include: bool,
    /// Whether the tokens so far are only BOM / shebang / whitespace / newlines (to check the position of inline metadata).
    head_comment_allowed: bool,
    /// Whether a shebang may appear here (up to right after the BOM).
    shebang_allowed: bool,
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_blank(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | 0x0B | 0x0C)
}

impl<'a> Lexer<'a> {
    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(
            self.file,
            TextRange::new((start as u32).into(), (end as u32).into()),
        )
    }

    fn at(&self, i: usize) -> u8 {
        if i < self.limit { self.bytes[i] } else { 0 }
    }

    fn has(&self, i: usize) -> bool {
        i < self.limit
    }

    fn push(&mut self, kind: SyntaxKind, end: usize, diags: Vec<Diagnostic>) {
        let start = self.pos;
        self.tokens.push(Token::new(kind, start as u32, end as u32));
        self.diags.extend(diags);
        self.pos = end;
        if !matches!(kind, BOM | SHEBANG | WHITESPACE | NEWLINE) {
            self.head_comment_allowed = false;
        }
        if kind != BOM {
            self.shebang_allowed = false;
        }
    }

    /// Cuts one token. Returns (kind, end, whether it was cut off by the end of input).
    fn lex_token(&mut self, diags: &mut Vec<Diagnostic>) -> (SyntaxKind, usize, bool) {
        let start = self.pos;
        let b = self.at(start);
        match b {
            b'\n' => (NEWLINE, start + 1, false),
            b'\r' => {
                if self.at(start + 1) == b'\n' {
                    (NEWLINE, start + 2, false)
                } else {
                    (NEWLINE, start + 1, false)
                }
            }
            b' ' | b'\t' | 0x0B | 0x0C => {
                let mut end = start + 1;
                while self.has(end) && is_blank(self.at(end)) {
                    end += 1;
                }
                (WHITESPACE, end, false)
            }
            b'/' if self.at(start + 1) == b'/' => {
                let end = self.line_end(start);
                let slashes = self.bytes[start..end]
                    .iter()
                    .take_while(|&&c| c == b'/')
                    .count();
                let kind = if slashes == 3 {
                    DOC_COMMENT
                } else {
                    LINE_COMMENT
                };
                self.check_nul(start, end, diags);
                (kind, end, false)
            }
            b'{' => self.lex_brace_comment(diags),
            b'(' if self.at(start + 1) == b'*' => self.lex_paren_comment(diags),
            b'*' if self.at(start + 1) == b')' => {
                diags.push(self.invalid_char_diag(start, start + 2, Code::E(3)));
                (ERROR, start + 2, false)
            }
            b'}' => {
                diags.push(self.invalid_char_diag(start, start + 1, Code::E(3)));
                (ERROR, start + 1, false)
            }
            b'#' => {
                if self.at(start + 1) == b'!' && self.shebang_allowed {
                    let end = self.line_end(start);
                    self.check_nul(start, end, diags);
                    (SHEBANG, end, false)
                } else if self.is_char_code_start(start + 1) {
                    self.lex_string(diags)
                } else {
                    diags.push(
                        Diagnostic::error(
                            Code::E(8),
                            "invalid character code after `#`",
                            Stage::Lexer,
                        )
                        .with_primary(self.span(start, start + 1), ""),
                    );
                    (ERROR, start + 1, false)
                }
            }
            b'\'' => self.lex_string(diags),
            b'0'..=b'9' => self.lex_number(diags),
            b'$' if self.at(start + 1).is_ascii_hexdigit() => self.lex_number(diags),
            b'%' if matches!(self.at(start + 1), b'0' | b'1') => self.lex_number(diags),
            b'&' if is_ident_start(self.at(start + 1)) => {
                let mut end = start + 2;
                while self.has(end) && is_ident_char(self.at(end)) {
                    end += 1;
                }
                (IDENT, end, false)
            }
            _ if is_ident_start(b) => {
                let mut end = start + 1;
                while self.has(end) && is_ident_char(self.at(end)) {
                    end += 1;
                }
                let text = std::str::from_utf8(&self.bytes[start..end]).unwrap();
                let lower = text.to_ascii_lowercase();
                let kind = SyntaxKind::reserved_word(&lower).unwrap_or(IDENT);
                (kind, end, false)
            }
            b'+' => (PLUS, start + 1, false),
            b'-' => (MINUS, start + 1, false),
            b'*' => (STAR, start + 1, false),
            b'/' => (SLASH, start + 1, false),
            b'=' => (EQ, start + 1, false),
            b'<' => match self.at(start + 1) {
                b'>' => (NE, start + 2, false),
                b'=' => (LE, start + 2, false),
                _ => (LT, start + 1, false),
            },
            b'>' => {
                if self.at(start + 1) == b'=' {
                    (GE, start + 2, false)
                } else {
                    (GT, start + 1, false)
                }
            }
            b':' => {
                if self.at(start + 1) == b'=' {
                    (ASSIGN, start + 2, false)
                } else {
                    (COLON, start + 1, false)
                }
            }
            b';' => (SEMICOLON, start + 1, false),
            b',' => (COMMA, start + 1, false),
            b'.' => {
                if self.at(start + 1) == b'.' {
                    (DOTDOT, start + 2, false)
                } else {
                    (DOT, start + 1, false)
                }
            }
            b'(' => (L_PAREN, start + 1, false),
            b')' => (R_PAREN, start + 1, false),
            b'[' => (L_BRACK, start + 1, false),
            b']' => (R_BRACK, start + 1, false),
            b'^' => (CARET, start + 1, false),
            b'@' => (AT, start + 1, false),
            _ => self.lex_invalid_run(diags),
        }
    }

    /// Position of the end of the line (just before the newline, or `limit`).
    fn line_end(&self, from: usize) -> usize {
        let mut end = from;
        while self.has(end) && !matches!(self.at(end), b'\n' | b'\r') {
            end += 1;
        }
        end
    }

    /// Whether what follows `#` can start a character code.
    fn is_char_code_start(&self, i: usize) -> bool {
        let b = self.at(i);
        // Treat `#$` / `#%` as a character code even without digits after it, so that the whole `#$` is a single E0008
        b.is_ascii_digit() || b == b'$' || b == b'%'
    }

    /// `{ ... }` / `{$ ... }`.
    fn lex_brace_comment(&mut self, diags: &mut Vec<Diagnostic>) -> (SyntaxKind, usize, bool) {
        let start = self.pos;
        let is_directive = self.at(start + 1) == b'$';
        let kind = if is_directive {
            DIRECTIVE
        } else {
            BLOCK_COMMENT
        };
        let mut end = start + 1;
        while self.has(end) {
            if self.at(end) == b'}' {
                let end = end + 1;
                self.check_nul(start, end, diags);
                return (kind, end, false);
            }
            end += 1;
        }
        self.check_nul(start, end, diags);
        diags.push(self.unterminated_diag(
            start,
            start + if is_directive { 2 } else { 1 },
            is_directive,
        ));
        (ERROR, end, true)
    }

    /// `(* ... *)` / `(*$ ... *)`.
    fn lex_paren_comment(&mut self, diags: &mut Vec<Diagnostic>) -> (SyntaxKind, usize, bool) {
        let start = self.pos;
        let is_directive = self.at(start + 2) == b'$';
        let kind = if is_directive {
            DIRECTIVE
        } else {
            BLOCK_COMMENT
        };
        let mut end = start + 2;
        while self.has(end) {
            if self.at(end) == b'*' && self.at(end + 1) == b')' {
                let end = end + 2;
                self.check_nul(start, end, diags);
                if !is_directive {
                    self.check_inline_metadata(start, end, diags);
                }
                return (kind, end, false);
            }
            end += 1;
        }
        self.check_nul(start, end, diags);
        diags.push(self.unterminated_diag(
            start,
            start + if is_directive { 3 } else { 2 },
            is_directive,
        ));
        (ERROR, end, true)
    }

    fn unterminated_diag(&self, start: usize, delim_end: usize, is_directive: bool) -> Diagnostic {
        let msg = if is_directive {
            "unterminated compiler directive"
        } else {
            "unterminated comment"
        };
        Diagnostic::error(Code::E(2), msg, Stage::Lexer)
            .with_primary(self.span(start, delim_end), "never closed")
    }

    /// W0015 if a comment starting with `(*! puy` + whitespace + newline is not at the start (or is inside an included file).
    fn check_inline_metadata(&self, start: usize, end: usize, diags: &mut Vec<Diagnostic>) {
        let body = &self.bytes[start + 2..end];
        if !is_inline_metadata_form(body) {
            return;
        }
        if self.is_include {
            diags.push(
                Diagnostic::warning(
                    Code::W(15),
                    "inline metadata is not read from an include file",
                    Stage::Lexer,
                )
                .with_primary(self.span(start, end), "")
                .with_note("move the `(*! puy` comment to the start of the file that is run"),
            );
        } else if !self.head_comment_allowed {
            diags.push(
                Diagnostic::warning(
                    Code::W(15),
                    "inline metadata is only recognized at the start of the file",
                    Stage::Lexer,
                )
                .with_primary(self.span(start, end), "")
                .with_note("move the `(*! puy` comment to the start of the file"),
            );
        }
    }

    /// NUL inside a comment, directive, string, or shebang (E0003; the token is not split).
    fn check_nul(&self, start: usize, end: usize, diags: &mut Vec<Diagnostic>) {
        let mut i = start;
        while i < end {
            if self.bytes[i] == 0 {
                let run_start = i;
                while i < end && self.bytes[i] == 0 {
                    i += 1;
                }
                diags.push(self.invalid_char_diag(run_start, i, Code::E(3)));
            } else {
                i += 1;
            }
        }
    }

    /// Makes a run of invalid characters (with the same code) a single `ERROR` token.
    fn lex_invalid_run(&mut self, diags: &mut Vec<Diagnostic>) -> (SyntaxKind, usize, bool) {
        let start = self.pos;
        let (code, first_len) = self
            .classify_invalid(start)
            .expect("invalid character expected");
        let mut end = start + first_len;
        while self.has(end) {
            match self.classify_invalid(end) {
                Some((c, len)) if c == code => end += len,
                _ => break,
            }
        }
        diags.push(self.invalid_char_diag(start, end, code));
        (ERROR, end, false)
    }

    /// If the character at position `i` is invalid, its code and byte length.
    fn classify_invalid(&self, i: usize) -> Option<(Code, usize)> {
        if !self.has(i) {
            return None;
        }
        let b = self.at(i);
        if b < 0x80 {
            let invalid = match b {
                b'\n' | b'\r' | b' ' | b'\t' | 0x0B | 0x0C => false,
                b'}' => true,
                b'*' => self.at(i + 1) == b')',
                b'$' => !self.at(i + 1).is_ascii_hexdigit(),
                b'%' => !matches!(self.at(i + 1), b'0' | b'1'),
                b'&' => !is_ident_start(self.at(i + 1)),
                b'#' | b'\'' | b'{' | b'(' | b')' | b'[' | b']' | b'+' | b'-' | b'/' | b'='
                | b'<' | b'>' | b':' | b';' | b',' | b'.' | b'^' | b'@' => false,
                _ if is_ident_char(b) => false,
                _ => true,
            };
            let len = if b == b'*' { 2 } else { 1 };
            return invalid.then_some((Code::E(3), len));
        }
        // Non-ASCII scalar value (valid UTF-8 up to limit)
        let s = std::str::from_utf8(&self.bytes[i..self.limit]).ok()?;
        let ch = s.chars().next()?;
        let code = if ch.is_alphabetic() || ch.is_numeric() {
            Code::E(6)
        } else {
            Code::E(3)
        };
        Some((code, ch.len_utf8()))
    }

    fn invalid_char_diag(&self, start: usize, end: usize, code: Code) -> Diagnostic {
        let shown = display_chars(&self.bytes[start..end]);
        let count = shown.chars().count();
        let msg = match code {
            Code::E(6) => {
                if count >= 2 {
                    "non-ASCII characters in identifier".to_string()
                } else {
                    "non-ASCII character in identifier".to_string()
                }
            }
            _ => {
                if count >= 2 {
                    format!("invalid characters `{shown}`")
                } else {
                    format!("invalid character `{shown}`")
                }
            }
        };
        let mut d =
            Diagnostic::error(code, msg, Stage::Lexer).with_primary(self.span(start, end), "");
        match code {
            Code::E(6) => {
                d = d.with_note("identifiers are limited to ASCII letters, digits and `_`")
            }
            _ => {
                let text = &self.bytes[start..end];
                if text == b"}" {
                    d = d.with_note("block comments do not nest, so this `}` has no matching `{`");
                } else if text == b"*)" {
                    d = d
                        .with_note("block comments do not nest, so this `*)` has no matching `(*`");
                }
            }
        }
        d
    }

    /// Numeric literal (3.6).
    fn lex_number(&mut self, diags: &mut Vec<Diagnostic>) -> (SyntaxKind, usize, bool) {
        let start = self.pos;
        let mut end = start;
        let mut bad = false;
        let mut kind = INT_LIT;
        let b = self.at(start);
        let digit_ok: fn(u8) -> bool = match b {
            b'$' => {
                end += 1;
                |c| c.is_ascii_hexdigit()
            }
            b'%' => {
                end += 1;
                |c| matches!(c, b'0' | b'1')
            }
            _ => |c| c.is_ascii_digit(),
        };
        let (e, ok) = self.digit_run(end, digit_ok);
        end = e;
        bad |= !ok;
        if b.is_ascii_digit() {
            // Fractional part
            if self.at(end) == b'.' && self.at(end + 1).is_ascii_digit() {
                if self.at(end - 1) == b'_' {
                    bad = true;
                }
                let (e, ok) = self.digit_run(end + 1, |c| c.is_ascii_digit());
                end = e;
                bad |= !ok;
                kind = FLOAT_LIT;
            }
            // Exponent part
            if matches!(self.at(end), b'e' | b'E') {
                if self.at(end - 1) == b'_' {
                    bad = true;
                }
                let mut e = end + 1;
                if matches!(self.at(e), b'+' | b'-') {
                    e += 1;
                }
                if self.at(e).is_ascii_digit() {
                    let (e2, ok) = self.digit_run(e, |c| c.is_ascii_digit());
                    end = e2;
                    bad |= !ok;
                    kind = FLOAT_LIT;
                } else {
                    bad = true;
                    end = e;
                }
            }
        }
        // If letters, digits, or `_` follow immediately, make the whole run a single ERROR
        if self.has(end) && is_ident_char(self.at(end)) {
            bad = true;
        }
        if bad {
            while self.has(end) && is_ident_char(self.at(end)) {
                end += 1;
            }
            diags.push(
                Diagnostic::error(Code::E(4), "invalid numeric literal", Stage::Lexer)
                    .with_primary(self.span(start, end), ""),
            );
            return (ERROR, end, false);
        }
        if kind == INT_LIT && int_literal_value(&self.bytes[start..end]).is_none() {
            diags.push(
                Diagnostic::error(Code::E(5), "integer literal is too large", Stage::Lexer)
                    .with_primary(self.span(start, end), ""),
            );
        }
        (kind, end, false)
    }

    /// Reads a run of digits and `_`. Returns (end, whether `_` is placed correctly).
    fn digit_run(&self, from: usize, digit_ok: fn(u8) -> bool) -> (usize, bool) {
        let mut end = from;
        let mut ok = true;
        let mut prev_underscore = false;
        let mut first = true;
        while self.has(end) {
            let c = self.at(end);
            if digit_ok(c) {
                prev_underscore = false;
            } else if c == b'_' {
                if first || prev_underscore {
                    ok = false;
                }
                prev_underscore = true;
            } else {
                break;
            }
            first = false;
            end += 1;
        }
        if prev_underscore {
            ok = false;
        }
        (end, ok)
    }

    /// String literal (3.5). Elements placed next to each other without whitespace.
    fn lex_string(&mut self, diags: &mut Vec<Diagnostic>) -> (SyntaxKind, usize, bool) {
        let start = self.pos;
        let mut end = start;
        loop {
            match self.at(end) {
                b'#' if self.is_char_code_start(end + 1) => {
                    let elem_start = end;
                    end += 1;
                    let digit_ok: fn(u8) -> bool = match self.at(end) {
                        b'$' => {
                            end += 1;
                            |c| c.is_ascii_hexdigit()
                        }
                        b'%' => {
                            end += 1;
                            |c| matches!(c, b'0' | b'1')
                        }
                        _ => |c| c.is_ascii_digit(),
                    };
                    let (e, ok) = self.digit_run(end, digit_ok);
                    end = e;
                    let value = int_literal_value(&self.bytes[elem_start + 1..end]);
                    // Value is 0..$10FFFF (65536 and above becomes two surrogates; `lexical.md` 3.5)
                    if !ok || !matches!(value, Some(v) if v <= 0x10FFFF) {
                        diags.push(
                            Diagnostic::error(
                                Code::E(8),
                                "invalid character code after `#`",
                                Stage::Lexer,
                            )
                            .with_primary(self.span(elem_start, end), ""),
                        );
                    }
                }
                b'\'' => {
                    let quotes = self.count_quotes(end);
                    if quotes >= 3 && quotes % 2 == 1 && self.rest_of_line_blank(end + quotes) {
                        match self.lex_multiline(end, quotes, diags) {
                            Some(e) => end = e,
                            None => {
                                // No terminator: ERROR up to the end of the file
                                self.check_nul(start, self.limit, diags);
                                return (ERROR, self.limit, true);
                            }
                        }
                    } else {
                        // Ordinary element: '' inside is a single '
                        let mut i = end + 1;
                        loop {
                            if !self.has(i) || matches!(self.at(i), b'\n' | b'\r') {
                                let line_end = self.line_end(start);
                                let unterminated = !self.has(i);
                                self.check_nul(start, line_end, diags);
                                diags.push(
                                    Diagnostic::error(
                                        Code::E(7),
                                        "string literal must not span lines",
                                        Stage::Lexer,
                                    )
                                    .with_primary(self.span(start, line_end), "")
                                    .with_note("use `'''` for a string that spans lines"),
                                );
                                return (ERROR, line_end, unterminated);
                            }
                            if self.at(i) == b'\'' {
                                if self.at(i + 1) == b'\'' {
                                    i += 2;
                                    continue;
                                }
                                i += 1;
                                break;
                            }
                            i += 1;
                        }
                        end = i;
                    }
                }
                _ => break,
            }
        }
        self.check_nul(start, end, diags);
        (STRING_LIT, end, false)
    }

    fn count_quotes(&self, from: usize) -> usize {
        let mut n = 0;
        while self.at(from + n) == b'\'' && self.has(from + n) {
            n += 1;
        }
        n
    }

    fn rest_of_line_blank(&self, from: usize) -> bool {
        let mut i = from;
        while self.has(i) {
            match self.at(i) {
                b'\n' | b'\r' => return true,
                c if is_blank(c) => i += 1,
                _ => return false,
            }
        }
        true
    }

    /// Multiline string. `open` is the position of the opening `'''`. Returns the position right after the closing `'''` (`None` if absent).
    fn lex_multiline(
        &self,
        open: usize,
        quotes: usize,
        diags: &mut Vec<Diagnostic>,
    ) -> Option<usize> {
        // Search from the line after the opening line
        let mut line_start = self.line_end(open);
        if !self.has(line_start) {
            diags.push(self.multiline_never_closed(open, quotes));
            return None;
        }
        loop {
            // Skip one newline
            if self.at(line_start) == b'\r' && self.at(line_start + 1) == b'\n' {
                line_start += 2;
            } else {
                line_start += 1;
            }
            let line_end = self.line_end(line_start);
            // Find the first run of `quotes` `'` characters on this line
            let mut i = line_start;
            while i + quotes <= line_end {
                if self.bytes[i..i + quotes].iter().all(|&c| c == b'\'') {
                    let close_end = i + quotes;
                    if !self.bytes[line_start..i].iter().all(|&c| is_blank(c)) {
                        diags.push(
                            Diagnostic::error(
                                Code::E(9),
                                "invalid multi-line string",
                                Stage::Lexer,
                            )
                            .with_primary(
                                self.span(line_start, i),
                                "text before the closing quotes",
                            ),
                        );
                    }
                    return Some(close_end);
                }
                i += 1;
            }
            if !self.has(line_end) {
                diags.push(self.multiline_never_closed(open, quotes));
                return None;
            }
            line_start = line_end;
        }
    }

    fn multiline_never_closed(&self, open: usize, quotes: usize) -> Diagnostic {
        Diagnostic::error(Code::E(9), "invalid multi-line string", Stage::Lexer)
            .with_primary(self.span(open, open + quotes), "never closed")
    }
}

/// Whether the contents between `(*` and `*)` have the inline metadata form (`! puy` + whitespace + newline).
fn is_inline_metadata_form(body: &[u8]) -> bool {
    let Some(rest) = body.strip_prefix(b"! puy") else {
        return false;
    };
    let mut i = 0;
    while i < rest.len() && matches!(rest[i], b' ' | b'\t') {
        i += 1;
    }
    matches!(rest.get(i), Some(b'\n' | b'\r'))
}

/// Whether the contents right after `(*` start with `! puy` (used by the driver to detect the E2507 variant).
pub fn starts_like_inline_metadata(body: &[u8]) -> bool {
    body.starts_with(b"! puy")
}

/// Value of an integer literal spelling (`42`, `$FF`, `%1010`, `1_000`). `None` if out of range.
pub fn int_literal_value(text: &[u8]) -> Option<u64> {
    let (radix, digits) = match text.first()? {
        b'$' => (16u64, &text[1..]),
        b'%' => (2u64, &text[1..]),
        _ => (10u64, text),
    };
    let mut value: u64 = 0;
    let mut any = false;
    for &c in digits {
        if c == b'_' {
            continue;
        }
        let d = (c as char).to_digit(radix as u32)? as u64;
        value = value.checked_mul(radix)?.checked_add(d)?;
        any = true;
    }
    any.then_some(value)
}

/// Position of an error in a multiline string's value (`shallower than the closing quotes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultilineError {
    pub range: TextRange,
}

/// Value of a string literal token (a sequence of UTF-16 code units).
///
/// The lexer only determines ranges, so the error of a body line being shallower than the closing `'''` (E0009) is found here.
/// `base` is the byte offset of the token's start (to return the error position).
pub fn string_literal_value(text: &[u8], base: u32) -> Result<Vec<u16>, MultilineError> {
    let mut out: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'#' => {
                let start = i + 1;
                let mut end = start;
                if matches!(text.get(end), Some(b'$' | b'%')) {
                    end += 1;
                }
                while end < text.len() && (text[end].is_ascii_alphanumeric() || text[end] == b'_') {
                    end += 1;
                }
                let v = int_literal_value(&text[start..end]).unwrap_or(0);
                if v > 0xFFFF {
                    // 65536..$10FFFF expands to a surrogate pair (`lexical.md` 3.5)
                    let v = (v.min(0x10FFFF) - 0x10000) as u32;
                    out.push((0xD800 + (v >> 10)) as u16);
                    out.push((0xDC00 + (v & 0x3FF)) as u16);
                } else {
                    out.push(v as u16);
                }
                i = end;
            }
            b'\'' => {
                let mut quotes = 0;
                while i + quotes < text.len() && text[i + quotes] == b'\'' {
                    quotes += 1;
                }
                let after = i + quotes;
                let rest_blank = {
                    let mut j = after;
                    while j < text.len() && is_blank(text[j]) {
                        j += 1;
                    }
                    j >= text.len() || matches!(text[j], b'\n' | b'\r')
                };
                if quotes >= 3 && quotes % 2 == 1 && rest_blank {
                    // Multiline string
                    let (value, end) = multiline_value(text, i, quotes, base)?;
                    out.extend(value);
                    i = end;
                } else {
                    let mut j = i + 1;
                    let mut s: Vec<u8> = Vec::new();
                    while j < text.len() {
                        if text[j] == b'\'' {
                            if text.get(j + 1) == Some(&b'\'') {
                                s.push(b'\'');
                                j += 2;
                                continue;
                            }
                            j += 1;
                            break;
                        }
                        s.push(text[j]);
                        j += 1;
                    }
                    push_utf8(&mut out, &s);
                    i = j;
                }
            }
            _ => {
                // Unreachable (the lexer guarantees the token's form)
                i += 1;
            }
        }
    }
    Ok(out)
}

fn push_utf8(out: &mut Vec<u16>, bytes: &[u8]) {
    let s = String::from_utf8_lossy(bytes);
    out.extend(s.encode_utf16());
}

/// Value of a multiline string. Returns (value, position right after the closing `'''`).
fn multiline_value(
    text: &[u8],
    open: usize,
    quotes: usize,
    base: u32,
) -> Result<(Vec<u16>, usize), MultilineError> {
    // Split into lines (starting from the line after the opening line)
    let mut lines: Vec<(usize, usize)> = Vec::new(); // (start, end) excluding the newline
    let mut i = open;
    while i < text.len() && !matches!(text[i], b'\n' | b'\r') {
        i += 1;
    }
    let skip_newline = |i: usize| -> usize {
        if text.get(i) == Some(&b'\r') && text.get(i + 1) == Some(&b'\n') {
            i + 2
        } else {
            i + 1
        }
    };
    let mut close: Option<(usize, usize)> = None; // (line start, position of the closing ''')
    while i < text.len() {
        let line_start = skip_newline(i);
        let mut line_end = line_start;
        while line_end < text.len() && !matches!(text[line_end], b'\n' | b'\r') {
            line_end += 1;
        }
        let mut k = line_start;
        let mut found = None;
        while k + quotes <= line_end {
            if text[k..k + quotes].iter().all(|&c| c == b'\'') {
                found = Some(k);
                break;
            }
            k += 1;
        }
        if let Some(k) = found {
            close = Some((line_start, k));
            break;
        }
        lines.push((line_start, line_end));
        i = line_end;
    }
    let Some((close_line_start, close_pos)) = close else {
        // The lexer has reported E0009. The value is empty
        return Ok((Vec::new(), text.len()));
    };
    let strip = close_pos - close_line_start;
    let mut value: Vec<u16> = Vec::new();
    for (n, &(ls, le)) in lines.iter().enumerate() {
        if n > 0 {
            value.push(b'\n' as u16);
        }
        let line = &text[ls..le];
        if line.iter().all(|&c| is_blank(c)) {
            continue;
        }
        if line.len() < strip || !line[..strip].iter().all(|&c| is_blank(c)) {
            let bad_end = ls + strip.min(line.len());
            return Err(MultilineError {
                range: TextRange::new((base + ls as u32).into(), (base + bad_end as u32).into()),
            });
        }
        push_utf8(&mut value, &line[strip..]);
    }
    Ok((value, close_pos + quotes))
}

/// Display of characters in excerpts and messages (`docs/spec/diagnostics.md` chapter 6): control characters other than tab
/// become control pictures, and invalid UTF-8 becomes U+FFFD.
pub fn display_chars(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    s.chars()
        .map(|c| match c {
            '\t' => '\t',
            '\x00'..='\x1f' => char::from_u32(0x2400 + c as u32).unwrap(),
            '\x7f' => '\u{2421}',
            c => c,
        })
        .collect()
}

/// Display of text in `tokens` and `cst` dumps (`docs/spec/lexical.md` chapter 5).
/// The valid parts use Rust's `Debug` for `str`, invalid bytes use `\xNN`. Includes the surrounding `"`.
pub fn debug_text(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    for chunk in bytes.utf8_chunks() {
        let valid = chunk.valid();
        if !valid.is_empty() {
            let dbg = format!("{valid:?}");
            out.push_str(&dbg[1..dbg.len() - 1]);
        }
        for b in chunk.invalid() {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out.push('"');
    out
}

/// `tokens` dump (chapter 5). One token per line, each line ending with a single newline.
pub fn dump_tokens(tokens: &[Token], bytes: &[u8]) -> String {
    let mut out = String::new();
    for t in tokens {
        let text = &bytes[t.start() as usize..t.end() as usize];
        out.push_str(&format!(
            "{}@{}..{} {}\n",
            t.kind.name(),
            t.start(),
            t.end(),
            debug_text(text)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_str(src: &[u8]) -> (Vec<Token>, Vec<Diagnostic>) {
        let file = SourceFile::new("t.pas", src.to_vec());
        let mut sink = DiagnosticSink::new();
        let out = lex(FileId(0), &file, &mut sink, false);
        (out.tokens, sink.into_sorted())
    }

    fn dump(src: &[u8]) -> String {
        let (tokens, _) = lex_str(src);
        dump_tokens(&tokens, src)
    }

    fn kinds(src: &[u8]) -> Vec<SyntaxKind> {
        lex_str(src).0.iter().map(|t| t.kind).collect()
    }

    fn codes(src: &[u8]) -> Vec<String> {
        lex_str(src)
            .1
            .iter()
            .map(|d| d.code.unwrap().to_string())
            .collect()
    }

    #[test]
    fn spec_example() {
        assert_eq!(
            dump(b"program Hello;\n"),
            "PROGRAM_KW@0..7 \"program\"\nWHITESPACE@7..8 \" \"\nIDENT@8..13 \"Hello\"\nSEMICOLON@13..14 \";\"\nNEWLINE@14..15 \"\\n\"\nEOF@15..15 \"\"\n"
        );
    }

    #[test]
    fn boundary_inputs() {
        assert_eq!(dump(b""), "EOF@0..0 \"\"\n");
        assert_eq!(
            dump(b"\xEF\xBB\xBF"),
            "BOM@0..3 \"\\u{feff}\"\nEOF@3..3 \"\"\n"
        );
        assert_eq!(
            dump(b"#!/usr/bin/env puy"),
            "SHEBANG@0..18 \"#!/usr/bin/env puy\"\nEOF@18..18 \"\"\n"
        );
        assert_eq!(dump(b"#!"), "SHEBANG@0..2 \"#!\"\nEOF@2..2 \"\"\n");
        assert!(dump(b"X := 1").ends_with("INT_LIT@5..6 \"1\"\nEOF@6..6 \"\"\n"));
        assert!(dump(b"X := 1\r").ends_with("NEWLINE@6..7 \"\\r\"\nEOF@7..7 \"\"\n"));
        assert_eq!(dump(b"\xFF"), "ERROR@0..1 \"\\xff\"\nEOF@1..1 \"\"\n");
        assert_eq!(codes(b"\xFF"), ["E0001"]);
        assert_eq!(dump(b"{ a\xFF"), "ERROR@0..4 \"{ a\\xff\"\nEOF@4..4 \"\"\n");
        assert_eq!(codes(b"{ a\xFF"), ["E0001"]);
        assert_eq!(dump(b"\0"), "ERROR@0..1 \"\\0\"\nEOF@1..1 \"\"\n");
        assert_eq!(codes(b"\0"), ["E0003"]);
    }

    #[test]
    fn invalid_utf8_after_complete_token() {
        let (tokens, diags) = lex_str(b"abc\xFFdef");
        assert_eq!(tokens[0], Token::new(IDENT, 0, 3));
        assert_eq!(tokens[1], Token::new(ERROR, 3, 7));
        assert_eq!(tokens[2].kind, EOF);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].primary.as_ref().unwrap().span.range,
            TextRange::new(3.into(), 4.into())
        );
    }

    #[test]
    fn invalid_utf8_inside_line_comment() {
        let (tokens, diags) = lex_str(b"// a\xFF\nX");
        assert_eq!(tokens[0], Token::new(ERROR, 0, 7));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn comments_and_directives() {
        assert_eq!(
            kinds(b"{ x } {$R+} (* y *) (*$Q-*) // z\n/// d\n//// e"),
            [
                BLOCK_COMMENT,
                WHITESPACE,
                DIRECTIVE,
                WHITESPACE,
                BLOCK_COMMENT,
                WHITESPACE,
                DIRECTIVE,
                WHITESPACE,
                LINE_COMMENT,
                NEWLINE,
                DOC_COMMENT,
                NEWLINE,
                LINE_COMMENT,
                EOF
            ]
        );
        // The closing delimiter of one comment style is plain content inside the other
        assert_eq!(kinds(b"{ *) }"), [BLOCK_COMMENT, EOF]);
        assert_eq!(kinds(b"(* } *)"), [BLOCK_COMMENT, EOF]);
        assert_eq!(codes(b"{ abc"), ["E0002"]);
        assert_eq!(kinds(b"{ abc"), [ERROR, EOF]);
        let (_, d) = lex_str(b"(*$X");
        assert_eq!(d[0].message, "unterminated compiler directive");
        assert_eq!(
            d[0].primary.as_ref().unwrap().span.range,
            TextRange::new(0.into(), 3.into())
        );
        assert_eq!(d[0].primary.as_ref().unwrap().label, "never closed");
    }

    #[test]
    fn unmatched_closers() {
        let (t, d) = lex_str(b"x } *) y");
        assert_eq!(
            t.iter().map(|t| t.kind).collect::<Vec<_>>(),
            [
                IDENT, WHITESPACE, ERROR, WHITESPACE, ERROR, WHITESPACE, IDENT, EOF
            ]
        );
        assert_eq!(d[0].message, "invalid character `}`");
        assert_eq!(
            d[0].notes[0],
            "block comments do not nest, so this `}` has no matching `{`"
        );
        assert_eq!(d[1].message, "invalid characters `*)`");
    }

    #[test]
    fn identifiers_and_keywords() {
        assert_eq!(
            kinds(b"Begin &begin _x1 forward"),
            [
                BEGIN_KW, WHITESPACE, IDENT, WHITESPACE, IDENT, WHITESPACE, IDENT, EOF
            ]
        );
        assert_eq!(kinds(b"&1"), [ERROR, INT_LIT, EOF]);
        assert_eq!(codes(b"&1"), ["E0003"]);
    }

    #[test]
    fn symbols() {
        assert_eq!(
            kinds(b"<= >= <> := .. ( ) [ ] ^ @ + - * / = < > : ; , ."),
            [
                LE, WHITESPACE, GE, WHITESPACE, NE, WHITESPACE, ASSIGN, WHITESPACE, DOTDOT,
                WHITESPACE, L_PAREN, WHITESPACE, R_PAREN, WHITESPACE, L_BRACK, WHITESPACE, R_BRACK,
                WHITESPACE, CARET, WHITESPACE, AT, WHITESPACE, PLUS, WHITESPACE, MINUS, WHITESPACE,
                STAR, WHITESPACE, SLASH, WHITESPACE, EQ, WHITESPACE, LT, WHITESPACE, GT,
                WHITESPACE, COLON, WHITESPACE, SEMICOLON, WHITESPACE, COMMA, WHITESPACE, DOT, EOF
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            kinds(b"42 $FF %1010 1_000 $FF_FF 1.5 1e10 2.5E-3"),
            [
                INT_LIT, WHITESPACE, INT_LIT, WHITESPACE, INT_LIT, WHITESPACE, INT_LIT, WHITESPACE,
                INT_LIT, WHITESPACE, FLOAT_LIT, WHITESPACE, FLOAT_LIT, WHITESPACE, FLOAT_LIT, EOF
            ]
        );
        assert_eq!(kinds(b"1..3"), [INT_LIT, DOTDOT, INT_LIT, EOF]);
        assert_eq!(kinds(b"1."), [INT_LIT, DOT, EOF]);
        assert_eq!(kinds(b"1.e5"), [INT_LIT, DOT, IDENT, EOF]);
        for bad in [
            &b"1e"[..],
            b"123abc",
            b"$FFG",
            b"%12",
            b"0x1F",
            b"1__0",
            b"1_",
            b"1_.5",
        ] {
            assert_eq!(kinds(bad), [ERROR, EOF], "{}", String::from_utf8_lossy(bad));
            assert_eq!(codes(bad), ["E0004"], "{}", String::from_utf8_lossy(bad));
        }
        assert_eq!(kinds(b"$ $G"), [ERROR, WHITESPACE, ERROR, IDENT, EOF]);
        assert_eq!(codes(b"$ $G"), ["E0003", "E0003"]);
        assert_eq!(kinds(b"%2"), [ERROR, INT_LIT, EOF]);
        assert_eq!(codes(b"18446744073709551615"), Vec::<String>::new());
        assert_eq!(codes(b"18446744073709551616"), ["E0005"]);
        assert_eq!(kinds(b"18446744073709551616"), [INT_LIT, EOF]);
        assert_eq!(int_literal_value(b"$FF_FF"), Some(0xFFFF));
        assert_eq!(int_literal_value(b"%1010"), Some(10));
    }

    #[test]
    fn strings() {
        assert_eq!(
            kinds(b"'a' 'it''s' '' #13#10 'a'#13#10'b' #$0D #%1101 #1_000"),
            [
                STRING_LIT, WHITESPACE, STRING_LIT, WHITESPACE, STRING_LIT, WHITESPACE, STRING_LIT,
                WHITESPACE, STRING_LIT, WHITESPACE, STRING_LIT, WHITESPACE, STRING_LIT, WHITESPACE,
                STRING_LIT, EOF
            ]
        );
        assert_eq!(kinds(b"'a' 'b'"), [STRING_LIT, WHITESPACE, STRING_LIT, EOF]);
        assert_eq!(
            string_literal_value(b"'it''s'", 0).unwrap(),
            "it's".encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(
            string_literal_value(b"'a'#13#10'b'", 0).unwrap(),
            vec![97, 13, 10, 98]
        );
        assert_eq!(
            string_literal_value("'日本'".as_bytes(), 0).unwrap(),
            "日本".encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(string_literal_value(b"#$D800", 0).unwrap(), vec![0xD800]);
        // 65536..$10FFFF expands to a surrogate pair; beyond $10FFFF is E0008
        assert_eq!(
            string_literal_value(b"#$1F600", 0).unwrap(),
            vec![0xD83D, 0xDE00]
        );
        assert_eq!(
            string_literal_value(b"#$10FFFF'a'#65536", 0).unwrap(),
            vec![0xDBFF, 0xDFFF, 97, 0xD800, 0xDC00]
        );
        assert_eq!(codes(b"#$1F600"), [] as [&str; 0]);
        assert_eq!(codes(b"#$110000"), ["E0008"]);
        assert_eq!(kinds(b"#x"), [ERROR, IDENT, EOF]);
        assert_eq!(codes(b"#x"), ["E0008"]);
        assert_eq!(kinds(b"#"), [ERROR, EOF]);
        // An unterminated string is ERROR up to the end of the line
        let (t, d) = lex_str(b"S := 'abc;\nWriteLn(S);");
        assert_eq!(t[4], Token::new(ERROR, 5, 10));
        assert_eq!(t[5].kind, NEWLINE);
        assert_eq!(d[0].code, Some(Code::E(7)));
        assert_eq!(
            d[0].primary.as_ref().unwrap().span.range,
            TextRange::new(5.into(), 10.into())
        );
    }

    #[test]
    fn multiline_strings() {
        let src = b"var S := '''\n  line 1\n    line 2\n  ''';\n";
        let (t, d) = lex_str(src);
        assert!(d.is_empty(), "{d:?}");
        let lit = t.iter().find(|t| t.kind == STRING_LIT).unwrap();
        assert_eq!(lit.range, TextRange::new(9.into(), 38.into()));
        let v = string_literal_value(&src[9..38], 9).unwrap();
        assert_eq!(String::from_utf16(&v).unwrap(), "line 1\n  line 2");
        // Opening check: if characters follow ''', it is an ordinary element
        assert_eq!(kinds(b"'''a'"), [STRING_LIT, EOF]);
        assert_eq!(string_literal_value(b"'''a'", 0).unwrap(), vec![39, 97]);
        assert_eq!(string_literal_value(b"''''", 0).unwrap(), vec![39]);
        assert_eq!(string_literal_value(b"''''''", 0).unwrap(), vec![39, 39]);
        // No terminator
        assert_eq!(kinds(b"'''\n  abc\n"), [ERROR, EOF]);
        let (_, d) = lex_str(b"'''\n  abc\n");
        assert_eq!(d[0].code, Some(Code::E(9)));
        assert_eq!(d[0].primary.as_ref().unwrap().label, "never closed");
        // Characters before the terminator
        let (_, d) = lex_str(b"'''\n  abc\nx ''';\n");
        assert_eq!(
            d[0].primary.as_ref().unwrap().label,
            "text before the closing quotes"
        );
        assert_eq!(
            d[0].primary.as_ref().unwrap().span.range,
            TextRange::new(10.into(), 12.into())
        );
        // Body is shallower
        let src = b"'''\n a\n  '''";
        let e = string_literal_value(src, 0).unwrap_err();
        assert_eq!(e.range, TextRange::new(4.into(), 6.into()));
        // Empty lines are kept as is
        let src = b"'''\n  a\n\n  b\n  '''";
        let v = string_literal_value(src, 0).unwrap();
        assert_eq!(String::from_utf16(&v).unwrap(), "a\n\nb");
        // Five quotes allow ''' in the body
        let src = b"'''''\n  '''\n  '''''";
        let v = string_literal_value(src, 0).unwrap();
        assert_eq!(String::from_utf16(&v).unwrap(), "'''");
    }

    #[test]
    fn invalid_characters_merge() {
        let (t, d) = lex_str(b"x && y \"abc\" !=");
        assert_eq!(
            t.iter().map(|t| t.kind).collect::<Vec<_>>(),
            [
                IDENT, WHITESPACE, ERROR, WHITESPACE, IDENT, WHITESPACE, ERROR, IDENT, ERROR,
                WHITESPACE, ERROR, EQ, EOF
            ]
        );
        assert_eq!(d[0].message, "invalid characters `&&`");
        assert_eq!(d[1].message, "invalid character `\"`");
        assert_eq!(d.len(), 4);
        // Non-ASCII
        let (t, d) = lex_str("x := 変数;".as_bytes());
        assert_eq!(
            t.iter().map(|t| t.kind).collect::<Vec<_>>(),
            [IDENT, WHITESPACE, ASSIGN, WHITESPACE, ERROR, SEMICOLON, EOF]
        );
        assert_eq!(d[0].code, Some(Code::E(6)));
        assert_eq!(d[0].message, "non-ASCII characters in identifier");
        let (_, d) = lex_str("x → y".as_bytes());
        assert_eq!(d[0].code, Some(Code::E(3)));
        assert_eq!(d[0].message, "invalid character `→`");
        // U+FEFF not at the start
        let (_, d) = lex_str("x\u{feff}".as_bytes());
        assert_eq!(d[0].code, Some(Code::E(3)));
        // NUL is shown as a control picture
        let (_, d) = lex_str(b"\0\0");
        assert_eq!(d[0].message, "invalid characters `\u{2400}\u{2400}`");
        assert_eq!(d[0].severity, crate::diagnostics::Severity::Error);
    }

    #[test]
    fn nul_inside_comment_keeps_token() {
        let (t, d) = lex_str(b"{ a\0\0b } 'x\0'");
        assert_eq!(
            t.iter().map(|t| t.kind).collect::<Vec<_>>(),
            [BLOCK_COMMENT, WHITESPACE, STRING_LIT, EOF]
        );
        assert_eq!(d.len(), 2);
        assert_eq!(
            d[0].primary.as_ref().unwrap().span.range,
            TextRange::new(3.into(), 5.into())
        );
    }

    #[test]
    fn shebang_only_at_head() {
        assert_eq!(kinds(b"\xEF\xBB\xBF#!x\n"), [BOM, SHEBANG, NEWLINE, EOF]);
        // `#!` not at the start: `#` is E0008 and `!` is E0003 (different codes, so separate tokens)
        assert_eq!(kinds(b" #!x"), [WHITESPACE, ERROR, ERROR, IDENT, EOF]);
    }

    #[test]
    fn inline_metadata_warning() {
        let ok = b"#!/usr/bin/env puy\n(*! puy\ntoolchain = \"1\"\n*)\n";
        assert!(codes(ok).is_empty());
        let misplaced = b"// c\n(*! puy\n*)\n";
        assert_eq!(codes(misplaced), ["W0015"]);
        let not_meta = b"// c\n(*!!! note *)\n";
        assert!(codes(not_meta).is_empty());
        // In an included file, the W0015 variant applies even at the start
        let file = SourceFile::new("i.inc", b"(*! puy\n*)".to_vec());
        let mut sink = DiagnosticSink::new();
        lex(FileId(1), &file, &mut sink, true);
        let d = sink.into_sorted();
        assert_eq!(
            d[0].message,
            "inline metadata is not read from an include file"
        );
    }

    #[test]
    fn debug_text_escapes() {
        assert_eq!(debug_text(b"a\n\t\"\\\0"), "\"a\\n\\t\\\"\\\\\\0\"");
        assert_eq!(debug_text(b"\xEF\xBB\xBF"), "\"\\u{feff}\"");
        assert_eq!(debug_text("日本".as_bytes()), "\"日本\"");
        assert_eq!(debug_text(b"{ a\xff"), "\"{ a\\xff\"");
        assert_eq!(debug_text(b"\x1b"), "\"\\u{1b}\"");
    }

    #[test]
    fn tokens_cover_source() {
        for src in [
            &b"program X; begin WriteLn('a') end."[..],
            b"{ x",
            b"'abc\nd",
            b"a\xFF",
            b"\0x\0",
            b"1e 2",
        ] {
            let (tokens, _) = lex_str(src);
            let mut pos = 0;
            for t in &tokens {
                assert_eq!(t.start(), pos);
                pos = t.end();
            }
            assert_eq!(pos as usize, src.len());
        }
    }
}
