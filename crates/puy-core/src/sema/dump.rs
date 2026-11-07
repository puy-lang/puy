//! The `hir` dump (`docs/spec/semantics.md` chapter 9). The format of `puy check --dump hir` and of golden tests (`.hir`).
//!
//! A tree with one node per line. Children are indented by 2 columns. Statements carry a line number (`@line`), expressions carry a type.

use super::hir::*;
use super::types::Const;
use crate::source::SourceFile;
use crate::syntax::ast::ParamMode;
use std::fmt::Write;

/// Builds the `hir` dump. Each line ends with a single newline.
pub fn dump_hir(program: &Program, src: &SourceFile) -> String {
    let mut d = Dumper {
        out: String::new(),
        program,
        src,
        vars: &[],
    };
    d.program();
    d.out
}

struct Dumper<'a> {
    out: String,
    program: &'a Program,
    src: &'a SourceFile,
    /// Variables of the routine being dumped (names of the form `var vN`).
    vars: &'a [Var],
}

impl<'a> Dumper<'a> {
    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn line_no(&self, span: crate::source::TextRange) -> u32 {
        self.src.line_index(span.start()) + 1
    }

    fn program(&mut self) {
        let p = self.program;
        let mut head = format!("program {}", p.name);
        if p.uses_sysutils {
            head.push_str(" uses=System.SysUtils");
        }
        self.line(0, &head);
        for (i, g) in p.globals.iter().enumerate() {
            self.line(0, &format!(".global g{i} {}: {}", g.name, g.ty));
        }
        for (i, r) in p.routines.iter().enumerate() {
            self.routine(r, Some(i));
        }
        if let Some(init) = &p.init {
            self.routine(init, None);
        }
        self.routine(&p.main, None);
    }

    fn routine(&mut self, r: &'a Routine, index: Option<usize>) {
        self.vars = &r.vars;
        let mut head = String::from(".routine ");
        if let Some(i) = index {
            let _ = write!(head, "f{i} ");
        }
        head.push_str(&r.name);
        if index.is_some() {
            head.push('(');
            for (i, v) in r.vars.iter().take(r.param_count as usize).enumerate() {
                if i > 0 {
                    head.push_str("; ");
                }
                if let VarKind::Param(mode) = v.kind
                    && let Some(m) = mode_text(mode)
                {
                    head.push_str(m);
                    head.push(' ');
                }
                let _ = write!(head, "{}: {}", v.name, v.ty);
            }
            head.push(')');
            if let Some(t) = r.ret {
                let _ = write!(head, ": {t}");
            }
        }
        if r.has_syntax_errors {
            head.push_str(" syntax-errors");
        } else if r.has_errors {
            head.push_str(" errors");
        }
        self.line(0, &head);
        for (i, v) in r.vars.iter().enumerate() {
            let kind = match v.kind {
                VarKind::Param(ParamMode::Value) => "param",
                VarKind::Param(ParamMode::Const) => "const param",
                VarKind::Param(ParamMode::Var) => "var param",
                VarKind::Param(ParamMode::Out) => "out param",
                VarKind::Result => "result",
                VarKind::Local if v.is_for_var => "for var",
                VarKind::Local => "local",
                VarKind::Hidden => "hidden",
            };
            let name = if v.name.is_empty() { "-" } else { &v.name };
            self.line(1, &format!(".var v{i} {name}: {} {kind}", v.ty));
        }
        for s in &r.body {
            self.stmt(s, 1);
        }
    }

    fn stmt(&mut self, s: &Stmt, depth: usize) {
        let at = format!("@{}", self.line_no(s.span));
        match &s.kind {
            StmtKind::Assign {
                target,
                value,
                check,
                is_init,
            } => {
                let init = if *is_init { " init" } else { "" };
                self.line(depth, &format!("assign {at} {}{init}", range_flag(check)));
                self.place(target, depth + 1);
                self.expr(value, depth + 1);
            }
            StmtKind::Block(stmts) => {
                self.line(depth, &format!("block {at}"));
                for s in stmts {
                    self.stmt(s, depth + 1);
                }
            }
            StmtKind::Expr(e) => {
                self.line(depth, &format!("expr {at}"));
                self.expr(e, depth + 1);
            }
            StmtKind::If {
                branches,
                else_body,
            } => {
                self.line(depth, &format!("if {at}"));
                for (cond, body) in branches {
                    self.line(depth + 1, "cond");
                    self.expr(cond, depth + 2);
                    self.line(depth + 1, "then");
                    self.stmt(body, depth + 2);
                }
                if let Some(e) = else_body {
                    self.line(depth + 1, "else");
                    self.stmt(e, depth + 2);
                }
            }
            StmtKind::While { cond, body } => {
                self.line(depth, &format!("while {at}"));
                self.line(depth + 1, "cond");
                self.expr(cond, depth + 2);
                self.line(depth + 1, "body");
                self.stmt(body, depth + 2);
            }
            StmtKind::Repeat { body, cond } => {
                self.line(depth, &format!("repeat {at}"));
                self.line(depth + 1, "body");
                for s in body {
                    self.stmt(s, depth + 2);
                }
                self.line(depth + 1, "until");
                self.expr(cond, depth + 2);
            }
            StmtKind::For {
                var,
                from,
                to,
                downto,
                body,
                checks,
                ..
            } => {
                let dir = if *downto { "downto" } else { "to" };
                self.line(depth, &format!("for {at} {dir} {}", checks_flags(checks)));
                match var {
                    ForVar::Local(v) => {
                        let name = self.var_name(*v);
                        self.line(depth + 1, &format!("var v{} {name}", v.0));
                    }
                    ForVar::Global { id, counter } => {
                        let name = self.program.globals[id.0 as usize].name.clone();
                        self.line(
                            depth + 1,
                            &format!("global g{} {name} counter=v{}", id.0, counter.0),
                        );
                    }
                }
                self.line(depth + 1, "from");
                self.expr(from, depth + 2);
                self.line(depth + 1, "to");
                self.expr(to, depth + 2);
                self.line(depth + 1, "body");
                self.stmt(body, depth + 2);
            }
            StmtKind::Break => self.line(depth, &format!("break {at}")),
            StmtKind::Continue => self.line(depth, &format!("continue {at}")),
            StmtKind::Exit(value) => {
                self.line(depth, &format!("exit {at}"));
                if let Some(e) = value {
                    self.expr(e, depth + 1);
                }
            }
            StmtKind::Halt(code) => {
                self.line(depth, &format!("halt {at}"));
                if let Some(e) = code {
                    self.expr(e, depth + 1);
                }
            }
            StmtKind::IncDec {
                target,
                delta,
                negative,
                calc,
                checks,
                ..
            } => {
                let name = if *negative { "dec" } else { "inc" };
                self.line(
                    depth,
                    &format!("{name} {at} calc={calc} {}", checks_flags(checks)),
                );
                self.place(target, depth + 1);
                if let Some(e) = delta {
                    self.expr(e, depth + 1);
                }
            }
            StmtKind::ReadLn(place) => {
                self.line(depth, &format!("readln {at}"));
                if let Some(p) = place {
                    self.place(p, depth + 1);
                }
            }
            StmtKind::Write { args, newline } => {
                let name = if *newline { "writeln" } else { "write" };
                self.line(depth, &format!("{name} {at}"));
                for a in args {
                    self.line(depth + 1, "arg");
                    self.expr(&a.value, depth + 2);
                    if let Some(w) = &a.width {
                        self.line(depth + 1, "width");
                        self.expr(w, depth + 2);
                    }
                }
            }
            StmtKind::Assert {
                cond,
                message,
                text,
            } => {
                self.line(depth, &format!("assert {at} {}", escape_str(text)));
                self.expr(cond, depth + 1);
                if let Some(m) = message {
                    self.expr(m, depth + 1);
                }
            }
            StmtKind::Nop => self.line(depth, &format!("nop {at}")),
        }
    }

    fn place(&mut self, p: &Place, depth: usize) {
        match p {
            Place::Var(v, _) => {
                let name = self.var_name(*v);
                self.line(depth, &format!("var v{} {name}", v.0));
            }
            Place::Global(g) => {
                let name = self.program.globals[g.0 as usize].name.clone();
                self.line(depth, &format!("global g{} {name}", g.0));
            }
            Place::StrIndex { base, index, .. } => {
                self.line(depth, "index");
                self.place(base, depth + 1);
                self.expr(index, depth + 1);
            }
        }
    }

    fn expr(&mut self, e: &Expr, depth: usize) {
        let ty = e.ty;
        // Expressions with a constant value (literals, constant names, folded constant expressions, constant `Char`s converted to `string`)
        if let Some(v) = &e.value {
            self.line(depth, &format!("const {ty} {}", const_text(v)));
            return;
        }
        let folded = "";
        match &e.kind {
            ExprKind::Const => self.line(depth, &format!("const {ty} -")),
            ExprKind::Var(v) => {
                let name = self.var_name(*v);
                self.line(depth, &format!("var v{} {name}: {ty}{folded}", v.0));
            }
            ExprKind::Global(g) => {
                let name = self.program.globals[g.0 as usize].name.clone();
                self.line(depth, &format!("global g{} {name}: {ty}{folded}", g.0));
            }
            ExprKind::Binary { first, steps } => {
                self.line(depth, &format!("binary {ty}{folded}"));
                self.expr(first, depth + 1);
                for step in steps {
                    self.line(
                        depth + 1,
                        &format!("{} {} {}", step.op.text(), step.ty, calc_text(step.calc)),
                    );
                    self.expr(&step.rhs, depth + 2);
                }
            }
            ExprKind::Unary { op, operand, calc } => {
                let name = match op {
                    UnOp::Neg => "neg",
                    UnOp::Not => "not",
                };
                self.line(
                    depth,
                    &format!("unary {name} {ty} {}{folded}", calc_text(*calc)),
                );
                self.expr(operand, depth + 1);
            }
            ExprKind::StrIndex { base, index } => {
                self.line(depth, &format!("index {ty}{folded}"));
                self.expr(base, depth + 1);
                self.expr(index, depth + 1);
            }
            ExprKind::Call { routine, args } => {
                let r = &self.program.routines[routine.0 as usize];
                let ret = match r.ret {
                    Some(t) => t.to_string(),
                    None => "-".to_string(),
                };
                let name = r.name.clone();
                self.line(depth, &format!("call f{} {name}: {ret}{folded}", routine.0));
                for a in args {
                    match a {
                        CallArg::Value(e) => self.expr(e, depth + 1),
                        CallArg::Ref { place, is_out } => {
                            self.line(depth + 1, if *is_out { "out" } else { "ref" });
                            self.place(place, depth + 2);
                        }
                    }
                }
            }
            ExprKind::Builtin { func, args } => {
                self.line(
                    depth,
                    &format!("builtin {} {ty}{folded}", builtin_text(*func)),
                );
                for a in args {
                    self.expr(a, depth + 1);
                }
            }
            ExprKind::Convert { operand, kind } => {
                let text = match kind {
                    Conversion::Int { to, check } => {
                        format!("narrow {to} {}", if *check { "R+" } else { "R-" })
                    }
                    Conversion::CharToStr => "char->string".to_string(),
                    Conversion::IntToChar { check } => {
                        format!("int->char {}", if *check { "R+" } else { "R-" })
                    }
                    Conversion::IntToBool => "int->bool".to_string(),
                    Conversion::OrdToInt => format!("ord->{ty}"),
                };
                self.line(depth, &format!("convert {text}{folded}"));
                self.expr(operand, depth + 1);
            }
            ExprKind::Error => self.line(depth, "error"),
        }
    }

    fn var_name(&self, v: VarId) -> String {
        let name = &self.vars[v.0 as usize].name;
        if name.is_empty() {
            "-".to_string()
        } else {
            name.clone()
        }
    }
}

fn mode_text(mode: ParamMode) -> Option<&'static str> {
    match mode {
        ParamMode::Value => None,
        ParamMode::Const => Some("const"),
        ParamMode::Var => Some("var"),
        ParamMode::Out => Some("out"),
    }
}

fn range_flag(c: &Checks) -> &'static str {
    if c.range { "R+" } else { "R-" }
}

fn checks_flags(c: &Checks) -> String {
    format!(
        "Q{}R{}",
        if c.overflow { "+" } else { "-" },
        if c.range { "+" } else { "-" }
    )
}

fn calc_text(c: Calc) -> String {
    match c {
        Calc::Int(t) => format!("int:{t}"),
        Calc::Bool => "bool".to_string(),
        Calc::Str => "str".to_string(),
        Calc::Char => "char".to_string(),
    }
}

fn builtin_text(b: Builtin) -> &'static str {
    match b {
        Builtin::Length => "Length",
        Builtin::Copy => "Copy",
        Builtin::Pos => "Pos",
        Builtin::Ord => "Ord",
        Builtin::Chr => "Chr",
        Builtin::Abs => "Abs",
        Builtin::ParamCount => "ParamCount",
        Builtin::ParamStr => "ParamStr",
        Builtin::IntToStr => "IntToStr",
        Builtin::UIntToStr => "UIntToStr",
        Builtin::StrToInt => "StrToInt",
        Builtin::StrToInt64 => "StrToInt64",
        Builtin::StrToIntDef => "StrToIntDef",
        Builtin::UpperCase => "UpperCase",
        Builtin::LowerCase => "LowerCase",
        Builtin::Trim => "Trim",
        Builtin::GetEnvironmentVariable => "GetEnvironmentVariable",
    }
}

fn const_text(c: &Const) -> String {
    match c {
        Const::Int(v) => v.to_string(),
        Const::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Const::Char(u) => format!("#{u}"),
        Const::Str(s) => escape_units(s),
    }
}

/// Quotes a UTF-16 sequence in `"…"` with escapes (same as `str16` in the `bytecode` dump:
/// surrogate pairs are combined into a scalar value and escaped like `str`'s `Debug` output,
/// and lone surrogates are written as `\u{d800}`).
fn escape_units(units: &[u16]) -> String {
    let mut out = String::from("\"");
    for r in char::decode_utf16(units.iter().copied()) {
        match r {
            Ok('\'') => out.push('\''),
            Ok(c) => out.extend(c.escape_debug()),
            Err(e) => {
                let _ = write!(out, "\\u{{{:x}}}", e.unpaired_surrogate());
            }
        }
    }
    out.push('"');
    out
}

fn escape_str(s: &str) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    escape_units(&units)
}
