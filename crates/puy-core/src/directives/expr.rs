//! Evaluator for `{$IF}` / `{$ELSEIF}` expressions (`docs/spec/directives.md` 3.2).
//!
//! Semantic analysis is not available. Values are of three types: integer (`Int64`), boolean, and string.

use crate::lexer::int_literal_value;

/// The value of an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
}

/// An evaluation failure. Positions are byte ranges within the expression text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// E0205
    Syntax { start: usize, end: usize },
    /// E0206. `name` is as written. If `bool_position`, a `Defined(…)` fix can be attached
    UnknownName {
        start: usize,
        end: usize,
        name: String,
        bool_position: bool,
    },
    /// E0207
    Declared { start: usize, end: usize },
    /// E0208. `fix` replaces the whole directive (with the form `IFDEF PUY`, when the directive name is `IF`)
    CompilerVersion {
        start: usize,
        end: usize,
        name: String,
        /// When the direction of the comparison is determined: `true` is `{$IFDEF PUY}`, `false` is `{$IFNDEF PUY}`
        fix: Option<bool>,
    },
    /// E0215 type mismatch
    TypeMismatch { start: usize, end: usize },
    /// E0215 division by zero
    DivByZero { start: usize, end: usize },
}

impl EvalError {
    pub fn range(&self) -> (usize, usize) {
        match *self {
            EvalError::Syntax { start, end }
            | EvalError::UnknownName { start, end, .. }
            | EvalError::Declared { start, end }
            | EvalError::CompilerVersion { start, end, .. }
            | EvalError::TypeMismatch { start, end }
            | EvalError::DivByZero { start, end } => (start, end),
        }
    }
}

/// Evaluation context.
pub trait Context {
    fn is_defined(&self, name: &str) -> bool;
    /// Reports, for a warning, when a name is an OS, CPU, or compiler symbol that is always treated as false.
    fn note_symbol_use(&mut self, name: &str, start: usize, end: usize);
    fn version(&self) -> (i64, &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Int(u64),
    Str(String),
    Name(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
}

#[derive(Debug, Clone)]
struct Spanned {
    tok: Tok,
    start: usize,
    end: usize,
}

fn tokenize(text: &str) -> Result<Vec<Spanned>, EvalError> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start = i;
        match c {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C => {
                i += 1;
            }
            b'(' => {
                out.push(Spanned {
                    tok: Tok::LParen,
                    start,
                    end: i + 1,
                });
                i += 1;
            }
            b')' => {
                out.push(Spanned {
                    tok: Tok::RParen,
                    start,
                    end: i + 1,
                });
                i += 1;
            }
            b',' => {
                out.push(Spanned {
                    tok: Tok::Comma,
                    start,
                    end: i + 1,
                });
                i += 1;
            }
            b'+' | b'-' | b'*' | b'=' => {
                let op: &'static str = match c {
                    b'+' => "+",
                    b'-' => "-",
                    b'*' => "*",
                    _ => "=",
                };
                out.push(Spanned {
                    tok: Tok::Op(op),
                    start,
                    end: i + 1,
                });
                i += 1;
            }
            b'<' => {
                let (op, len): (&'static str, usize) = match b.get(i + 1) {
                    Some(b'>') => ("<>", 2),
                    Some(b'=') => ("<=", 2),
                    _ => ("<", 1),
                };
                out.push(Spanned {
                    tok: Tok::Op(op),
                    start,
                    end: i + len,
                });
                i += len;
            }
            b'>' => {
                let (op, len): (&'static str, usize) = match b.get(i + 1) {
                    Some(b'=') => (">=", 2),
                    _ => (">", 1),
                };
                out.push(Spanned {
                    tok: Tok::Op(op),
                    start,
                    end: i + len,
                });
                i += len;
            }
            b'\'' => {
                let mut j = i + 1;
                let mut s = String::new();
                loop {
                    match b.get(j) {
                        None | Some(b'\n') | Some(b'\r') => {
                            return Err(EvalError::Syntax { start, end: j });
                        }
                        Some(b'\'') => {
                            if b.get(j + 1) == Some(&b'\'') {
                                s.push('\'');
                                j += 2;
                                continue;
                            }
                            j += 1;
                            break;
                        }
                        Some(_) => {
                            // Copy the UTF-8 character as is
                            let ch_len = utf8_len(b[j]);
                            s.push_str(&text[j..j + ch_len]);
                            j += ch_len;
                        }
                    }
                }
                // Concatenated elements (`#` or `'''`) are not allowed
                if matches!(b.get(j), Some(b'\'') | Some(b'#')) {
                    return Err(EvalError::Syntax { start, end: j + 1 });
                }
                out.push(Spanned {
                    tok: Tok::Str(s),
                    start,
                    end: j,
                });
                i = j;
            }
            b'#' => return Err(EvalError::Syntax { start, end: i + 1 }),
            b'0'..=b'9' | b'$' | b'%' => {
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                let lit = &b[i..j];
                let valid_shape = match lit[0] {
                    b'$' => {
                        lit.len() > 1
                            && lit[1..].iter().all(|c| c.is_ascii_hexdigit() || *c == b'_')
                    }
                    b'%' => {
                        lit.len() > 1 && lit[1..].iter().all(|c| matches!(c, b'0' | b'1' | b'_'))
                    }
                    _ => lit.iter().all(|c| c.is_ascii_digit() || *c == b'_'),
                };
                let underscore_ok = !lit.ends_with(b"_")
                    && !lit.windows(2).any(|w| w == b"__")
                    && !(matches!(lit[0], b'$' | b'%') && lit.get(1) == Some(&b'_'));
                if !valid_shape || !underscore_ok {
                    return Err(EvalError::Syntax { start, end: j });
                }
                let Some(v) = int_literal_value(lit) else {
                    return Err(EvalError::Syntax { start, end: j });
                };
                out.push(Spanned {
                    tok: Tok::Int(v),
                    start,
                    end: j,
                });
                i = j;
            }
            _ if c.is_ascii_alphabetic() || c == b'_' => {
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                let word = &text[i..j];
                let lower = word.to_ascii_lowercase();
                let tok = match lower.as_str() {
                    "or" => Tok::Op("or"),
                    "xor" => Tok::Op("xor"),
                    "and" => Tok::Op("and"),
                    "div" => Tok::Op("div"),
                    "mod" => Tok::Op("mod"),
                    "not" => Tok::Op("not"),
                    _ => Tok::Name(word.to_string()),
                };
                out.push(Spanned { tok, start, end: j });
                i = j;
            }
            _ => {
                let len = utf8_len(c);
                return Err(EvalError::Syntax {
                    start,
                    end: i + len,
                });
            }
        }
    }
    Ok(out)
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

/// An expression tree.
#[derive(Debug, Clone)]
enum Expr {
    Int(i64, usize, usize),
    Str(String, usize, usize),
    Bool(bool),
    /// Constant or symbol name (written without `Defined`)
    Name(String, usize, usize),
    Defined(String, usize, usize),
    Declared(usize, usize),
    Not(Box<Expr>, usize, usize),
    Neg(Box<Expr>, usize, usize),
    Binary(&'static str, Box<Expr>, Box<Expr>, usize, usize),
}

impl Expr {
    fn range(&self) -> (usize, usize) {
        match self {
            Expr::Int(_, s, e)
            | Expr::Str(_, s, e)
            | Expr::Name(_, s, e)
            | Expr::Defined(_, s, e)
            | Expr::Declared(s, e)
            | Expr::Not(_, s, e)
            | Expr::Neg(_, s, e)
            | Expr::Binary(_, _, _, s, e) => (*s, *e),
            Expr::Bool(_) => (0, 0),
        }
    }
}

const MAX_PAREN_DEPTH: usize = 256;

struct Parser {
    toks: Vec<Spanned>,
    pos: usize,
    depth: usize,
    text_len: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn cur_range(&self) -> (usize, usize) {
        match self.toks.get(self.pos) {
            Some(t) => (t.start, t.end),
            None => (self.text_len, self.text_len),
        }
    }

    fn syntax_err(&self) -> EvalError {
        let (s, e) = self.cur_range();
        EvalError::Syntax { start: s, end: e }
    }

    fn expr(&mut self) -> Result<Expr, EvalError> {
        let left = self.simple()?;
        if let Some(Tok::Op(op)) = self.peek()
            && matches!(*op, "=" | "<>" | "<" | ">" | "<=" | ">=")
        {
            let op = *op;
            self.pos += 1;
            let right = self.simple()?;
            let (s, _) = left.range();
            let (_, e) = right.range();
            return Ok(Expr::Binary(op, Box::new(left), Box::new(right), s, e));
        }
        Ok(left)
    }

    fn simple(&mut self) -> Result<Expr, EvalError> {
        let mut left = self.term()?;
        while let Some(Tok::Op(op)) = self.peek()
            && matches!(*op, "+" | "-" | "or" | "xor")
        {
            let op = *op;
            self.pos += 1;
            let right = self.term()?;
            let (s, _) = left.range();
            let (_, e) = right.range();
            left = Expr::Binary(op, Box::new(left), Box::new(right), s, e);
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Expr, EvalError> {
        let mut left = self.factor()?;
        while let Some(Tok::Op(op)) = self.peek()
            && matches!(*op, "*" | "div" | "mod" | "and")
        {
            let op = *op;
            self.pos += 1;
            let right = self.factor()?;
            let (s, _) = left.range();
            let (_, e) = right.range();
            left = Expr::Binary(op, Box::new(left), Box::new(right), s, e);
        }
        Ok(left)
    }

    fn factor(&mut self) -> Result<Expr, EvalError> {
        let Some(t) = self.toks.get(self.pos).cloned() else {
            return Err(self.syntax_err());
        };
        match t.tok {
            Tok::Op("not") => {
                self.pos += 1;
                let inner = self.factor()?;
                let (_, e) = inner.range();
                Ok(Expr::Not(Box::new(inner), t.start, e))
            }
            Tok::Op("-") => {
                self.pos += 1;
                let inner = self.factor()?;
                let (_, e) = inner.range();
                Ok(Expr::Neg(Box::new(inner), t.start, e))
            }
            Tok::LParen => {
                self.pos += 1;
                self.depth += 1;
                if self.depth > MAX_PAREN_DEPTH {
                    return Err(EvalError::Syntax {
                        start: t.start,
                        end: t.end,
                    });
                }
                let inner = self.expr()?;
                self.depth -= 1;
                match self.peek() {
                    Some(Tok::RParen) => {
                        self.pos += 1;
                        Ok(inner)
                    }
                    _ => Err(self.syntax_err()),
                }
            }
            Tok::Int(v) => {
                self.pos += 1;
                if v > i64::MAX as u64 {
                    return Err(EvalError::Syntax {
                        start: t.start,
                        end: t.end,
                    });
                }
                Ok(Expr::Int(v as i64, t.start, t.end))
            }
            Tok::Str(s) => {
                self.pos += 1;
                Ok(Expr::Str(s, t.start, t.end))
            }
            Tok::Name(name) => {
                self.pos += 1;
                let lower = name.to_ascii_lowercase();
                match lower.as_str() {
                    "true" => Ok(Expr::Bool(true)),
                    "false" => Ok(Expr::Bool(false)),
                    "defined" | "declared" => {
                        if self.peek() != Some(&Tok::LParen) {
                            return Err(self.syntax_err());
                        }
                        self.pos += 1;
                        let Some(arg) = self.toks.get(self.pos).cloned() else {
                            return Err(self.syntax_err());
                        };
                        let Tok::Name(arg_name) = arg.tok else {
                            return Err(self.syntax_err());
                        };
                        self.pos += 1;
                        if self.peek() != Some(&Tok::RParen) {
                            return Err(self.syntax_err());
                        }
                        self.pos += 1;
                        let end = self.toks[self.pos - 1].end;
                        if lower == "declared" {
                            Ok(Expr::Declared(t.start, end))
                        } else {
                            Ok(Expr::Defined(arg_name, arg.start, arg.end))
                        }
                    }
                    _ => {
                        if self.peek() == Some(&Tok::LParen) {
                            // Unknown function
                            return Err(EvalError::UnknownName {
                                start: t.start,
                                end: t.end,
                                name,
                                bool_position: false,
                            });
                        }
                        Ok(Expr::Name(name, t.start, t.end))
                    }
                }
            }
            _ => Err(self.syntax_err()),
        }
    }
}

/// Evaluates an expression. Returns a boolean (E0215 if the type is not boolean).
pub fn evaluate(text: &str, ctx: &mut dyn Context) -> Result<bool, EvalError> {
    let toks = tokenize(text)?;
    let mut p = Parser {
        toks,
        pos: 0,
        depth: 0,
        text_len: text.len(),
    };
    let expr = p.expr()?;
    if p.pos != p.toks.len() {
        return Err(p.syntax_err());
    }
    // Whether the whole expression has the form `CompilerVersion <comparison> <integer>` (for the E0208 fix)
    if let Expr::Binary(op, l, r, _, _) = &expr {
        let cv = |e: &Expr| match e {
            Expr::Name(n, s, en) if is_compiler_version(n) => Some((n.clone(), *s, *en)),
            _ => None,
        };
        let int = |e: &Expr| matches!(e, Expr::Int(..));
        let flipped_fix = |op: &str, cv_on_left: bool| -> Option<bool> {
            let effective = if cv_on_left {
                op.to_string()
            } else {
                match op {
                    "<" => ">",
                    ">" => "<",
                    "<=" => ">=",
                    ">=" => "<=",
                    o => o,
                }
                .to_string()
            };
            match effective.as_str() {
                ">=" | ">" => Some(true),
                "<" | "<=" | "<>" => Some(false),
                _ => None,
            }
        };
        if let Some((name, s, e)) = cv(l)
            && int(r)
        {
            return Err(EvalError::CompilerVersion {
                start: s,
                end: e,
                name,
                fix: flipped_fix(op, true),
            });
        }
        if let Some((name, s, e)) = cv(r)
            && int(l)
        {
            return Err(EvalError::CompilerVersion {
                start: s,
                end: e,
                name,
                fix: flipped_fix(op, false),
            });
        }
    }
    match eval(&expr, ctx, true)? {
        Value::Bool(b) => Ok(b),
        _ => {
            let (s, e) = expr.range();
            Err(EvalError::TypeMismatch { start: s, end: e })
        }
    }
}

fn is_compiler_version(name: &str) -> bool {
    name.eq_ignore_ascii_case("CompilerVersion") || name.eq_ignore_ascii_case("RTLVersion")
}

fn eval(expr: &Expr, ctx: &mut dyn Context, bool_position: bool) -> Result<Value, EvalError> {
    Ok(match expr {
        Expr::Int(v, _, _) => Value::Int(*v),
        Expr::Str(s, _, _) => Value::Str(s.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Defined(name, s, e) => {
            ctx.note_symbol_use(name, *s, *e);
            Value::Bool(ctx.is_defined(name))
        }
        Expr::Declared(s, e) => return Err(EvalError::Declared { start: *s, end: *e }),
        Expr::Name(name, s, e) => {
            let lower = name.to_ascii_lowercase();
            match lower.as_str() {
                "puyversion" => Value::Int(ctx.version().0),
                "puyversionstring" => Value::Str(ctx.version().1.to_string()),
                _ if is_compiler_version(name) => {
                    return Err(EvalError::CompilerVersion {
                        start: *s,
                        end: *e,
                        name: name.clone(),
                        fix: None,
                    });
                }
                _ => {
                    return Err(EvalError::UnknownName {
                        start: *s,
                        end: *e,
                        name: name.clone(),
                        bool_position,
                    });
                }
            }
        }
        Expr::Not(inner, s, e) => match eval(inner, ctx, true)? {
            Value::Bool(b) => Value::Bool(!b),
            Value::Int(i) => Value::Int(!i),
            _ => return Err(EvalError::TypeMismatch { start: *s, end: *e }),
        },
        Expr::Neg(inner, s, e) => match eval(inner, ctx, false)? {
            Value::Int(i) => Value::Int(i.wrapping_neg()),
            _ => return Err(EvalError::TypeMismatch { start: *s, end: *e }),
        },
        Expr::Binary(op, l, r, s, e) => {
            let logical = matches!(*op, "and" | "or" | "xor");
            let lv = eval(l, ctx, logical)?;
            let rv = eval(r, ctx, logical)?;
            let mismatch = || EvalError::TypeMismatch { start: *s, end: *e };
            match (*op, lv, rv) {
                ("and", Value::Bool(a), Value::Bool(b)) => Value::Bool(a && b),
                ("or", Value::Bool(a), Value::Bool(b)) => Value::Bool(a || b),
                ("xor", Value::Bool(a), Value::Bool(b)) => Value::Bool(a ^ b),
                ("and", Value::Int(a), Value::Int(b)) => Value::Int(a & b),
                ("or", Value::Int(a), Value::Int(b)) => Value::Int(a | b),
                ("xor", Value::Int(a), Value::Int(b)) => Value::Int(a ^ b),
                ("+", Value::Int(a), Value::Int(b)) => Value::Int(a.wrapping_add(b)),
                ("-", Value::Int(a), Value::Int(b)) => Value::Int(a.wrapping_sub(b)),
                ("*", Value::Int(a), Value::Int(b)) => Value::Int(a.wrapping_mul(b)),
                ("div", Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        return Err(EvalError::DivByZero { start: *s, end: *e });
                    }
                    Value::Int(a.wrapping_div(b))
                }
                ("mod", Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        return Err(EvalError::DivByZero { start: *s, end: *e });
                    }
                    Value::Int(a.wrapping_rem(b))
                }
                ("+", Value::Str(a), Value::Str(b)) => Value::Str(a + &b),
                (cmp, Value::Int(a), Value::Int(b)) => {
                    Value::Bool(compare(cmp, a.cmp(&b)).ok_or_else(mismatch)?)
                }
                (cmp, Value::Str(a), Value::Str(b)) => {
                    let ord = a.chars().cmp(b.chars());
                    Value::Bool(compare(cmp, ord).ok_or_else(mismatch)?)
                }
                (cmp, Value::Bool(a), Value::Bool(b)) => {
                    Value::Bool(compare(cmp, a.cmp(&b)).ok_or_else(mismatch)?)
                }
                _ => return Err(mismatch()),
            }
        }
    })
}

fn compare(op: &str, ord: std::cmp::Ordering) -> Option<bool> {
    use std::cmp::Ordering::*;
    Some(match op {
        "=" => ord == Equal,
        "<>" => ord != Equal,
        "<" => ord == Less,
        ">" => ord == Greater,
        "<=" => ord != Greater,
        ">=" => ord != Less,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ctx(Vec<&'static str>);

    impl Context for Ctx {
        fn is_defined(&self, name: &str) -> bool {
            self.0.iter().any(|s| s.eq_ignore_ascii_case(name))
        }
        fn note_symbol_use(&mut self, _: &str, _: usize, _: usize) {}
        fn version(&self) -> (i64, &str) {
            (10203, "1.2.3")
        }
    }

    fn ev(s: &str) -> Result<bool, EvalError> {
        evaluate(s, &mut Ctx(vec!["PUY", "DEBUG"]))
    }

    #[test]
    fn basics() {
        assert_eq!(ev("Defined(PUY)"), Ok(true));
        assert_eq!(ev("defined(puy) and not Defined(X)"), Ok(true));
        assert_eq!(ev("PuyVersion >= 10200"), Ok(true));
        assert_eq!(ev("PuyVersionString = '1.2.3'"), Ok(true));
        assert_eq!(ev("(1 + 2) * 3 = 9"), Ok(true));
        assert_eq!(ev("$FF = 255"), Ok(true));
        assert_eq!(ev("'a' < 'b'"), Ok(true));
        assert_eq!(ev("'A' < 'a'"), Ok(true));
        assert_eq!(ev("True"), Ok(true));
        assert_eq!(ev("not False"), Ok(true));
        assert_eq!(ev("7 div 2 = 3"), Ok(true));
        assert_eq!(ev("-7 mod 2 = -1"), Ok(true));
    }

    #[test]
    fn errors() {
        assert!(matches!(ev("1 +"), Err(EvalError::Syntax { .. })));
        assert!(matches!(ev("Defined(X"), Err(EvalError::Syntax { .. })));
        assert!(matches!(ev("1"), Err(EvalError::TypeMismatch { .. })));
        assert!(matches!(
            ev("1 + True"),
            Err(EvalError::TypeMismatch { .. })
        ));
        assert!(matches!(
            ev("1 div 0 = 1"),
            Err(EvalError::DivByZero { .. })
        ));
        assert!(matches!(
            ev("Declared(Foo)"),
            Err(EvalError::Declared { .. })
        ));
        assert!(matches!(ev("#13 = 'a'"), Err(EvalError::Syntax { .. })));
        assert!(matches!(ev("'''"), Err(EvalError::Syntax { .. })));
        match ev("FOO") {
            Err(EvalError::UnknownName {
                name,
                bool_position,
                ..
            }) => {
                assert_eq!(name, "FOO");
                assert!(bool_position);
            }
            other => panic!("{other:?}"),
        }
        match ev("Defined(PUY) and FOO") {
            Err(EvalError::UnknownName { bool_position, .. }) => assert!(bool_position),
            other => panic!("{other:?}"),
        }
        match ev("Level > 1") {
            Err(EvalError::UnknownName { bool_position, .. }) => assert!(!bool_position),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn compiler_version() {
        match ev("CompilerVersion >= 23") {
            Err(EvalError::CompilerVersion { fix, .. }) => assert_eq!(fix, Some(true)),
            other => panic!("{other:?}"),
        }
        match ev("23 > CompilerVersion") {
            Err(EvalError::CompilerVersion { fix, .. }) => assert_eq!(fix, Some(false)),
            other => panic!("{other:?}"),
        }
        match ev("RTLVersion = 23") {
            Err(EvalError::CompilerVersion { fix, .. }) => assert_eq!(fix, None),
            other => panic!("{other:?}"),
        }
        match ev("Defined(X) and (CompilerVersion >= 23)") {
            Err(EvalError::CompilerVersion { fix, .. }) => assert_eq!(fix, None),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn paren_limit() {
        let ok = format!("{}True{}", "(".repeat(256), ")".repeat(256));
        assert_eq!(ev(&ok), Ok(true));
        let bad = format!("{}True{}", "(".repeat(257), ")".repeat(257));
        assert!(matches!(ev(&bad), Err(EvalError::Syntax { .. })));
    }
}
