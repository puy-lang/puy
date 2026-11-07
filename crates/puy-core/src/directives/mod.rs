//! Directive processing (`docs/spec/directives.md`). A stage between lexing and parsing.
//!
//! - Evaluates conditional compilation and folds the tokens of inactive ranges into a single `INACTIVE` token.
//! - Includes `{$I}` files (tokens of an included file are not put into the CST; only the result of evaluation remains).
//! - Builds the "position → state" table for switch directives (`{$Q}` / `{$R}` / `{$C}` / `{$WARN}`).
//! - Directives not in the list are compile errors.

mod expr;
mod table;

use std::collections::HashSet;

use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Fix, Severity, Stage, TextEdit, codes};
use crate::lexer::{self, Token};
use crate::source::{
    FileId, MAX_SOURCE_SIZE, SourceFile, SourceMap, SourceProvider, Span, TOO_LARGE_MESSAGE,
    TextRange, TextSize,
};
use crate::syntax::SyntaxKind::*;
use puy_host::HostErrorKind;
pub use table::IgnoreCategory;

/// Maximum depth of `{$I}` inclusion (`directives.md` chapter 5).
pub const MAX_INCLUDE_DEPTH: usize = 16;

/// Build kind and symbols (`docs/spec/directives.md` 2.5, 3.3).
#[derive(Debug, Clone)]
pub struct BuildConfig {
    /// Whether this is a release build (`{$Q-}` / `{$R-}` / `{$C-}` are the defaults, and `RELEASE` is defined).
    pub release: bool,
    /// `puy test` (`{$C}` is always `+`).
    pub test: bool,
    /// Symbols from `--define` / `[defines]`.
    pub defines: Vec<String>,
    /// `PuyVersion` (`major * 10000 + minor * 100 + patch`) and `PuyVersionString`.
    pub version: i64,
    pub version_string: String,
}

impl Default for BuildConfig {
    fn default() -> Self {
        BuildConfig {
            release: false,
            test: false,
            defines: Vec::new(),
            version: crate::version_number(),
            version_string: crate::VERSION.to_string(),
        }
    }
}

/// State of `{$WARN}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarnState {
    On,
    Off,
    Error,
}

/// Switch state at a position (2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switches {
    /// `{$Q}`
    pub overflow_checks: bool,
    /// `{$R}`
    pub range_checks: bool,
    /// `{$C}`
    pub assertions: bool,
    /// Those changed by `{$WARN}` (`DEFAULT` removes the entry).
    pub warns: Vec<(Code, WarnState)>,
}

impl Switches {
    pub fn initial(config: &BuildConfig) -> Self {
        Switches {
            overflow_checks: !config.release,
            range_checks: !config.release,
            assertions: !config.release || config.test,
            warns: Vec::new(),
        }
    }

    /// State of a warning. The default if not changed by `{$WARN}` (`puy.toml` is M5; defaults from chapter 5).
    pub fn warn_state(&self, code: Code) -> WarnState {
        if let Some((_, s)) = self.warns.iter().find(|(c, _)| *c == code) {
            return *s;
        }
        if codes::WARNINGS_OFF_BY_DEFAULT.contains(&code) {
            WarnState::Off
        } else {
            WarnState::On
        }
    }

    fn set_warn(&mut self, code: Code, state: Option<WarnState>) {
        self.warns.retain(|(c, _)| *c != code);
        if let Some(s) = state {
            self.warns.push((code, s));
        }
    }
}

/// Position → state table. Each entry is "the state after this token", keyed by the end position of the token of the
/// switch directive (or `{$I}`).
#[derive(Debug, Clone)]
pub struct SwitchTable {
    initial: Switches,
    entries: Vec<(TextSize, Switches)>,
}

impl SwitchTable {
    pub fn new(initial: Switches) -> Self {
        SwitchTable {
            initial,
            entries: Vec::new(),
        }
    }

    /// State at position `pos` (the last switch ending before `pos`).
    pub fn at(&self, pos: TextSize) -> &Switches {
        let mut cur = &self.initial;
        for (k, s) in &self.entries {
            if *k <= pos {
                cur = s;
            } else {
                break;
            }
        }
        cur
    }

    fn push(&mut self, key: TextSize, state: Switches) {
        if let Some(last) = self.entries.last_mut()
            && last.0 == key
        {
            last.1 = state;
            return;
        }
        self.entries.push((key, state));
    }
}

/// Kind of directive, used by the parser to decide W0005 (3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Conditional compilation directives and `INACTIVE`. `block_start` is the position of the directive that opens the block.
    Conditional { block_start: TextSize },
    /// Switch directives (including `{$I}` that changed the state).
    Switch,
}

/// Result of directive processing (one file).
#[derive(Debug, Clone)]
pub struct DirectiveOutput {
    /// The token sequence after folding `INACTIVE`.
    pub tokens: Vec<Token>,
    pub switches: SwitchTable,
    /// Start positions and kinds of `DIRECTIVE` / `INACTIVE` tokens (ascending order of position).
    pub placements: Vec<(TextSize, Placement)>,
    /// Ranges of conditional compilation blocks (from the opening directive to the end of the matching `{$ENDIF}`; to the end of the file if unclosed).
    pub blocks: Vec<TextRange>,
    /// When `INACTIVE` runs to the end of the file because of E0203, its start position (syntax errors after it are suppressed).
    pub suppress_after: Option<TextSize>,
    /// Included files and the position of `{$I}` in the file passed to `puy` (same as in `ProcessContext`).
    pub include_positions: Vec<(FileId, TextSize)>,
}

impl DirectiveOutput {
    pub fn placement_at(&self, pos: TextSize) -> Option<Placement> {
        self.placements
            .binary_search_by_key(&pos, |(p, _)| *p)
            .ok()
            .map(|i| self.placements[i].1)
    }

    /// Whether the position is inside a conditional compilation block.
    pub fn in_conditional_block(&self, pos: TextSize) -> bool {
        self.blocks.iter().any(|b| b.contains(pos))
    }
}

/// Processing context (shared across files).
pub struct ProcessContext<'a> {
    pub config: &'a BuildConfig,
    pub provider: &'a mut dyn SourceProvider,
    pub sources: &'a mut SourceMap,
    pub sink: &'a mut DiagnosticSink,
    /// Included files and the position of `{$I}` in the file passed to `puy` (the `{$WARN}` state of warnings
    /// inside an included file is determined by the state at that position; chapter 5).
    pub include_positions: Vec<(FileId, TextSize)>,
}

/// Processes the directives of the file passed to `puy`.
pub fn process(file: FileId, tokens: Vec<Token>, cx: &mut ProcessContext<'_>) -> DirectiveOutput {
    let mut symbols: HashSet<String> = table::BUILTIN_SYMBOLS
        .iter()
        .map(|s| s.to_string())
        .collect();
    symbols.insert(
        if cx.config.release {
            "RELEASE"
        } else {
            "DEBUG"
        }
        .to_string(),
    );
    for d in &cx.config.defines {
        symbols.insert(d.to_ascii_uppercase());
    }
    let mut table = SwitchTable::new(Switches::initial(cx.config));
    let mut state = table.initial.clone();
    let root_path = cx.sources.get(file).path.clone();
    let mut fp = FileProcessor::new(
        file,
        tokens,
        cx,
        &mut symbols,
        &mut table,
        &mut state,
        None,
        vec![root_path],
    );
    let (tokens, placements, blocks, suppress_after) = fp.run();
    DirectiveOutput {
        tokens,
        switches: table,
        placements,
        blocks,
        suppress_after,
        include_positions: std::mem::take(&mut cx.include_positions),
    }
}

type RunResult = (
    Vec<Token>,
    Vec<(TextSize, Placement)>,
    Vec<TextRange>,
    Option<TextSize>,
);

struct CondFrame {
    /// The token of the opening directive.
    start: TextRange,
    /// Whether the opening directive was in an active range (the token is kept and evaluated).
    kept: bool,
    parent_active: bool,
    taken: bool,
    active: bool,
    seen_else: bool,
}

struct FileProcessor<'c, 'a> {
    cx: &'c mut ProcessContext<'a>,
    file: FileId,
    bytes: Vec<u8>,
    tokens: Vec<Token>,
    symbols: &'c mut HashSet<String>,
    table: &'c mut SwitchTable,
    state: &'c mut Switches,
    /// Inside an included file, the end position of the including `{$I}`.
    key_override: Option<TextSize>,
    /// Inclusion path (starting from the file passed to `puy`).
    chain: Vec<String>,
    stack: Vec<CondFrame>,
    out: Vec<Token>,
    inactive_start: Option<u32>,
    placements: Vec<(TextSize, Placement)>,
    blocks: Vec<TextRange>,
    /// Number of evaluated (active-range) uses of compiler-identification symbols (for the `notes` of W0007).
    compiler_symbol_uses: usize,
    /// W0007 for the first use (issued at the end of the file, with a note on the remaining count).
    pending_w7: Option<Diagnostic>,
    /// Whether a switch changed while processing this file (decides `Placement::Switch` for `{$I}`).
    changed: bool,
    /// Start position when `INACTIVE` runs to the end of the file because of E0203.
    inactive_eof_start: Option<TextSize>,
}

impl<'c, 'a> FileProcessor<'c, 'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        file: FileId,
        tokens: Vec<Token>,
        cx: &'c mut ProcessContext<'a>,
        symbols: &'c mut HashSet<String>,
        table: &'c mut SwitchTable,
        state: &'c mut Switches,
        key_override: Option<TextSize>,
        chain: Vec<String>,
    ) -> Self {
        let bytes = cx.sources.get(file).bytes.clone();
        FileProcessor {
            cx,
            file,
            bytes,
            tokens,
            symbols,
            table,
            state,
            key_override,
            chain,
            stack: Vec::new(),
            out: Vec::new(),
            inactive_start: None,
            placements: Vec::new(),
            blocks: Vec::new(),
            compiler_symbol_uses: 0,
            pending_w7: None,
            changed: false,
            inactive_eof_start: None,
        }
    }

    fn is_include(&self) -> bool {
        self.key_override.is_some()
    }

    fn span(&self, start: u32, end: u32) -> Span {
        Span::new(self.file, TextRange::new(start.into(), end.into()))
    }

    fn span_of(&self, range: TextRange) -> Span {
        Span::new(self.file, range)
    }

    fn active(&self) -> bool {
        self.stack.last().is_none_or(|f| f.active)
    }

    fn run(&mut self) -> RunResult {
        let tokens = std::mem::take(&mut self.tokens);
        for tok in &tokens {
            if tok.kind == DIRECTIVE {
                self.handle_directive(*tok);
            } else if tok.kind == EOF {
                self.finish_at_eof(*tok);
            } else if self.active() {
                self.out.push(*tok);
            }
        }
        let suppress = self.suppress_after_eof();
        if let Some(mut d) = self.pending_w7.take() {
            let rest = self.compiler_symbol_uses.saturating_sub(1);
            if rest == 1 {
                d = d.with_note("1 more use of a compiler symbol in this file is not reported");
            } else if rest > 1 {
                d = d.with_note(format!(
                    "{rest} more uses of compiler symbols in this file are not reported"
                ));
            }
            self.cx.sink.push(d);
        }
        let out = std::mem::take(&mut self.out);
        let mut placements = std::mem::take(&mut self.placements);
        placements.sort_by_key(|(p, _)| *p);
        let blocks = std::mem::take(&mut self.blocks);
        (out, placements, blocks, suppress)
    }

    fn suppress_after_eof(&self) -> Option<TextSize> {
        self.inactive_eof_start
    }

    /// Records that an `INACTIVE` range has started.
    fn begin_inactive(&mut self, from: u32) {
        if self.inactive_start.is_none() {
            self.inactive_start = Some(from);
        }
    }

    /// Emits an `INACTIVE` token (nothing if the range is empty).
    fn flush_inactive(&mut self, upto: u32) {
        if let Some(start) = self.inactive_start.take()
            && upto > start
        {
            self.out.push(Token::new(INACTIVE, start, upto));
            let block_start = self
                .stack
                .iter()
                .rev()
                .find(|f| f.kept)
                .map(|f| f.start.start());
            if let Some(bs) = block_start {
                self.placements
                    .push((start.into(), Placement::Conditional { block_start: bs }));
            }
        }
    }

    fn finish_at_eof(&mut self, eof: Token) {
        let eof_pos = eof.start();
        // Unclosed branches (E0203). Issued from the innermost (only one remains since they share a position)
        let unclosed: Vec<TextRange> = self
            .stack
            .iter()
            .rev()
            .filter(|f| f.kept)
            .map(|f| f.start)
            .collect();
        let inactive = !self.active();
        for start in unclosed {
            let d = Diagnostic::error(
                Code::E(203),
                "unterminated conditional directive",
                Stage::Directives,
            )
            .with_primary(self.span(eof_pos, eof_pos), "")
            .with_secondary(self.span_of(start), "starts here");
            self.cx.sink.push(d);
        }
        for f in self.stack.drain(..) {
            if f.kept {
                self.blocks
                    .push(TextRange::new(f.start.start(), eof_pos.into()));
            }
        }
        if inactive {
            self.inactive_eof_start = self.inactive_start.map(TextSize::from);
        }
        self.flush_inactive(eof_pos);
        self.out.push(eof);
    }

    fn handle_directive(&mut self, tok: Token) {
        let (content_start, content_end) = self.content_range(tok);
        let content = &self.bytes[content_start as usize..content_end as usize];
        let name_len = content
            .iter()
            .take_while(|b| b.is_ascii_alphabetic())
            .count();
        let name = std::str::from_utf8(&content[..name_len])
            .unwrap()
            .to_string();
        let upper = name.to_ascii_uppercase();
        let (args, args_start) = {
            let rest = &content[name_len..];
            let lead = rest.iter().take_while(|b| b.is_ascii_whitespace()).count();
            let trimmed = String::from_utf8_lossy(rest[lead..].trim_ascii_end()).into_owned();
            (trimmed, content_start + name_len as u32 + lead as u32)
        };
        let is_conditional = matches!(
            upper.as_str(),
            "IFDEF" | "IFNDEF" | "IF" | "ELSEIF" | "ELSE" | "ENDIF" | "IFEND"
        );
        if !is_conditional {
            if self.active() {
                self.out.push(tok);
                self.handle_plain_directive(tok, &name, &upper, &args, content_start, args_start);
            }
            return;
        }
        match upper.as_str() {
            "IFDEF" | "IFNDEF" | "IF" => {
                let parent_active = self.active();
                if !parent_active {
                    self.stack.push(CondFrame {
                        start: tok.range,
                        kept: false,
                        parent_active: false,
                        taken: true,
                        active: false,
                        seen_else: false,
                    });
                    return;
                }
                self.out.push(tok);
                self.placements.push((
                    tok.range.start(),
                    Placement::Conditional {
                        block_start: tok.range.start(),
                    },
                ));
                let cond = if upper == "IF" {
                    self.eval_if(tok, &args, args_start)
                } else {
                    let defined = self.eval_ifdef(tok, &args, args_start);
                    if upper == "IFDEF" { defined } else { !defined }
                };
                self.stack.push(CondFrame {
                    start: tok.range,
                    kept: true,
                    parent_active: true,
                    taken: cond,
                    active: cond,
                    seen_else: false,
                });
                if !cond {
                    self.begin_inactive(tok.end());
                }
            }
            "ELSE" | "ELSEIF" => {
                let Some(top) = self.stack.last() else {
                    self.unmatched(tok, &upper);
                    return;
                };
                if !top.kept {
                    return; // nested inside an inactive range
                }
                let block_start = top.start.start();
                self.flush_inactive(tok.start());
                self.out.push(tok);
                self.placements
                    .push((tok.range.start(), Placement::Conditional { block_start }));
                let frame = self.stack.last_mut().unwrap();
                if frame.seen_else {
                    frame.active = false;
                    frame.taken = true;
                    let d = Diagnostic::error(
                        Code::E(204),
                        "`{$ELSE}` after `{$ELSE}`",
                        Stage::Directives,
                    )
                    .with_primary(self.span_of(tok.range), "");
                    self.cx.sink.push(d);
                    self.begin_inactive(tok.end());
                    return;
                }
                if upper == "ELSE" {
                    frame.seen_else = true;
                    let active = !frame.taken;
                    frame.active = active;
                    frame.taken = true;
                    if !active {
                        self.begin_inactive(tok.end());
                    }
                } else {
                    let taken = frame.taken;
                    let active = if taken {
                        false
                    } else {
                        self.eval_if(tok, &args, args_start)
                    };
                    let frame = self.stack.last_mut().unwrap();
                    frame.active = active;
                    frame.taken |= active;
                    if !active {
                        self.begin_inactive(tok.end());
                    }
                }
            }
            _ => {
                // ENDIF / IFEND
                let Some(top) = self.stack.last() else {
                    self.unmatched(tok, &upper);
                    return;
                };
                if !top.kept {
                    self.stack.pop();
                    return;
                }
                self.flush_inactive(tok.start());
                self.out.push(tok);
                let frame = self.stack.pop().unwrap();
                self.placements.push((
                    tok.range.start(),
                    Placement::Conditional {
                        block_start: frame.start.start(),
                    },
                ));
                self.blocks
                    .push(TextRange::new(frame.start.start(), tok.range.end()));
                debug_assert!(frame.parent_active);
            }
        }
    }

    fn unmatched(&mut self, tok: Token, upper: &str) {
        if !self.active() {
            return;
        }
        self.out.push(tok);
        self.placements.push((
            tok.range.start(),
            Placement::Conditional {
                block_start: tok.range.start(),
            },
        ));
        let d = Diagnostic::error(
            Code::E(202),
            format!("`{{${upper}}}` without matching `{{$IF}}`"),
            Stage::Directives,
        )
        .with_primary(self.span_of(tok.range), "");
        self.cx.sink.push(d);
    }

    /// Range of the contents of a `DIRECTIVE` token (from just after `$` up to before the closing delimiter).
    fn content_range(&self, tok: Token) -> (u32, u32) {
        let text = &self.bytes[tok.start() as usize..tok.end() as usize];
        let (lead, trail) = if text.starts_with(b"{") {
            (2, 1)
        } else {
            (3, 2)
        };
        (tok.start() + lead, tok.end() - trail)
    }

    /// The directive as written (for messages; normalized to the form `{$NAME args}`).
    fn directive_text(&self, tok: Token) -> String {
        let (s, e) = self.content_range(tok);
        let content = String::from_utf8_lossy(&self.bytes[s as usize..e as usize]);
        format!("{{${}}}", content.trim())
    }

    fn push_state(&mut self, tok: Token) {
        let key = self.key_override.unwrap_or(tok.range.end());
        self.table.push(key, self.state.clone());
        self.changed = true;
        if !self.is_include() {
            self.placements.push((tok.range.start(), Placement::Switch));
        }
    }

    fn e0213(&mut self, tok: Token, name: &str) {
        let d = Diagnostic::error(
            Code::E(213),
            format!("invalid argument for `{{${}}}`", name.to_ascii_uppercase()),
            Stage::Directives,
        )
        .with_primary(self.span_of(tok.range), "");
        self.cx.sink.push(d);
    }

    fn ignored(&mut self, tok: Token, category: IgnoreCategory) {
        let text = self.directive_text(tok);
        let msg = if category.is_fpc() {
            format!("FPC-only directive `{text}` is ignored")
        } else {
            format!("directive `{text}` is ignored")
        };
        let range = crate::diagnostics::deletion_range(&self.bytes, tok.range);
        let mut d = Diagnostic::warning(Code::W(4), msg, Stage::Directives)
            .with_primary(self.span_of(tok.range), "")
            .with_fix(Fix::safe(
                format!("remove `{text}`"),
                vec![TextEdit::delete(self.file, range)],
            ));
        for n in category.notes() {
            d = d.with_note(*n);
        }
        self.cx.sink.push(d);
    }

    fn e0201(&mut self, tok: Token, shown: &str, note: &str) {
        let d = Diagnostic::error(
            Code::E(201),
            format!("directive `{shown}` is not supported in Puy"),
            Stage::Directives,
        )
        .with_primary(self.span_of(tok.range), "")
        .with_note(note);
        self.cx.sink.push(d);
    }

    /// Directives other than conditional compilation (inside an active range).
    fn handle_plain_directive(
        &mut self,
        tok: Token,
        name: &str,
        upper: &str,
        args: &str,
        content_start: u32,
        args_start: u32,
    ) {
        if name.is_empty() {
            let d = Diagnostic::error(
                Code::E(200),
                "`{$` starts a compiler directive",
                Stage::Directives,
            )
            .with_primary(self.span_of(tok.range), "")
            .with_note("write `{ $` with a space after `{` if this is a comment");
            self.cx.sink.push(d);
            return;
        }
        let on_off = parse_on_off(args);
        match upper {
            "DEFINE" | "UNDEF" => {
                if !is_symbol_name(args) {
                    self.e0213(tok, upper);
                    return;
                }
                let sym = args.to_ascii_uppercase();
                if let Some(msg) = builtin_symbol_message(&sym) {
                    let mut d = Diagnostic::error(Code::E(213), msg, Stage::Directives)
                        .with_primary(self.span_of(tok.range), "");
                    if sym == "DEBUG" || sym == "RELEASE" {
                        d = d.with_note(
                            "the build type is chosen by the `puy` command, not by the source",
                        );
                    }
                    self.cx.sink.push(d);
                    return;
                }
                if upper == "DEFINE" {
                    self.symbols.insert(sym);
                } else {
                    self.symbols.remove(&sym);
                }
            }
            "I" => {
                if let Some(on) = on_off {
                    self.iochecks(tok, on, "{$I-}");
                } else if is_comma_switches(args) {
                    self.comma_switches(tok, "I", args);
                } else {
                    self.include(tok, args);
                }
            }
            "INCLUDE" => self.include(tok, args),
            "Q" | "OVERFLOWCHECKS" => {
                if let Some(on) = on_off {
                    self.state.overflow_checks = on;
                    self.push_state(tok);
                } else if upper == "Q" && is_comma_switches(args) {
                    self.comma_switches(tok, "Q", args);
                } else {
                    self.e0213(tok, upper);
                }
            }
            "R" => {
                if let Some(on) = on_off {
                    self.state.range_checks = on;
                    self.push_state(tok);
                } else if is_comma_switches(args) {
                    self.comma_switches(tok, "R", args);
                } else if args.trim().is_empty() {
                    self.e0213(tok, upper);
                } else {
                    self.ignored(tok, IgnoreCategory::Resource);
                }
            }
            "RANGECHECKS" => {
                if let Some(on) = on_off {
                    self.state.range_checks = on;
                    self.push_state(tok);
                } else {
                    self.e0213(tok, upper);
                }
            }
            "C" | "ASSERTIONS" => {
                if let Some(on) = on_off {
                    self.state.assertions = on;
                    self.push_state(tok);
                } else if upper == "C" && is_comma_switches(args) {
                    self.comma_switches(tok, "C", args);
                } else {
                    self.e0213(tok, upper);
                }
            }
            "WARN" => self.warn_directive(tok, args),
            "MESSAGE" => self.message_directive(tok, args, None),
            "WARNING" => self.message_directive(tok, args, Some("WARN")),
            "ERROR" => self.message_directive(tok, args, Some("ERROR")),
            "FATAL" => self.message_directive(tok, args, Some("FATAL")),
            "HINT" | "NOTE" | "INFO" => self.message_directive(tok, args, Some("HINT")),
            "IFOPT" => {
                // A kind of conditional compilation: count the depth and branch
                self.ifopt(tok, args);
            }
            "POINTERS" | "POINTERMATH" => match on_off {
                Some(true) => {
                    let d = Diagnostic::error(
                        Code::E(212),
                        format!("`{{${upper} ON}}` is not supported yet"),
                        Stage::Directives,
                    )
                    .with_primary(self.span_of(tok.range), "");
                    self.cx.sink.push(d);
                }
                Some(false) => {}
                None => self.e0213(tok, upper),
            },
            "H" | "LONGSTRINGS" => match on_off {
                Some(true) => {}
                Some(false) => {
                    let shown = if upper == "H" {
                        "{$H-}"
                    } else {
                        "{$LONGSTRINGS OFF}"
                    };
                    self.e0201(
                        tok,
                        shown,
                        "`{$H-}` would make `string` a short string, which Puy does not have",
                    );
                }
                None if upper == "H" && is_comma_switches(args) => {
                    self.comma_switches(tok, "H", args)
                }
                None => self.e0213(tok, upper),
            },
            "ZEROBASEDSTRINGS" => match on_off {
                Some(false) => {}
                Some(true) => self.e0201(
                    tok,
                    "{$ZEROBASEDSTRINGS ON}",
                    "string indexes in Puy always start at 1",
                ),
                None => self.e0213(tok, upper),
            },
            "IOCHECKS" => match on_off {
                Some(on) => self.iochecks(tok, on, "{$IOCHECKS OFF}"),
                None => self.e0213(tok, upper),
            },
            "MODE" => {
                let m = args.to_ascii_uppercase();
                if m == "DELPHI" || m == "DELPHIUNICODE" {
                } else {
                    self.ignored(tok, IgnoreCategory::Fpc);
                }
            }
            "M" => {
                if on_off.is_some() {
                    self.ignored(tok, IgnoreCategory::Rtti);
                } else if is_comma_switches(args) {
                    self.comma_switches(tok, "M", args);
                } else {
                    self.ignored(tok, IgnoreCategory::Codegen);
                }
            }
            _ => match table::lookup(upper) {
                Some(table::Handling::Silent) => {
                    // Directives that take a state are limited to `ON` / `OFF` (`INLINE` also allows `AUTO`) (2.2)
                    let a = args.trim().to_ascii_uppercase();
                    let ok = match upper {
                        "INLINE" => matches!(a.as_str(), "ON" | "OFF" | "AUTO"),
                        "LEGACYIFEND" => matches!(a.as_str(), "ON" | "OFF"),
                        _ => true,
                    };
                    if !ok {
                        self.e0213(tok, upper);
                    }
                }
                Some(table::Handling::Ignored(cat)) => {
                    if upper.len() == 1 && is_comma_switches(args) {
                        self.comma_switches(tok, upper, args);
                    } else {
                        self.ignored(tok, cat);
                    }
                }
                Some(table::Handling::Implemented) | Some(table::Handling::ByArgument) => {
                    // Handled above
                    self.e0213(tok, upper);
                }
                None => {
                    let name_range = TextRange::new(
                        content_start.into(),
                        (content_start + name.len() as u32).into(),
                    );
                    let mut d = Diagnostic::error(
                        Code::E(200),
                        format!("unknown compiler directive `{}`", self.directive_text(tok)),
                        Stage::Directives,
                    )
                    .with_primary(self.span_of(name_range), "");
                    if let Some(s) = codes::similar_name(upper, table::names()) {
                        d = d.with_fix(Fix::suggestion(
                            format!("replace with `{s}`"),
                            vec![TextEdit::replace(self.file, name_range, s)],
                        ));
                    }
                    self.cx.sink.push(d);
                }
            },
        }
        let _ = args_start;
    }

    fn iochecks(&mut self, tok: Token, on: bool, shown: &str) {
        if !on {
            self.e0201(
                tok,
                shown,
                "I/O errors in Puy are always exceptions; there is no `IOResult`",
            );
        }
    }

    /// The comma form `{$R+,Q-}`. `first` is the one-letter name of the directive.
    fn comma_switches(&mut self, tok: Token, first: &str, args: &str) {
        let mut items: Vec<(String, bool)> = Vec::new();
        let mut ok = true;
        for (i, part) in args.split(',').enumerate() {
            let part = part.trim();
            if i == 0 {
                match part {
                    "+" => items.push((first.to_ascii_uppercase(), true)),
                    "-" => items.push((first.to_ascii_uppercase(), false)),
                    _ => ok = false,
                }
            } else if part.len() == 2
                && part.as_bytes()[0].is_ascii_alphabetic()
                && matches!(part.as_bytes()[1], b'+' | b'-')
            {
                items.push((part[..1].to_ascii_uppercase(), part.as_bytes()[1] == b'+'));
            } else {
                ok = false;
            }
        }
        if !ok {
            self.e0213(tok, first);
            return;
        }
        let mut changed = false;
        let mut ignored: Option<IgnoreCategory> = None;
        let mut kept: Vec<String> = Vec::new();
        let mut dropped: Vec<String> = Vec::new();
        for (letter, on) in items {
            let part = format!("{letter}{}", if on { "+" } else { "-" });
            let is_ignored = matches!(letter.as_str(), "M")
                || matches!(table::lookup(&letter), Some(table::Handling::Ignored(_)));
            if is_ignored {
                dropped.push(part);
            } else {
                kept.push(part);
            }
            match letter.as_str() {
                "Q" => {
                    self.state.overflow_checks = on;
                    changed = true;
                }
                "R" => {
                    self.state.range_checks = on;
                    changed = true;
                }
                "C" => {
                    self.state.assertions = on;
                    changed = true;
                }
                "H" => {
                    if !on {
                        self.e0201(
                            tok,
                            "{$H-}",
                            "`{$H-}` would make `string` a short string, which Puy does not have",
                        );
                    }
                }
                "I" => self.iochecks(tok, on, "{$I-}"),
                "M" => ignored = ignored.or(Some(IgnoreCategory::Rtti)),
                _ => match table::lookup(&letter) {
                    Some(table::Handling::Ignored(cat)) => ignored = ignored.or(Some(cat)),
                    Some(_) => {}
                    None => {
                        self.e0213(tok, first);
                        return;
                    }
                },
            }
        }
        if changed {
            self.push_state(tok);
        }
        if let Some(cat) = ignored {
            if kept.is_empty() {
                self.ignored(tok, cat);
            } else {
                // Effective and ignored switches are mixed: remove only the ignored part (chapter 1)
                let text = self.directive_text(tok);
                let rebuilt = format!("{{${}}}", kept.join(","));
                let mut d = Diagnostic::warning(
                    Code::W(4),
                    format!("`{}` in directive `{text}` is ignored", dropped.join(",")),
                    Stage::Directives,
                )
                .with_primary(self.span_of(tok.range), "")
                .with_fix(Fix::safe(
                    format!("replace with `{rebuilt}`"),
                    vec![TextEdit::replace(self.file, tok.range, rebuilt)],
                ));
                for n in cat.notes() {
                    d = d.with_note(*n);
                }
                self.cx.sink.push(d);
            }
        }
    }

    fn warn_directive(&mut self, tok: Token, args: &str) {
        let mut parts = args.split_ascii_whitespace();
        let (Some(name), Some(state), None) = (parts.next(), parts.next(), parts.next()) else {
            self.e0213(tok, "WARN");
            return;
        };
        let state = match state.to_ascii_uppercase().as_str() {
            "ON" => Some(WarnState::On),
            "OFF" => Some(WarnState::Off),
            "ERROR" => Some(WarnState::Error),
            "DEFAULT" => None,
            _ => {
                self.e0213(tok, "WARN");
                return;
            }
        };
        let code = codes::warning_by_name(name)
            .or_else(|| codes::delphi_warning_name(name).and_then(codes::warning_by_name));
        let Some(code) = code else {
            let name_start = tok.start()
                + self.bytes[tok.start() as usize..]
                    .windows(name.len())
                    .position(|w| w == name.as_bytes())
                    .unwrap_or(0) as u32;
            let name_range =
                TextRange::new(name_start.into(), (name_start + name.len() as u32).into());
            let mut d = Diagnostic::warning(
                Code::W(14),
                format!("unknown warning name `{name}`"),
                Stage::Directives,
            )
            .with_primary(self.span_of(tok.range), "");
            if let Some(s) = codes::similar_name(name, codes::warning_names()) {
                d = d.with_fix(Fix::suggestion(
                    format!("replace with `{s}`"),
                    vec![TextEdit::replace(self.file, name_range, s)],
                ));
            }
            self.cx.sink.push(d);
            return;
        };
        self.state.set_warn(code, state);
        self.push_state(tok);
    }

    fn message_directive(&mut self, tok: Token, args: &str, fpc_kind: Option<&str>) {
        let (kind, text) = if let Some(k) = fpc_kind {
            // FPC form: the string may or may not be quoted
            let text = if args.starts_with('\'') {
                match parse_quoted(args) {
                    Some(t) => t,
                    None => args.to_string(),
                }
            } else {
                args.to_string()
            };
            (k.to_string(), text)
        } else {
            let (kind, rest) = if args.starts_with('\'') {
                ("HINT".to_string(), args)
            } else {
                let word: String = args
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                let rest = args[word.len()..].trim_start();
                (word.to_ascii_uppercase(), rest)
            };
            if !matches!(kind.as_str(), "HINT" | "WARN" | "ERROR" | "FATAL") {
                self.e0213(tok, "MESSAGE");
                return;
            }
            let Some(text) = parse_quoted(rest) else {
                self.e0213(tok, "MESSAGE");
                return;
            };
            (kind, text)
        };
        let d = match kind.as_str() {
            "HINT" => Diagnostic::new(
                Code::W(8),
                Severity::Note,
                format!("user hint: {text}"),
                Stage::Directives,
            ),
            "WARN" => Diagnostic::warning(
                Code::W(8),
                format!("user warning: {text}"),
                Stage::Directives,
            ),
            _ => Diagnostic::error(
                Code::E(211),
                format!("user error: {text}"),
                Stage::Directives,
            ),
        };
        self.cx
            .sink
            .push(d.with_primary(self.span_of(tok.range), ""));
    }

    /// `{$IFOPT X+}`. Treated as the start of conditional compilation.
    fn ifopt(&mut self, tok: Token, args: &str) {
        // `handle_directive` does not treat IFOPT as conditional compilation, so it comes here.
        // The token has already been emitted. Only push the branch frame
        let b = args.as_bytes();
        let cond = if b.len() == 2 && b[0].is_ascii_alphabetic() && matches!(b[1], b'+' | b'-') {
            let on = b[1] == b'+';
            let cur = match b[0].to_ascii_uppercase() {
                b'Q' => Some(self.state.overflow_checks),
                b'R' => Some(self.state.range_checks),
                b'C' => Some(self.state.assertions),
                b'H' | b'I' => Some(true),
                _ => None,
            };
            match cur {
                Some(c) => c == on,
                None => {
                    let d = Diagnostic::error(
                        Code::E(214),
                        format!("`{}` cannot test this switch", self.directive_text(tok)),
                        Stage::Directives,
                    )
                    .with_primary(self.span_of(tok.range), "");
                    self.cx.sink.push(d);
                    false
                }
            }
        } else {
            self.e0213(tok, "IFOPT");
            false
        };
        // Mark the already emitted DIRECTIVE token as conditional compilation
        self.placements.push((
            tok.range.start(),
            Placement::Conditional {
                block_start: tok.range.start(),
            },
        ));
        self.stack.push(CondFrame {
            start: tok.range,
            kept: true,
            parent_active: true,
            taken: cond,
            active: cond,
            seen_else: false,
        });
        if !cond {
            self.begin_inactive(tok.end());
        }
    }

    /// Decides `{$IFDEF}` / `{$IFNDEF}`. Also issues W0007.
    fn eval_ifdef(&mut self, tok: Token, args: &str, args_start: u32) -> bool {
        if !is_symbol_name(args) {
            let name = if self.bytes[tok.start() as usize..].starts_with(b"{$IFN")
                || self.bytes[tok.start() as usize..].starts_with(b"(*$IFN")
            {
                "IFNDEF"
            } else {
                "IFDEF"
            };
            let name = self.directive_name_upper(tok).unwrap_or(name.to_string());
            self.e0213(tok, &name);
            return false;
        }
        self.symbol_use(args, args_start, args_start + args.len() as u32)
    }

    fn directive_name_upper(&self, tok: Token) -> Option<String> {
        let (s, e) = self.content_range(tok);
        let content = &self.bytes[s as usize..e as usize];
        let n = content
            .iter()
            .take_while(|b| b.is_ascii_alphabetic())
            .count();
        (n > 0).then(|| String::from_utf8_lossy(&content[..n]).to_ascii_uppercase())
    }

    /// Whether a symbol is defined. OS, CPU, and compiler symbols (not defined by the user) are always false, with W0007.
    fn symbol_use(&mut self, name: &str, start: u32, end: u32) -> bool {
        let upper = name.to_ascii_uppercase();
        if self.symbols.contains(&upper) {
            return true;
        }
        if table::is_platform_symbol(&upper) {
            let d = Diagnostic::warning(
                Code::W(7),
                format!("conditional symbol `{name}` is never defined in Puy"),
                Stage::Directives,
            )
            .with_primary(self.span(start, end), "")
            .with_note("a Puy program runs unchanged on every platform, so OS and CPU symbols are never defined")
            .with_note("check the platform at run time with `TOSVersion.Platform`");
            self.cx.sink.push(d);
        } else if table::is_compiler_symbol(&upper) {
            self.compiler_symbol_uses += 1;
            if self.pending_w7.is_none() {
                let d = Diagnostic::warning(
                    Code::W(7),
                    format!("conditional symbol `{name}` is never defined in Puy"),
                    Stage::Directives,
                )
                .with_primary(self.span(start, end), "")
                .with_note("write the branch for Puy with `{$IFDEF PUY}`");
                self.pending_w7 = Some(d);
            }
        }
        false
    }

    /// The expression of `{$IF}` / `{$ELSEIF}`.
    fn eval_if(&mut self, tok: Token, args: &str, args_start: u32) -> bool {
        struct Cx<'x, 'c, 'a> {
            fp: &'x mut FileProcessor<'c, 'a>,
            base: u32,
        }
        impl expr::Context for Cx<'_, '_, '_> {
            fn is_defined(&self, name: &str) -> bool {
                self.fp.symbols.contains(&name.to_ascii_uppercase())
                    || table::is_platform_symbol(&name.to_ascii_uppercase())
                    || table::is_compiler_symbol(&name.to_ascii_uppercase())
            }
            fn note_symbol_use(&mut self, name: &str, start: usize, end: usize) {
                // If not defined, issue the warning for OS, CPU, and compiler symbols
                self.fp
                    .symbol_use(name, self.base + start as u32, self.base + end as u32);
            }
            fn version(&self) -> (i64, &str) {
                (self.fp.cx.config.version, &self.fp.cx.config.version_string)
            }
        }
        // Evaluate (`is_defined` also answers "defined" for warned symbols, so the actual value is decided by `symbol_use`)
        let mut cx = Cx {
            fp: self,
            base: args_start,
        };
        let result = expr::evaluate(args, &mut cx);
        match result {
            Ok(v) => {
                // An OS, CPU, or compiler symbol written in `Defined()` is always false, so
                // instead of using the result evaluated as defined, re-evaluate with the real definitions
                struct Real<'x, 'c, 'a> {
                    fp: &'x mut FileProcessor<'c, 'a>,
                }
                impl expr::Context for Real<'_, '_, '_> {
                    fn is_defined(&self, name: &str) -> bool {
                        self.fp.symbols.contains(&name.to_ascii_uppercase())
                    }
                    fn note_symbol_use(&mut self, _: &str, _: usize, _: usize) {}
                    fn version(&self) -> (i64, &str) {
                        (self.fp.cx.config.version, &self.fp.cx.config.version_string)
                    }
                }
                let mut real = Real { fp: self };
                expr::evaluate(args, &mut real).unwrap_or(v)
            }
            Err(e) => {
                self.if_error(tok, args, args_start, e);
                false
            }
        }
    }

    fn if_error(&mut self, tok: Token, args: &str, args_start: u32, e: expr::EvalError) {
        let (s, en) = e.range();
        let span = self.span(args_start + s as u32, args_start + en as u32);
        let d = match e {
            expr::EvalError::Syntax { .. } => Diagnostic::error(
                Code::E(205),
                "invalid expression in `{$IF}`",
                Stage::Directives,
            )
            .with_primary(span, ""),
            expr::EvalError::UnknownName {
                name,
                bool_position,
                ..
            } => {
                let mut d = Diagnostic::error(
                    Code::E(206),
                    format!("unknown name `{name}` in `{{$IF}}` expression"),
                    Stage::Directives,
                )
                .with_primary(span, "");
                if bool_position {
                    d = d.with_fix(Fix::suggestion(
                        format!("replace with `Defined({name})`"),
                        vec![TextEdit::replace(
                            self.file,
                            span.range,
                            format!("Defined({name})"),
                        )],
                    ));
                }
                d
            }
            expr::EvalError::Declared { .. } => Diagnostic::error(
                Code::E(207),
                "`Declared()` is not supported in `{$IF}`",
                Stage::Directives,
            )
            .with_primary(span, "")
            .with_note(
                "use `Defined(…)` for a conditional symbol, or test the condition at run time",
            ),
            expr::EvalError::CompilerVersion { name, fix, .. } => {
                let mut d = Diagnostic::error(
                    Code::E(208),
                    format!("`{name}` is not defined in Puy"),
                    Stage::Directives,
                )
                .with_primary(span, "");
                let is_elseif = self.directive_name_upper(tok).as_deref() == Some("ELSEIF");
                match fix {
                    Some(positive) => {
                        let replacement = match (is_elseif, positive) {
                            (false, true) => "{$IFDEF PUY}",
                            (false, false) => "{$IFNDEF PUY}",
                            (true, true) => "{$ELSEIF Defined(PUY)}",
                            (true, false) => "{$ELSEIF not Defined(PUY)}",
                        };
                        d = d.with_fix(Fix::safe(
                            format!("replace with `{replacement}`"),
                            vec![TextEdit::replace(self.file, tok.range, replacement)],
                        ));
                    }
                    None => {
                        d = d.with_note("write the branch for Puy with `{$IFDEF PUY}`");
                    }
                }
                d
            }
            expr::EvalError::TypeMismatch { .. } => Diagnostic::error(
                Code::E(215),
                "type mismatch in `{$IF}` expression",
                Stage::Directives,
            )
            .with_primary(span, ""),
            expr::EvalError::DivByZero { .. } => Diagnostic::error(
                Code::E(215),
                "division by zero in `{$IF}` expression",
                Stage::Directives,
            )
            .with_primary(span, ""),
        };
        let _ = args;
        self.cx.sink.push(d);
    }

    /// `{$I filename}`.
    fn include(&mut self, tok: Token, args: &str) {
        let arg = args.trim();
        let arg = if arg.len() >= 2 && arg.starts_with('\'') && arg.ends_with('\'') {
            &arg[1..arg.len() - 1]
        } else {
            arg
        };
        if arg.is_empty() || (arg.starts_with('%') && arg.ends_with('%') && arg.len() >= 2) {
            self.e0213(tok, "I");
            return;
        }
        let path = resolve_include_path(&self.cx.sources.get(self.file).path, arg);
        // Inclusion depth is limited to 16 (`directives.md` chapter 5; `chain` is the list of including files,
        // starting with the file passed to `puy`). Cycles use the same code; both show the path in `notes`.
        let too_deep = self.chain.len() > MAX_INCLUDE_DEPTH;
        if too_deep || self.chain.contains(&path) {
            let msg = if too_deep {
                "include files nested too deeply".to_string()
            } else {
                format!("include file `{path}` includes itself")
            };
            let mut chain = self.chain.clone();
            chain.push(path.clone());
            let d = Diagnostic::error(Code::E(216), msg, Stage::Directives)
                .with_primary(self.span_of(tok.range), "")
                .with_note(format!("include chain: {}", chain.join(" -> ")));
            self.cx.sink.push(d);
            return;
        }
        let bytes = match self.cx.provider.read(&path) {
            Ok(b) => {
                if b.len() as u64 > MAX_SOURCE_SIZE {
                    self.cannot_read(tok, &path, TOO_LARGE_MESSAGE);
                    return;
                }
                b
            }
            Err(e) => {
                if e.kind == HostErrorKind::NotFound {
                    let d = Diagnostic::error(
                        Code::E(209),
                        format!("include file `{path}` not found"),
                        Stage::Directives,
                    )
                    .with_primary(self.span_of(tok.range), "")
                    .with_note("include file names are matched exactly on Linux and WASI")
                    .with_note("write the name with the same case as the file on disk");
                    self.cx.sink.push(d);
                } else {
                    let text = e.fixed_text().to_string();
                    self.cannot_read(tok, &path, &text);
                }
                return;
            }
        };
        let inc_file = self.cx.sources.add(SourceFile::new(path.clone(), bytes));
        // Warnings inside an included file are decided by the `{$WARN}` state at the position of `{$I}` in the file passed to `puy`
        let root_pos = self.key_override.unwrap_or(tok.range.start());
        self.cx.include_positions.push((inc_file, root_pos));
        let src = self.cx.sources.get(inc_file).clone();
        let lexed = lexer::lex(inc_file, &src, self.cx.sink, true);
        if lexed.is_invalid_utf8() {
            return;
        }
        let key = self.key_override.unwrap_or(tok.range.end());
        let mut chain = self.chain.clone();
        chain.push(path);
        let before = self.state.clone();
        let mut sub = FileProcessor::new(
            inc_file,
            lexed.tokens,
            self.cx,
            self.symbols,
            self.table,
            self.state,
            Some(key),
            chain,
        );
        let (tokens, _, _, _) = sub.run();
        let changed = sub.changed;
        // E0210 if there are tokens other than directives and comments
        if let Some(bad) = tokens
            .iter()
            .find(|t| !(t.kind.is_trivia() || matches!(t.kind, DIRECTIVE | ERROR | EOF)))
        {
            let d = Diagnostic::error(
                Code::E(210),
                "include file must contain only directives and comments",
                Stage::Directives,
            )
            .with_primary(Span::new(inc_file, bad.range), "")
            .with_secondary(self.span_of(tok.range), "included from here")
            .with_note(
                "move the declarations and statements into the including file, or into a unit",
            );
            self.cx.sink.push(d);
        }
        if changed && *self.state != before {
            self.changed = true;
            if !self.is_include() {
                self.placements.push((tok.range.start(), Placement::Switch));
            }
        } else if changed {
            self.changed = true;
        }
    }

    fn cannot_read(&mut self, tok: Token, path: &str, why: &str) {
        let d = Diagnostic::error(
            Code::E(209),
            format!("cannot read include file `{path}`: {why}"),
            Stage::Directives,
        )
        .with_primary(self.span_of(tok.range), "");
        self.cx.sink.push(d);
    }
}

/// How `{$I}` paths are joined (chapter 5): the including file's path up to its last separator is kept as a prefix,
/// followed by the argument as a string with `\` read as `/`. Absolute paths are used as is.
pub fn resolve_include_path(including: &str, arg: &str) -> String {
    let arg = arg.replace('\\', "/");
    if std::path::Path::new(&arg).is_absolute() || arg.starts_with('/') {
        return arg;
    }
    let cut = including.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    format!("{}{}", &including[..cut], arg)
}

fn parse_on_off(args: &str) -> Option<bool> {
    match args.to_ascii_uppercase().as_str() {
        "+" | "ON" => Some(true),
        "-" | "OFF" => Some(false),
        _ => None,
    }
}

fn is_comma_switches(args: &str) -> bool {
    (args.starts_with('+') || args.starts_with('-')) && args.contains(',')
}

/// Form of a symbol name (chapter 4).
pub fn is_symbol_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

fn builtin_symbol_message(upper: &str) -> Option<String> {
    if upper == "DEBUG" || upper == "RELEASE" {
        return Some(format!(
            "`{upper}` is defined by the build type and cannot be changed"
        ));
    }
    if table::BUILTIN_SYMBOLS.contains(&upper) {
        return Some(format!("`{upper}` is always defined and cannot be changed"));
    }
    None
}

/// Value of a single-element `'...'` string literal.
fn parse_quoted(s: &str) -> Option<String> {
    let s = s.trim();
    let inner = s.strip_prefix('\'')?;
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    loop {
        match chars.next()? {
            '\'' => {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    out.push('\'');
                } else {
                    break;
                }
            }
            c => out.push(c),
        }
    }
    if chars.next().is_some() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySourceProvider;

    struct Run {
        out: DirectiveOutput,
        diags: Vec<Diagnostic>,
        sources: SourceMap,
    }

    fn run_with(src: &str, config: BuildConfig, provider: MemorySourceProvider) -> Run {
        let mut sources = SourceMap::new();
        let file = sources.add(SourceFile::new("main.pas", src.as_bytes().to_vec()));
        let mut sink = DiagnosticSink::new();
        let lexed = lexer::lex(file, sources.get(file), &mut sink, false);
        let mut provider = provider;
        let out = {
            let mut cx = ProcessContext {
                config: &config,
                provider: &mut provider,
                sources: &mut sources,
                sink: &mut sink,
                include_positions: Vec::new(),
            };
            process(file, lexed.tokens, &mut cx)
        };
        Run {
            out,
            diags: sink.into_sorted(),
            sources,
        }
    }

    fn run(src: &str) -> Run {
        run_with(src, BuildConfig::default(), MemorySourceProvider::new())
    }

    fn kinds(r: &Run) -> Vec<crate::syntax::SyntaxKind> {
        r.out.tokens.iter().map(|t| t.kind).collect()
    }

    fn codes(r: &Run) -> Vec<String> {
        r.diags
            .iter()
            .map(|d| d.code.unwrap().to_string())
            .collect()
    }

    #[test]
    fn ifdef_inactive_merges() {
        let r = run("{$IFDEF X}a b{$ELSE}c{$ENDIF}d");
        assert_eq!(
            kinds(&r),
            [DIRECTIVE, INACTIVE, DIRECTIVE, IDENT, DIRECTIVE, IDENT, EOF]
        );
        assert!(r.diags.is_empty());
        let inactive = r.out.tokens[1];
        assert_eq!(inactive.range, TextRange::new(10.into(), 13.into()));
        assert_eq!(r.out.blocks.len(), 1);
        assert_eq!(r.out.blocks[0], TextRange::new(0.into(), 29.into()));
        assert_eq!(
            r.out.placement_at(10.into()),
            Some(Placement::Conditional {
                block_start: 0.into()
            })
        );
        assert_eq!(
            r.out.placement_at(13.into()),
            Some(Placement::Conditional {
                block_start: 0.into()
            })
        );
        assert_eq!(
            r.out.placement_at(21.into()),
            Some(Placement::Conditional {
                block_start: 0.into()
            })
        );
        assert_eq!(r.out.placement_at(20.into()), None);
    }

    #[test]
    fn define_and_nesting() {
        let r = run(
            "{$DEFINE X}{$IFDEF X}{$IFDEF Y}a{$ELSE}b{$ENDIF}{$ELSE}{$IFDEF Z}z{$ENDIF}c{$ENDIF}",
        );
        assert_eq!(
            kinds(&r),
            [
                DIRECTIVE, DIRECTIVE, DIRECTIVE, INACTIVE, DIRECTIVE, IDENT, DIRECTIVE, DIRECTIVE,
                INACTIVE, DIRECTIVE, EOF
            ]
        );
        assert!(r.diags.is_empty());
        let r = run("{$DEFINE X}{$UNDEF X}{$IFNDEF X}ok{$ENDIF}");
        assert_eq!(
            kinds(&r),
            [DIRECTIVE, DIRECTIVE, DIRECTIVE, IDENT, DIRECTIVE, EOF]
        );
        assert_eq!(codes(&run("{$DEFINE DEBUG}")), ["E0213"]);
        assert_eq!(
            run("{$DEFINE DEBUG}").diags[0].message,
            "`DEBUG` is defined by the build type and cannot be changed"
        );
        assert_eq!(
            run("{$UNDEF PUY}").diags[0].message,
            "`PUY` is always defined and cannot be changed"
        );
        assert_eq!(codes(&run("{$DEFINE 1X}")), ["E0213"]);
        assert_eq!(
            run("{$DEFINE 1X}").diags[0].message,
            "invalid argument for `{$DEFINE}`"
        );
    }

    #[test]
    fn predefined_symbols() {
        let r = run("{$IFDEF PUY}a{$ENDIF}{$IFDEF DEBUG}b{$ENDIF}{$IFDEF RELEASE}c{$ENDIF}");
        assert_eq!(
            kinds(&r),
            [
                DIRECTIVE, IDENT, DIRECTIVE, DIRECTIVE, IDENT, DIRECTIVE, DIRECTIVE, INACTIVE,
                DIRECTIVE, EOF
            ]
        );
        let cfg = BuildConfig {
            release: true,
            defines: vec!["foo".into()],
            ..Default::default()
        };
        let r = run_with(
            "{$IFDEF RELEASE}a{$ENDIF}{$IFDEF FOO}b{$ENDIF}",
            cfg,
            MemorySourceProvider::new(),
        );
        assert_eq!(
            kinds(&r),
            [
                DIRECTIVE, IDENT, DIRECTIVE, DIRECTIVE, IDENT, DIRECTIVE, EOF
            ]
        );
    }

    #[test]
    fn unmatched_and_unterminated() {
        let r = run("{$ENDIF}x");
        assert_eq!(codes(&r), ["E0202"]);
        assert_eq!(r.diags[0].message, "`{$ENDIF}` without matching `{$IF}`");
        assert_eq!(kinds(&r), [DIRECTIVE, IDENT, EOF]);
        let r = run("{$IFDEF X}a\n");
        assert_eq!(codes(&r), ["E0203"]);
        assert_eq!(
            r.diags[0].primary.as_ref().unwrap().span.range,
            TextRange::new(12.into(), 12.into())
        );
        assert_eq!(r.diags[0].secondary[0].label, "starts here");
        assert_eq!(r.out.suppress_after, Some(TextSize::from(10)));
        assert_eq!(kinds(&r), [DIRECTIVE, INACTIVE, EOF]);
        let r = run("{$IFDEF PUY}a\n");
        assert_eq!(codes(&r), ["E0203"]);
        assert_eq!(r.out.suppress_after, None);
        let r = run("{$IFDEF PUY}{$ELSE}{$ELSE}x{$ENDIF}");
        assert_eq!(codes(&r), ["E0204"]);
        assert_eq!(
            kinds(&r),
            [DIRECTIVE, DIRECTIVE, DIRECTIVE, INACTIVE, DIRECTIVE, EOF]
        );
    }

    #[test]
    fn if_expressions() {
        let r = run("{$IF Defined(PUY) and (PuyVersion >= 0)}a{$ELSEIF True}b{$ELSE}c{$ENDIF}");
        assert_eq!(
            kinds(&r),
            [
                DIRECTIVE, IDENT, DIRECTIVE, INACTIVE, DIRECTIVE, INACTIVE, DIRECTIVE, EOF
            ]
        );
        assert!(r.diags.is_empty());
        let r = run("{$IF Foo}a{$ENDIF}");
        assert_eq!(codes(&r), ["E0206"]);
        assert_eq!(
            r.diags[0].fixes[0].edits[0].range,
            TextRange::new(5.into(), 8.into())
        );
        assert_eq!(r.diags[0].fixes[0].edits[0].replacement, "Defined(Foo)");
        let r = run("{$IF CompilerVersion >= 23}a{$ENDIF}");
        assert_eq!(codes(&r), ["E0208"]);
        assert_eq!(r.diags[0].fixes[0].edits[0].replacement, "{$IFDEF PUY}");
        assert_eq!(
            r.diags[0].fixes[0].edits[0].range,
            TextRange::new(0.into(), 27.into())
        );
        let r = run("{$IF 1 + True}a{$ENDIF}");
        assert_eq!(codes(&r), ["E0215"]);
        let r = run("{$IF Declared(X)}a{$ENDIF}");
        assert_eq!(codes(&r), ["E0207"]);
        let r = run("{$IF (}a{$ENDIF}");
        assert_eq!(codes(&r), ["E0205"]);
        // Not evaluated inside an inactive range
        let r = run("{$IFDEF X}{$IF Foo}a{$ENDIF}{$ENDIF}");
        assert!(r.diags.is_empty());
    }

    #[test]
    fn platform_symbols() {
        let r = run(
            "{$IFDEF MSWINDOWS}a{$ENDIF}{$IF Defined(LINUX)}b{$ENDIF}{$IFDEF VER350}c{$ENDIF}{$IFDEF FPC}d{$ENDIF}",
        );
        assert_eq!(codes(&r), ["W0007", "W0007", "W0007"]);
        assert_eq!(
            r.diags[0].message,
            "conditional symbol `MSWINDOWS` is never defined in Puy"
        );
        assert_eq!(
            r.diags[2].notes[1],
            "1 more use of a compiler symbol in this file is not reported"
        );
        assert_eq!(
            kinds(&r),
            [
                DIRECTIVE, INACTIVE, DIRECTIVE, DIRECTIVE, INACTIVE, DIRECTIVE, DIRECTIVE,
                INACTIVE, DIRECTIVE, DIRECTIVE, INACTIVE, DIRECTIVE, EOF
            ]
        );
        // No warning if the user defines it
        let r = run("{$DEFINE MSWINDOWS}{$IFDEF MSWINDOWS}a{$ENDIF}");
        assert!(r.diags.is_empty());
        assert_eq!(kinds(&r), [DIRECTIVE, DIRECTIVE, IDENT, DIRECTIVE, EOF]);
    }

    #[test]
    fn switches() {
        let r = run("{$R-}\nvar N := 300;\n{$R+}\nB := N;\n{$Q-,C-}x{$RANGECHECKS OFF}");
        assert!(r.diags.is_empty(), "{:?}", r.diags);
        let t = &r.out.switches;
        assert!(t.at(0.into()).range_checks);
        assert!(!t.at(6.into()).range_checks);
        assert!(t.at(26.into()).range_checks);
        assert!(!t.at(42.into()).overflow_checks);
        assert!(!t.at(42.into()).assertions);
        assert!(!t.at(100.into()).range_checks);
        assert_eq!(r.out.placement_at(0.into()), Some(Placement::Switch));
        let r = run("{$IFOPT R+}{$DEFINE R_ON}{$R-}{$ENDIF}{$IFDEF R_ON}x{$ENDIF}");
        assert_eq!(
            kinds(&r),
            [
                DIRECTIVE, DIRECTIVE, DIRECTIVE, DIRECTIVE, DIRECTIVE, IDENT, DIRECTIVE, EOF
            ]
        );
        assert_eq!(codes(&run("{$IFOPT B+}{$ENDIF}")), ["E0214"]);
        assert_eq!(codes(&run("{$Q yes}")), ["E0213"]);
        let r = run(
            "{$WARN unused-variable OFF}{$WARN SYMBOL_PLATFORM ERROR}{$WARN Unused_Variable OFF}{$WARN foo bar}",
        );
        assert_eq!(codes(&r), ["W0014", "E0213"]);
        assert_eq!(r.diags[0].fixes[0].edits[0].replacement, "unused-variable");
        assert_eq!(
            r.out.switches.at(100.into()).warn_state(Code::W(1)),
            WarnState::Off
        );
        assert_eq!(
            r.out.switches.at(100.into()).warn_state(Code::W(10)),
            WarnState::Error
        );
        assert_eq!(
            r.out.switches.at(100.into()).warn_state(Code::W(3)),
            WarnState::On
        );
    }

    #[test]
    fn ignored_and_errors() {
        let r = run(
            "{$APPTYPE CONSOLE}\n{$J+}\n{$MODE objfpc}\n{$PUSH}\n{$MODE DELPHI}{$REGION 'x'}{$H+}{$M 16384,1048576}{$M+}",
        );
        assert_eq!(
            codes(&r),
            ["W0004", "W0004", "W0004", "W0004", "W0004", "W0004"]
        );
        assert_eq!(
            r.diags[0].message,
            "directive `{$APPTYPE CONSOLE}` is ignored"
        );
        assert_eq!(r.diags[0].fixes[0].title, "remove `{$APPTYPE CONSOLE}`");
        assert_eq!(
            r.diags[0].fixes[0].edits[0].range,
            TextRange::new(0.into(), 19.into())
        );
        assert_eq!(
            r.diags[2].message,
            "FPC-only directive `{$MODE objfpc}` is ignored"
        );
        assert_eq!(r.diags[3].notes.len(), 2);
        assert_eq!(
            r.diags[4].notes[0],
            "Puy compiles to bytecode for a VM, where this directive has no meaning"
        );
        assert_eq!(
            r.diags[5].notes[0],
            "Puy keeps run-time type information for every type"
        );
        assert_eq!(
            codes(&run(
                "{$H-}{$ZEROBASEDSTRINGS ON}{$I-}{$POINTERS ON}{$POINTERMATH OFF}{$IOCHECKS ON}"
            )),
            ["E0201", "E0201", "E0201", "E0212"]
        );
        let r = run("{$POINTER ON}");
        assert_eq!(codes(&r), ["E0200"]);
        assert_eq!(
            r.diags[0].message,
            "unknown compiler directive `{$POINTER ON}`"
        );
        assert_eq!(r.diags[0].fixes[0].edits[0].replacement, "POINTERS");
        assert_eq!(
            r.diags[0].fixes[0].edits[0].range,
            TextRange::new(2.into(), 9.into())
        );
        let r = run("{$100 yen}");
        assert_eq!(codes(&r), ["E0200"]);
        assert_eq!(r.diags[0].message, "`{$` starts a compiler directive");
        assert_eq!(codes(&run("{$IFC}")), ["E0200"]);
        assert_eq!(
            codes(&run("{$ENDIF DEBUG}{$IFDEF X}{$ELSE // y}{$ENDIF}")),
            ["E0202"]
        );
    }

    #[test]
    fn messages() {
        let r = run(
            "{$MESSAGE 'hi'}{$MESSAGE WARN 'w'}{$MESSAGE ERROR 'e'}{$MESSAGE Fatal 'f'}{$WARNING fpc w}{$NOTE 'n'}{$MESSAGE x}",
        );
        assert_eq!(
            codes(&r),
            [
                "W0008", "W0008", "E0211", "E0211", "W0008", "W0008", "E0213"
            ]
        );
        assert_eq!(r.diags[0].severity, Severity::Note);
        assert_eq!(r.diags[0].message, "user hint: hi");
        assert_eq!(r.diags[1].message, "user warning: w");
        assert_eq!(r.diags[2].message, "user error: e");
        assert_eq!(r.diags[4].message, "user warning: fpc w");
        assert_eq!(r.diags[5].message, "user hint: n");
        // Not evaluated in an inactive range
        assert!(
            run("{$IFDEF X}{$MESSAGE ERROR 'e'}{$ENDIF}")
                .diags
                .is_empty()
        );
    }

    #[test]
    fn includes() {
        let p = MemorySourceProvider::new()
            .with_file("defs.inc", "{$DEFINE FROM_INC}{$R-}")
            .with_file("sub/x.inc", "{$I ../defs.inc}")
            .with_file("sub/../defs.inc", "{$DEFINE FROM_INC}")
            .with_file("bad.inc", "var X: Integer;")
            .with_file("self.inc", "{$I self.inc}")
            .with_file("big.inc", b"\xFF".to_vec());
        let r = run_with(
            "{$I defs.inc}{$IFDEF FROM_INC}a{$ENDIF}",
            BuildConfig::default(),
            p.clone(),
        );
        assert!(r.diags.is_empty(), "{:?}", r.diags);
        assert_eq!(kinds(&r), [DIRECTIVE, DIRECTIVE, IDENT, DIRECTIVE, EOF]);
        assert!(!r.out.switches.at(13.into()).range_checks);
        assert!(r.out.switches.at(12.into()).range_checks);
        assert_eq!(r.out.placement_at(0.into()), Some(Placement::Switch));
        assert_eq!(r.sources.files().len(), 2);
        let r = run_with(
            "{$I 'sub\\x.inc'}{$IFDEF FROM_INC}a{$ENDIF}",
            BuildConfig::default(),
            p.clone(),
        );
        assert!(r.diags.is_empty(), "{:?}", r.diags);
        assert_eq!(r.sources.files()[1].path, "sub/x.inc");
        assert_eq!(r.sources.files()[2].path, "sub/../defs.inc");
        let r = run_with("{$I bad.inc}", BuildConfig::default(), p.clone());
        assert_eq!(codes(&r), ["E0210"]);
        assert_eq!(r.diags[0].primary.as_ref().unwrap().span.file, FileId(1));
        assert_eq!(r.diags[0].secondary[0].label, "included from here");
        let r = run_with("{$I missing.inc}", BuildConfig::default(), p.clone());
        assert_eq!(codes(&r), ["E0209"]);
        assert_eq!(r.diags[0].message, "include file `missing.inc` not found");
        let r = run_with("{$I self.inc}", BuildConfig::default(), p.clone());
        assert_eq!(codes(&r), ["E0216"]);
        assert_eq!(
            r.diags[0].message,
            "include file `self.inc` includes itself"
        );
        assert_eq!(
            r.diags[0].notes,
            ["include chain: main.pas -> self.inc -> self.inc"]
        );
        let r = run_with("{$I big.inc}x", BuildConfig::default(), p.clone());
        assert_eq!(codes(&r), ["E0001"]);
        assert_eq!(kinds(&r), [DIRECTIVE, IDENT, EOF]);
        assert_eq!(
            codes(&run("{$I}{$I ''}{$I %DATE%}")),
            ["E0213", "E0213", "E0213"]
        );
        // {$I} in an inactive range is not read
        let r = run_with(
            "{$IFDEF X}{$I missing.inc}{$ENDIF}",
            BuildConfig::default(),
            p,
        );
        assert!(r.diags.is_empty());
    }

    #[test]
    fn include_depth() {
        // Up to depth 16 can be read. The 17th-level `{$I}` gives E0216 (`directives.md` chapter 5)
        let mut p = MemorySourceProvider::new();
        for i in 1..=MAX_INCLUDE_DEPTH {
            p = p.with_file(format!("d{i}.inc"), format!("{{$I d{}.inc}}", i + 1));
        }
        let last = MAX_INCLUDE_DEPTH + 1;
        p = p.with_file(format!("d{last}.inc"), "{$DEFINE DEEP}");
        let r = run_with(
            "{$I d2.inc}{$IFDEF DEEP}a{$ENDIF}",
            BuildConfig::default(),
            p.clone(),
        );
        assert!(r.diags.is_empty(), "{:?}", r.diags);
        assert_eq!(kinds(&r), [DIRECTIVE, DIRECTIVE, IDENT, DIRECTIVE, EOF]);
        let r = run_with(
            "{$I d1.inc}{$IFDEF DEEP}a{$ENDIF}",
            BuildConfig::default(),
            p,
        );
        assert_eq!(codes(&r), ["E0216"]);
        assert_eq!(r.diags[0].message, "include files nested too deeply");
        assert_eq!(
            r.diags[0].primary.as_ref().unwrap().span.file,
            FileId(MAX_INCLUDE_DEPTH as u32)
        );
        assert!(r.diags[0].notes[0].starts_with("include chain: main.pas -> d1.inc -> d2.inc ->"));
        assert!(r.diags[0].notes[0].ends_with("-> d16.inc -> d17.inc"));
        // The inclusion was refused, so `DEEP` is not defined
        assert_eq!(kinds(&r), [DIRECTIVE, DIRECTIVE, INACTIVE, DIRECTIVE, EOF]);
    }
    #[test]
    fn include_path_rules() {
        assert_eq!(
            resolve_include_path("src/main.pas", "incl.inc"),
            "src/incl.inc"
        );
        assert_eq!(
            resolve_include_path("src\\main.pas", "incl.inc"),
            "src\\incl.inc"
        );
        assert_eq!(resolve_include_path("main.pas", "incl.inc"), "incl.inc");
        assert_eq!(
            resolve_include_path("src/main.pas", "..\\inc\\defs.inc"),
            "src/../inc/defs.inc"
        );
        assert_eq!(
            resolve_include_path("src/main.pas", "/abs/x.inc"),
            "/abs/x.inc"
        );
        assert_eq!(resolve_include_path("<stdin>", "x.inc"), "x.inc");
    }
}
