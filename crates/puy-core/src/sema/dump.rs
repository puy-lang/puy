//! The `hir` dump (`docs/spec/semantics.md` chapter 9). The format of `puy check --dump hir` and of golden tests (`.hir`).
//!
//! A tree with one node per line. Children are indented by 2 columns. Statements carry a line number (`@line`), expressions carry a type.

use super::hir::*;
use super::types::{Const, Ty, TypeKind, TypeTable, bound_text};
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

/// The kind column of a variable line (`.var` here, `.reg vN` in the `ir` dump).
pub(crate) fn var_kind_text(kind: VarKind, is_for_var: bool) -> String {
    let text = match kind {
        VarKind::Param(ParamMode::Value) => "param",
        VarKind::Param(ParamMode::Const) => "const param",
        VarKind::Param(ParamMode::Var) => "var param",
        VarKind::Param(ParamMode::Out) => "out param",
        VarKind::Result => "result",
        VarKind::Local if is_for_var => "for var",
        VarKind::Local => "local",
        VarKind::Hidden => "hidden",
        VarKind::Exception => "exception",
        // A variable of an enclosing routine (`docs/spec/semantics.md` 4.9)
        VarKind::Outer { up, var } => return format!("outer v{} up={up}", var.0),
    };
    text.into()
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

    /// A type name (`docs/spec/semantics.md` 9.1).
    fn t(&self, ty: Ty) -> String {
        self.program.types.name(ty)
    }

    fn line_no(&self, span: crate::source::TextRange) -> u32 {
        self.src.line_index(span.start()) + 1
    }

    /// The file passed to `puy`: the last unit of the program (`docs/spec/units.md` 8.3: the
    /// units it uses are not dumped).
    fn program(&mut self) {
        let p = self.program;
        let unit = p.units.len().saturating_sub(1) as u32;
        let part = p.units.last();
        let is_program = part.is_none_or(|u| u.is_program);
        let mut head = format!("{} {}", if is_program { "program" } else { "unit" }, p.name);
        let uses: Vec<String> = match part {
            Some(u) => u.uses.clone(),
            None if p.uses_sysutils => vec!["System.SysUtils".into()],
            None => Vec::new(),
        };
        if !uses.is_empty() {
            let _ = write!(head, " uses={}", uses.join(","));
        }
        self.line(0, &head);
        self.type_decls(None);
        for (i, g) in p.globals.iter().enumerate() {
            if g.unit.is_none_or(|u| u == unit) {
                self.line(0, &format!(".global g{i} {}: {}", g.name, self.t(g.ty)));
            }
        }
        for (i, r) in p.routines.iter().enumerate() {
            if r.unit == unit {
                self.routine(r, Some(i));
            }
        }
        let init = if is_program {
            p.init.as_ref()
        } else {
            part.and_then(|u| u.init.as_ref())
        };
        if let Some(init) = init {
            self.routine(init, None);
        }
        if let Some(fini) = part.and_then(|u| u.fini.as_ref()) {
            self.routine(fini, None);
        }
        if is_program {
            self.routine(&p.main, None);
        }
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
                let _ = write!(head, "{}: {}", v.name, self.t(v.ty));
            }
            head.push(')');
            if let Some(t) = r.ret {
                let _ = write!(head, ": {}", self.t(t));
            }
        }
        if r.has_syntax_errors {
            head.push_str(" syntax-errors");
        } else if r.has_errors {
            head.push_str(" errors");
        }
        self.line(0, &head);
        if index.is_some() {
            self.type_decls(Some(&r.name));
        }
        for (i, v) in r.vars.iter().enumerate() {
            let kind = var_kind_text(v.kind, v.is_for_var);
            let name = if v.name.is_empty() { "-" } else { &v.name };
            self.line(1, &format!(".var v{i} {name}: {} {kind}", self.t(v.ty)));
        }
        for s in &r.body {
            self.stmt(s, 1);
        }
    }

    /// The `type` declarations of the program (`routine` is `None`) or of a routine
    /// (`docs/spec/semantics.md` 9.1).
    fn type_decls(&mut self, routine: Option<&str>) {
        let depth = usize::from(routine.is_some());
        let decls: Vec<TypeDeclInfo> = self
            .program
            .type_decls
            .iter()
            .filter(|d| {
                d.routine.as_deref() == routine
                    && d.unit as usize + 1 == self.program.units.len().max(1)
            })
            .cloned()
            .collect();
        for d in decls {
            if !d.defines {
                self.line(depth, &format!(".type {} = {}", d.name, self.t(d.ty)));
                continue;
            }
            let types = &self.program.types;
            match d.ty {
                Ty::Record(_) => {
                    self.line(depth, &format!(".type {} = record", d.name));
                    let fields: Vec<String> = types
                        .fields(d.ty)
                        .iter()
                        .enumerate()
                        .map(|(i, f)| format!(".field {i} {}: {}", f.name, types.name(f.ty)))
                        .collect();
                    for f in fields {
                        self.line(depth + 1, &f);
                    }
                }
                Ty::Exception(id) => {
                    let parent = types
                        .parent(id)
                        .map_or("<error>".to_string(), |p| types.name(Ty::Exception(p)));
                    self.line(depth, &format!(".type {} = class({parent})", d.name));
                }
                Ty::Enum(_) => {
                    let values: Vec<String> = types
                        .enum_values(d.ty)
                        .iter()
                        .map(|v| format!("{} = {}", v.name, v.ord))
                        .collect();
                    self.line(
                        depth,
                        &format!(".type {} = ({})", d.name, values.join(", ")),
                    );
                }
                Ty::Subrange(_) => {
                    let def = subrange_definition(types, d.ty);
                    self.line(depth, &format!(".type {} = {def}", d.name));
                }
                Ty::Set(_) | Ty::BigSet(_) => {
                    let base = types.set_base(d.ty).unwrap_or(Ty::Error);
                    self.line(
                        depth,
                        &format!(".type {} = set of {}", d.name, types.name(base)),
                    );
                }
                Ty::DynArray(_) => {
                    let elem = types.dyn_elem(d.ty).unwrap_or(Ty::Error);
                    let def = if d.generic {
                        format!("TArray<{}>", types.name(elem))
                    } else {
                        format!("array of {}", types.name(elem))
                    };
                    self.line(depth, &format!(".type {} = {def}", d.name));
                }
                _ => {
                    let def = array_definition(types, d.ty);
                    self.line(depth, &format!(".type {} = {def}", d.name));
                }
            }
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
                counter,
                ..
            } => {
                let dir = if *downto { "downto" } else { "to" };
                self.line(depth, &format!("for {at} {dir} {}", checks_flags(checks)));
                let counter = counter.map(|c| format!(" counter=v{}", c.0));
                self.for_var(var, &counter.unwrap_or_default(), depth + 1);
                self.line(depth + 1, "from");
                self.expr(from, depth + 2);
                self.line(depth + 1, "to");
                self.expr(to, depth + 2);
                self.line(depth + 1, "body");
                self.stmt(body, depth + 2);
            }
            StmtKind::ForIn {
                var,
                coll,
                coll_var,
                idx_var,
                body,
                ..
            } => {
                self.line(
                    depth,
                    &format!("for-in {at} coll=v{} idx=v{}", coll_var.0, idx_var.0),
                );
                self.for_var(var, "", depth + 1);
                self.line(depth + 1, "in");
                self.expr(coll, depth + 2);
                self.line(depth + 1, "body");
                self.stmt(body, depth + 2);
            }
            StmtKind::Case {
                selector,
                arms,
                else_body,
            } => {
                self.line(depth, &format!("case {at}"));
                self.line(depth + 1, "selector");
                self.expr(selector, depth + 2);
                for arm in arms {
                    self.line(depth + 1, "arm");
                    for l in &arm.labels {
                        self.line(depth + 2, "label");
                        self.expr(&l.low, depth + 3);
                        if let Some(h) = &l.high {
                            self.expr(h, depth + 3);
                        }
                    }
                    if let Some(b) = &arm.body {
                        self.line(depth + 2, "then");
                        self.stmt(b, depth + 3);
                    }
                }
                if let Some(list) = else_body {
                    self.line(depth + 1, "else");
                    for s in list {
                        self.stmt(s, depth + 2);
                    }
                }
            }
            StmtKind::RaiseOuter(e) => {
                self.line(depth, &format!("raise-outer {at}"));
                self.expr(e, depth + 1);
            }
            StmtKind::Try { body, handler } => {
                self.line(depth, &format!("try {at}"));
                self.line(depth + 1, "body");
                for s in body {
                    self.stmt(s, depth + 2);
                }
                match handler {
                    TryHandler::Finally(list) => {
                        self.line(depth + 1, "finally");
                        for s in list {
                            self.stmt(s, depth + 2);
                        }
                    }
                    TryHandler::Except {
                        handlers,
                        else_body,
                    } => {
                        for h in handlers {
                            let head = match h.var {
                                Some(v) => {
                                    let name = &self.vars[v.0 as usize].name;
                                    format!("on v{} {name}: {}", v.0, self.t(h.class))
                                }
                                None => format!("on {}", self.t(h.class)),
                            };
                            self.line(depth + 1, &head);
                            if let Some(b) = &h.body {
                                self.stmt(b, depth + 2);
                            }
                        }
                        if let Some(list) = else_body {
                            let name = if handlers.is_empty() {
                                "except"
                            } else {
                                "else"
                            };
                            self.line(depth + 1, name);
                            for s in list {
                                self.stmt(s, depth + 2);
                            }
                        }
                    }
                }
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
                    &format!(
                        "{name} {at} calc={} {}",
                        self.t(*calc),
                        checks_flags(checks)
                    ),
                );
                self.place(target, depth + 1);
                if let Some(e) = delta {
                    self.expr(e, depth + 1);
                }
            }
            StmtKind::SetElem {
                target,
                elem,
                exclude,
                check,
            } => {
                let name = if *exclude { "exclude" } else { "include" };
                let flag = if *check { "R+" } else { "R-" };
                self.line(depth, &format!("{name} {at} {flag}"));
                self.place(target, depth + 1);
                self.expr(elem, depth + 1);
            }
            StmtKind::Read { targets, newline } => {
                let name = if *newline { "readln" } else { "read" };
                self.line(depth, &format!("{name} {at}"));
                for p in targets {
                    self.place(p, depth + 1);
                }
            }
            StmtKind::Flush => self.line(depth, &format!("flush {at}")),
            StmtKind::Write { args, newline, err } => {
                let name = if *newline { "writeln" } else { "write" };
                let to = if *err { " err" } else { "" };
                self.line(depth, &format!("{name} {at}{to}"));
                for a in args {
                    self.line(depth + 1, "arg");
                    self.expr(&a.value, depth + 2);
                    if let Some(w) = &a.width {
                        self.line(depth + 1, "width");
                        self.expr(w, depth + 2);
                    }
                    if let Some(d) = &a.precision {
                        self.line(depth + 1, "precision");
                        self.expr(d, depth + 2);
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
            StmtKind::SetLength { target, lens } => {
                self.line(depth, &format!("setlength {at}"));
                self.place(target, depth + 1);
                for l in lens {
                    self.expr(l, depth + 1);
                }
            }
            StmtKind::Raise(value) => {
                self.line(depth, &format!("raise {at}"));
                if let Some(e) = value {
                    self.expr(e, depth + 1);
                }
            }
            StmtKind::Insert {
                source,
                target,
                index,
                whole,
            } => {
                let kind = if *whole { " array" } else { "" };
                self.line(depth, &format!("insert {at}{kind}"));
                self.expr(source, depth + 1);
                self.place(target, depth + 1);
                self.expr(index, depth + 1);
            }
            StmtKind::Delete {
                target,
                index,
                count,
            } => {
                self.line(depth, &format!("delete {at}"));
                self.place(target, depth + 1);
                self.expr(index, depth + 1);
                self.expr(count, depth + 1);
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
            Place::Field {
                base,
                index,
                ty,
                name,
            } => {
                self.line(depth, &format!("field {} {name} #{index}", self.t(*ty)));
                self.place(base, depth + 1);
            }
            Place::Elem {
                base, index, ty, ..
            } => {
                self.line(depth, &format!("index {}", self.t(*ty)));
                self.place(base, depth + 1);
                self.expr(index, depth + 1);
            }
            Place::ExcMember { base, member } => {
                let ty = self.t(exc_member_type(*member));
                self.line(depth, &format!("member {ty} {}", member.name()));
                self.expr(base, depth + 1);
            }
        }
    }

    fn expr(&mut self, e: &Expr, depth: usize) {
        let ty = self.t(e.ty);
        // Expressions with a constant value (literals, constant names, folded constant expressions, constant `Char`s converted to `string`)
        if let Some(v) = &e.value {
            let text = const_value_text(v, e.ty, &self.program.types);
            self.line(depth, &format!("const {ty} {text}"));
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
                        &format!(
                            "{} {} {}",
                            step.op.text(),
                            self.t(step.ty),
                            calc_text(step.calc, &self.program.types)
                        ),
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
                    &format!(
                        "unary {name} {ty} {}{folded}",
                        calc_text(*calc, &self.program.types)
                    ),
                );
                self.expr(operand, depth + 1);
            }
            ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
                self.line(depth, &format!("index {ty}{folded}"));
                self.expr(base, depth + 1);
                self.expr(index, depth + 1);
            }
            ExprKind::Slice { base, low, high } => {
                // The form names the bounds that follow (`L..H`, `L..`, `..H`, `..`)
                let l = if low.is_some() { "L" } else { "" };
                let h = if high.is_some() { "H" } else { "" };
                self.line(depth, &format!("slice {ty} {l}..{h}"));
                self.expr(base, depth + 1);
                for b in low.iter().chain(high.iter()) {
                    self.expr(b, depth + 1);
                }
            }
            ExprKind::Field { base, index, name } => {
                self.line(depth, &format!("field {ty} {name} #{index}"));
                self.expr(base, depth + 1);
            }
            ExprKind::ArrayCtor(elems) => {
                self.line(depth, &format!("array {ty}"));
                for x in elems {
                    self.expr(x, depth + 1);
                }
            }
            ExprKind::SetCtor(elems) => {
                self.line(depth, &format!("set {ty}"));
                for el in elems {
                    let flag = if el.check { "R+" } else { "R-" };
                    match &el.high {
                        Some(h) => {
                            self.line(depth + 1, &format!("range {flag}"));
                            self.expr(&el.low, depth + 2);
                            self.expr(h, depth + 2);
                        }
                        None => {
                            self.line(depth + 1, &format!("elem {flag}"));
                            self.expr(&el.low, depth + 2);
                        }
                    }
                }
            }
            ExprKind::Call { routine, args } => {
                let r = &self.program.routines[routine.0 as usize];
                let ret = match r.ret {
                    Some(t) => self.t(t),
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
                        format!(
                            "narrow {} {}",
                            self.t(*to),
                            if *check { "R+" } else { "R-" }
                        )
                    }
                    Conversion::CharToStr => "char->string".to_string(),
                    Conversion::StrToU8 => "string->utf8".to_string(),
                    Conversion::U8ToStr => "utf8->string".to_string(),
                    Conversion::IntToChar { check } => {
                        format!("int->char {}", if *check { "R+" } else { "R-" })
                    }
                    Conversion::IntToBool => "int->bool".to_string(),
                    Conversion::OrdToInt => format!("ord->{ty}"),
                    Conversion::Range { to, check } => {
                        format!("range {} {}", self.t(*to), if *check { "R+" } else { "R-" })
                    }
                    Conversion::Set { to, check } => {
                        format!("set {} {}", self.t(*to), if *check { "R+" } else { "R-" })
                    }
                    Conversion::Num { to } => format!("num->{}", self.t(*to)),
                };
                self.line(depth, &format!("convert {text}{folded}"));
                self.expr(operand, depth + 1);
            }
            ExprKind::NewExc { class, msg } => {
                self.line(depth, &format!("new {}", self.t(*class)));
                self.expr(msg, depth + 1);
            }
            ExprKind::ExcMember { base, member } => {
                self.line(depth, &format!("member {ty} {}", member.name()));
                self.expr(base, depth + 1);
            }
            ExprKind::TryStrToInt { s, target } => {
                self.line(depth, &format!("builtin TryStrToInt {ty}"));
                self.expr(s, depth + 1);
                self.line(depth + 1, "out");
                self.place(target, depth + 2);
            }
            ExprKind::Error => self.line(depth, "error"),
        }
    }

    /// The loop variable line of `for` / `for-in` (`suffix` follows a global's name).
    fn for_var(&mut self, var: &ForVar, suffix: &str, depth: usize) {
        let text = match var {
            ForVar::Local(v) => format!("var v{} {}", v.0, self.var_name(*v)),
            ForVar::Global(id) => {
                let name = &self.program.globals[id.0 as usize].name;
                format!("global g{} {name}{suffix}", id.0)
            }
        };
        self.line(depth, &text);
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

/// The type of a member of an exception object (`docs/spec/exceptions.md` 2.1).
pub(crate) fn exc_member_type(m: ExcMember) -> Ty {
    match m {
        ExcMember::Message | ExcMember::StackTrace | ExcMember::ClassName | ExcMember::Value => {
            Ty::String
        }
        ExcMember::Suppressed => {
            Ty::DynArray(super::types::TypeId(crate::bytecode::EXCEPTION_ARRAY_TYPE))
        }
        ExcMember::InnerException => Ty::Exception(super::types::EXCEPTION),
        ExcMember::ErrorCode => Ty::Integer,
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

fn calc_text(c: Calc, types: &TypeTable) -> String {
    match c {
        Calc::Int(t) => format!("int:{}", types.name(t)),
        Calc::Bool => "bool".to_string(),
        Calc::Str => "str".to_string(),
        Calc::U8 => "utf8".to_string(),
        Calc::Char => "char".to_string(),
        Calc::Ord => "ord".to_string(),
        Calc::Ref => "ref".to_string(),
        Calc::Set => "set".to_string(),
        Calc::SetIn => "in".to_string(),
        Calc::Real(t) => format!("real:{}", types.name(t)),
        Calc::Curr => "curr".to_string(),
        Calc::Arr => "arr".to_string(),
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
        Builtin::Default => "Default",
        Builtin::Low => "Low",
        Builtin::High => "High",
        Builtin::Assigned => "Assigned",
        Builtin::Succ => "Succ",
        Builtin::Pred => "Pred",
        Builtin::Sqrt => "Sqrt",
        Builtin::Sqr => "Sqr",
        Builtin::Pi => "Pi",
        Builtin::Trunc => "Trunc",
        Builtin::Round => "Round",
        Builtin::FloatToStr => "FloatToStr",
        Builtin::StrToFloat => "StrToFloat",
        Builtin::Odd => "Odd",
        Builtin::UpCase => "UpCase",
        Builtin::SizeOf => "SizeOf",
        Builtin::Concat => "Concat",
        Builtin::Format => "Format",
        Builtin::IntToHex => "IntToHex",
        Builtin::StrToBool => "StrToBool",
        Builtin::BoolToStr => "BoolToStr",
        Builtin::TrimLeft => "TrimLeft",
        Builtin::TrimRight => "TrimRight",
        Builtin::SameText => "SameText",
        Builtin::CompareText => "CompareText",
        Builtin::CompareStr => "CompareStr",
        Builtin::StringReplace => "StringReplace",
        Builtin::StringOfChar => "StringOfChar",
        Builtin::QuotedStr => "QuotedStr",
        Builtin::TryStrToInt => "TryStrToInt",
        Builtin::AnsiUpperCase => "AnsiUpperCase",
        Builtin::AnsiLowerCase => "AnsiLowerCase",
        Builtin::AnsiSameText => "AnsiSameText",
        Builtin::AnsiCompareText => "AnsiCompareText",
        Builtin::Eof => "Eof",
        Builtin::Eoln => "Eoln",
    }
}

/// A constant value in the `hir` and `ir` dumps: `1`, `True`, `#65`, `"a"`, and a record or static
/// array constant as `(X: 1; Name: "a")` / `(1, 2, 3)` (`docs/spec/semantics.md` 9.1).
pub(crate) fn const_value_text(c: &Const, ty: Ty, types: &TypeTable) -> String {
    match c {
        Const::Int(v) => v.to_string(),
        // The default text of the value (`docs/spec/semantics.md` 6.1); a `Single` as binary32
        Const::Real(b) if ty == Ty::Single => crate::numfmt::f32_to_str(f64::from_bits(*b) as f32),
        Const::Real(b) => crate::numfmt::f64_to_str(f64::from_bits(*b)),
        Const::Curr(c) => crate::numfmt::curr_to_str(*c),
        Const::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Const::Char(u) => format!("#{u}"),
        Const::Str(s) => escape_units(s),
        Const::U8(s) => format!(
            "utf8 {}",
            escape_units(&s.encode_utf16().collect::<Vec<_>>())
        ),
        Const::Nil => "nil".to_string(),
        Const::Set(words) => format!("[{}]", crate::bytecode::set_ranges_text(words)),
        Const::Agg(items) => match ty {
            Ty::Record(_) => {
                let parts: Vec<String> = types
                    .fields(ty)
                    .iter()
                    .zip(items)
                    .map(|(f, v)| format!("{}: {}", f.name, const_value_text(v, f.ty, types)))
                    .collect();
                format!("({})", parts.join("; "))
            }
            _ => {
                let elem = types.elem_of(ty).unwrap_or(Ty::Error);
                let parts: Vec<String> = items
                    .iter()
                    .map(|v| const_value_text(v, elem, types))
                    .collect();
                // A dynamic array constant is written like its constructor
                if matches!(ty, Ty::DynArray(_)) {
                    format!("[{}]", parts.join(", "))
                } else {
                    format!("({})", parts.join(", "))
                }
            }
        },
    }
}

/// The definition of a subrange type in a `.type` line: `0..9`, `#97..#122`, and the bounds of
/// an enumeration as ordinals `TDay(1)..TDay(5)` (`docs/spec/semantics.md` 9).
fn subrange_definition(types: &TypeTable, ty: Ty) -> String {
    let base = types.base(ty);
    let (lo, hi) = types.ord_range(ty).unwrap_or((0, 0));
    match base {
        Ty::Enum(_) => {
            let b = types.name(base);
            format!("{b}({lo})..{b}({hi})")
        }
        _ => format!("{}..{}", types.bound_text(ty, lo), types.bound_text(ty, hi)),
    }
}

/// The definition of a static array type in a `.type` line: `array[L..H] of T`, with a
/// multi-dimensional array written as nested arrays (`docs/spec/semantics.md` 9.1).
fn array_definition(types: &TypeTable, ty: Ty) -> String {
    match ty.type_id().map(|id| &types.get(id).kind) {
        Some(TypeKind::Array {
            index,
            low,
            high,
            elem,
        }) => format!(
            "array[{}..{}] of {}",
            bound_text(*index, *low),
            bound_text(*index, *high),
            types.name(*elem)
        ),
        _ => types.name(ty),
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
