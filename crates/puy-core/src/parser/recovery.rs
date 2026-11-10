//! Balanced skipping (`docs/spec/grammar.md` chapter 3).

use super::{Parser, is_sync};
use crate::syntax::SyntaxKind::{self, *};

fn opens_block(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        BEGIN_KW
            | TRY_KW
            | RECORD_KW
            | OBJECT_KW
            | CLASS_KW
            | INTERFACE_KW
            | DISPINTERFACE_KW
            | ASM_KW
    )
}

/// Whether `cur` opens a block given the previous significant token: `interface` only counts as
/// an interface type right after `=` (M3); elsewhere it starts the unit's section
/// (`docs/spec/units.md` chapter 1).
fn opens_block_after(cur: SyntaxKind, prev: SyntaxKind) -> bool {
    if cur == INTERFACE_KW {
        prev == EQ
    } else {
        opens_block(cur)
    }
}

/// `class` / `interface` / `dispinterface` / `object` are not counted when they form a forward declaration, a class reference type, or a declaration with only a parent.
fn is_forward_class(p: &Parser<'_>) -> bool {
    match p.nth(1) {
        SEMICOLON | OF_KW => true,
        L_PAREN => {
            let mut i = 2;
            loop {
                match p.nth(i) {
                    R_PAREN => return p.nth(i + 1) == SEMICOLON,
                    EOF | SEMICOLON | END_KW => return false,
                    _ => i += 1,
                }
                if i > 64 {
                    return false;
                }
            }
        }
        _ => false,
    }
}

/// Statement skipping: from the opening reserved word up to the `end` that brings the depth back to 0
/// (for forms without `end`, up to just before the first synchronizing token at depth 0).
pub(super) fn skip_statement(p: &mut Parser<'_>) {
    skip_statement_with(p, is_sync);
}

/// When skipping at the nesting limit (E0413): do not stop at reserved words that can start a statement (`if`, etc.);
/// only `;` / `end` / `until` / `else` / `.` / `EOF` are synchronizing tokens (so that a chain of `if`s
/// following `then` becomes a single `ERROR` node; `grammar.md` chapter 3).
pub(super) fn skip_nested_statement(p: &mut Parser<'_>) {
    skip_statement_with(p, |k| {
        matches!(k, SEMICOLON | END_KW | UNTIL_KW | ELSE_KW | DOT | EOF)
    });
}

fn skip_statement_with(p: &mut Parser<'_>, is_sync: fn(SyntaxKind) -> bool) {
    p.start_node(ERROR_NODE);
    let mut depth: i32 = 0;
    let first = p.current();
    if matches!(first, TRY_KW | CASE_KW | ASM_KW | BEGIN_KW) {
        depth += 1;
    }
    p.bump();
    let mut prev = first;
    while !p.at_eof() {
        let cur = p.current();
        if cur == END_KW {
            depth -= 1;
            p.bump();
            if depth <= 0 {
                break;
            }
            prev = cur;
            continue;
        }
        let opens = cur == CASE_KW || opens_block_after(cur, prev);
        prev = cur;
        if opens {
            // `begin` in the middle of skipping is not a synchronizing token; it adds +1
            if matches!(cur, CLASS_KW | INTERFACE_KW | DISPINTERFACE_KW | OBJECT_KW)
                && is_forward_class(p)
            {
                p.bump();
                continue;
            }
            depth += 1;
            p.bump();
            continue;
        }
        if cur == DOT && p.nth(1) == IDENT {
            // The `.` in `Exception.Create` is a qualified name (not a synchronizing token)
            p.bump();
            continue;
        }
        if depth == 0 && is_sync(cur) {
            break;
        }
        p.bump();
    }
    p.finish_node();
}

/// Section skipping: from the section's reserved word up to just before the next section or body starts at depth 0.
pub(super) fn skip_section(p: &mut Parser<'_>) {
    p.start_node(ERROR_NODE);
    p.bump(); // the section's reserved word
    let mut depth: i32 = 0;
    let mut prev_semicolon = false;
    let mut prev = SEMICOLON;
    while !p.at_eof() {
        let cur = p.current();
        if depth == 0 {
            let section_start = matches!(
                cur,
                CONST_KW
                    | VAR_KW
                    | TYPE_KW
                    | LABEL_KW
                    | RESOURCESTRING_KW
                    | THREADVAR_KW
                    | EXPORTS_KW
                    | USES_KW
                    | BEGIN_KW
                    | INITIALIZATION_KW
                    | FINALIZATION_KW
                    | IMPLEMENTATION_KW
            ) || (cur == INTERFACE_KW && prev != EQ)
                || (matches!(cur, PROCEDURE_KW | FUNCTION_KW) && prev_semicolon);
            if section_start {
                break;
            }
        }
        if cur == END_KW {
            depth -= 1;
            if depth < 0 {
                // Unbalanced `end` inside the section: leave it to the outer level
                break;
            }
        } else if opens_block_after(cur, prev) {
            if matches!(cur, CLASS_KW | INTERFACE_KW | DISPINTERFACE_KW | OBJECT_KW)
                && is_forward_class(p)
            {
                prev_semicolon = false;
                prev = cur;
                p.bump();
                continue;
            }
            depth += 1;
        }
        prev_semicolon = cur == SEMICOLON;
        prev = cur;
        p.bump();
    }
    p.finish_node();
}

/// The body of a `class` / `interface` / `dispinterface` / `object` type in a `type` declaration
/// (M3 or removed): from the reserved word up to just before the `;` that ends the declaration at
/// depth 0, keeping `end` balanced. A forward declaration (`TFoo = class;`) is only the word.
pub(super) fn skip_type_body(p: &mut Parser<'_>) {
    p.start_node(ERROR_NODE);
    let mut depth: i32 = 0;
    let mut first = true;
    while !p.at_eof() {
        let cur = p.current();
        if cur == END_KW {
            depth -= 1;
            p.bump();
            if depth <= 0 {
                break;
            }
            continue;
        }
        if opens_block(cur) || (cur == CASE_KW && depth > 0) {
            if matches!(cur, CLASS_KW | INTERFACE_KW | DISPINTERFACE_KW | OBJECT_KW)
                && is_forward_class(p)
            {
                p.bump();
                first = false;
                continue;
            }
            depth += 1;
            p.bump();
            first = false;
            continue;
        }
        if depth == 0 && !first && (cur == SEMICOLON || is_sync(cur)) {
            break;
        }
        first = false;
        p.bump();
    }
    p.finish_node();
}

/// Expression skipping (on E0413): up to just before a synchronizing token.
pub(super) fn skip_expr(p: &mut Parser<'_>, extra_sync: &[SyntaxKind]) {
    p.start_node(ERROR_NODE);
    let mut consumed = false;
    let mut parens: i32 = 0;
    while !p.at_eof() {
        let cur = p.current();
        if parens <= 0
            && (is_sync(cur)
                || extra_sync.contains(&cur)
                || matches!(cur, R_PAREN | R_BRACK | COMMA))
        {
            break;
        }
        match cur {
            L_PAREN | L_BRACK => parens += 1,
            R_PAREN | R_BRACK => parens -= 1,
            _ => {}
        }
        p.bump();
        consumed = true;
    }
    if !consumed {
        // Progress guarantee: advance at least one token
        if !p.at_eof() && !matches!(p.current(), SEMICOLON | END_KW | R_PAREN | R_BRACK) {
            p.bump();
        }
    }
    p.finish_node();
}
