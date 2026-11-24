//! The `hir` dump (`docs/spec/semantics.md` chapter 9). The format of `puy check --dump hir` and of golden tests (`.hir`).
//!
//! A tree with one node per line. Children are indented by 2 columns. Statements carry a line number (`@line`), expressions carry a type.

use super::hir::*;
use super::types::{Const, Ty, TypeKind, TypeTable, bound_text};
use crate::source::{SourceFile, TextRange};
use crate::syntax::ast::ParamMode;
use std::collections::HashMap;
use std::fmt::Write;

/// Builds the `hir` dump. Each line ends with a single newline.
pub fn dump_hir(program: &Program, src: &SourceFile) -> String {
    let mut d = Dumper {
        out: String::new(),
        program,
        src,
        vars: &[],
        funcs: FuncNumbers::new(program),
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
        VarKind::SelfParam => "self",
        VarKind::RecordSelf => "var self",
        // A variable of an enclosing routine (`docs/spec/semantics.md` 4.9)
        VarKind::Outer { up, var } => return format!("outer v{} up={up}", var.0),
        VarKind::Cell { var, owned: true } => return format!("cell v{}", var.0),
        VarKind::Cell { var, owned: false } => return format!("capture v{}", var.0),
    };
    text.into()
}

struct Dumper<'a> {
    out: String,
    program: &'a Program,
    src: &'a SourceFile,
    /// Variables of the routine being dumped (names of the form `var vN`).
    vars: &'a [Var],
    /// The numbers of the functions in the dump (`fN`).
    funcs: FuncNumbers,
}

/// The function numbers of the dump (`docs/spec/semantics.md` chapter 9): the routines whose
/// headings the dump shows are numbered from `f0` in the order of their headings, and the
/// routines it only refers to continue the numbers in the order of their first reference. The
/// routines of the units the file uses do not shift the numbers.
///
/// A reference is written as a placeholder (`f`) and numbered when its line is emitted
/// (`resolve`), so that the order of first reference is the order of the lines in the dump even
/// where the text of several lines is built before they are emitted (the members of a type).
struct FuncNumbers {
    numbers: HashMap<RoutineId, u32>,
}

/// Encloses the routine id of a placeholder written by `FuncNumbers::f`.
const FUNC_MARK: char = '\u{0}';

impl FuncNumbers {
    /// Numbers the routines whose headings the dump shows, in the order `Dumper::program` shows them.
    fn new(p: &Program) -> Self {
        let unit = p.units.len().saturating_sub(1) as u32;
        let shown = file_routines(p, unit)
            .chain(file_instances(p, unit).flat_map(|inst| instance_routines(p, inst).copied()))
            .chain(file_generated(p, unit).map(|g| g.routine));
        let mut numbers = HashMap::new();
        for r in shown {
            // A generic body has no function (`docs/spec/generics.md` 8)
            if !p.routines[r.0 as usize].generic {
                let next = numbers.len() as u32;
                numbers.entry(r).or_insert(next);
            }
        }
        FuncNumbers { numbers }
    }

    /// The placeholder of `fN` for a routine, numbered by `resolve` when its line is emitted.
    fn f(&self, r: RoutineId) -> String {
        format!("{FUNC_MARK}{}{FUNC_MARK}", r.0)
    }

    /// Replaces the placeholders of a line with `fN`; a routine the dump has not numbered yet
    /// takes the next number.
    fn resolve(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut parts = text.split(FUNC_MARK);
        out.push_str(parts.next().unwrap_or_default());
        while let (Some(id), Some(rest)) = (parts.next(), parts.next()) {
            let id: u32 = id.parse().expect("a placeholder holds a routine id");
            let next = self.numbers.len() as u32;
            let n = self.numbers.entry(RoutineId(id)).or_insert(next);
            let _ = write!(out, "f{n}");
            out.push_str(rest);
        }
        out
    }
}

/// The routines of the file `unit` the dump shows before the instantiations: an abstract method
/// has no body (`docs/spec/objects.md` 15.1), and the routines of the instantiations are shown
/// under them.
fn file_routines(p: &Program, unit: u32) -> impl Iterator<Item = RoutineId> + '_ {
    p.routines
        .iter()
        .enumerate()
        .filter(move |(_, r)| r.unit == unit && !r.is_abstract && r.instance.is_none())
        .map(|(i, _)| RoutineId(i as u32))
}

/// The instantiations the file `unit` asked for first (`docs/spec/generics.md` 8).
fn file_instances(p: &Program, unit: u32) -> impl Iterator<Item = &InstanceInfo> {
    p.instances.iter().filter(move |i| i.unit == unit)
}

/// The routines of an instantiation the dump shows under it (those with a body).
fn instance_routines<'p>(
    p: &'p Program,
    inst: &'p InstanceInfo,
) -> impl Iterator<Item = &'p RoutineId> {
    inst.routines
        .iter()
        .filter(|r| !p.routines[r.0 as usize].is_abstract)
}

/// The generated functions the file `unit` asked for first (`docs/spec/equality.md` 9).
fn file_generated(p: &Program, unit: u32) -> impl Iterator<Item = &GeneratedInfo> {
    p.generated.iter().filter(move |g| g.unit == unit)
}

impl<'a> Dumper<'a> {
    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        let text = self.funcs.resolve(text);
        self.out.push_str(&text);
        self.out.push('\n');
    }

    /// The attribute classes written on the declaration named at `decl`
    /// (`docs/spec/attributes.md` 7): `.attr Hello.TableAttribute f0` with the arguments under it.
    fn attr_lines(&mut self, decl: TextRange, depth: usize) {
        let unit = self.program.units.len().saturating_sub(1) as u32;
        let program = self.program;
        for a in program
            .attrs
            .iter()
            .filter(|a| a.unit == unit && a.decl == decl)
        {
            let ctor = a.ctor.map_or("builtin".to_string(), |r| self.funcs.f(r));
            self.line(depth, &format!(".attr {} {ctor}", a.class));
            for e in &a.args {
                self.expr(e, depth + 1);
            }
        }
    }

    /// ` deprecated` / ` experimental` after the line of a declaration named at `decl` that has
    /// these hint directives (`docs/spec/attributes.md` 7).
    fn hint_mark(&self, decl: TextRange) -> &'static str {
        let Some(file) = self.program.units.last().map(|u| u.file) else {
            return "";
        };
        match self
            .program
            .hints
            .iter()
            .find(|h| h.decl.file == file && h.decl.range == decl)
        {
            Some(h) if h.deprecated && h.experimental => " deprecated experimental",
            Some(h) if h.deprecated => " deprecated",
            Some(h) if h.experimental => " experimental",
            _ => "",
        }
    }

    /// The lines of a type's members, each followed by its attributes.
    fn member_block(&mut self, depth: usize, lines: Vec<(String, Option<TextRange>)>) {
        for (line, decl) in lines {
            let mark = decl.map_or("", |d| self.hint_mark(d));
            self.line(depth, &format!("{line}{mark}"));
            if let Some(d) = decl {
                self.attr_lines(d, depth + 1);
            }
        }
    }

    /// A type name (`docs/spec/semantics.md` chapter 9).
    fn t(&self, ty: Ty) -> String {
        self.program.types.name(ty)
    }

    fn line_no(&self, span: TextRange) -> u32 {
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
                let weak = weak_mark(g.ty);
                // A `class var`'s marks are shown on its line in the class
                let mark = match g.span {
                    Some(s) if !g.name.contains('.') => self.hint_mark(s),
                    _ => "",
                };
                self.line(
                    0,
                    &format!(".global g{i} {}: {}{weak}{mark}", g.name, self.t(g.ty)),
                );
                // A `class var`'s attributes are shown on its line in the class
                if let Some(s) = g.span.filter(|_| !g.name.contains('.')) {
                    self.attr_lines(s, 1);
                }
            }
        }
        for r in file_routines(p, unit) {
            self.routine(&p.routines[r.0 as usize], Some(r.0 as usize));
        }
        for inst in file_instances(p, unit) {
            self.line(0, &format!(".instance {}", inst.name));
            for r in instance_routines(p, inst) {
                self.routine(&p.routines[r.0 as usize], Some(r.0 as usize));
            }
        }
        for g in file_generated(p, unit) {
            let routine = &p.routines[g.routine.0 as usize];
            let name = routine.instance.as_deref().unwrap_or(&routine.name);
            self.line(0, &format!(".generated {name}"));
            self.routine(routine, Some(g.routine.0 as usize));
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
            // A generic body has no function (`docs/spec/generics.md` 8)
            if r.generic {
                head.push_str("- ");
            } else {
                let _ = write!(head, "{} ", self.funcs.f(RoutineId(i as u32)));
            }
        }
        head.push_str(&r.name);
        if index.is_some() {
            head.push('(');
            let params = r
                .vars
                .iter()
                .take(r.param_count as usize)
                .filter(|v| !matches!(v.kind, VarKind::SelfParam | VarKind::RecordSelf));
            for (i, v) in params.enumerate() {
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
        if r.closure {
            head.push_str(" closure");
        }
        if r.has_syntax_errors {
            head.push_str(" syntax-errors");
        } else if r.has_errors {
            head.push_str(" errors");
        }
        if index.is_some() && !r.name.contains('.') {
            head.push_str(self.hint_mark(r.name_span));
        }
        self.line(0, &head);
        // A method's attributes are shown on its line in the class (`docs/spec/attributes.md` 7)
        if index.is_some() && !r.name.contains('.') {
            self.attr_lines(r.name_span, 1);
        }
        if let Some(i) = index {
            self.type_decls(Some(RoutineId(i as u32)));
        }
        for (i, v) in r.vars.iter().enumerate() {
            let kind = var_kind_text(v.kind, v.is_for_var);
            let name = if v.name.is_empty() { "-" } else { &v.name };
            let weak = weak_mark(v.ty);
            let cell = if v.cell { " cell" } else { "" };
            let mark = if v.name.is_empty() {
                ""
            } else {
                self.hint_mark(v.span)
            };
            self.line(
                1,
                &format!(
                    ".var v{i} {name}: {} {kind}{weak}{cell}{mark}",
                    self.t(v.ty)
                ),
            );
            if !v.name.is_empty() {
                self.attr_lines(v.span, 2);
            }
        }
        // The variables a closure captures (`docs/spec/objects.md` 15.5)
        for (i, c) in r.captures.iter().enumerate() {
            let v = &r.vars[c.0 as usize];
            // A closure-form nested routine's index in its group's block
            let k = r.groups.slots.get(i).map_or(i, |s| *s as usize);
            let name = if v.name.is_empty() { "-" } else { &v.name };
            let VarKind::Outer { up, var } = v.kind else {
                continue;
            };
            let mut owner = Some(r);
            for _ in 0..up {
                owner = owner
                    .and_then(|o| o.parent)
                    .map(|p| &self.program.routines[p.0 as usize]);
            }
            let from = match owner.map(|o| &o.vars[var.0 as usize]) {
                Some(o) if o.kind == VarKind::SelfParam => "self".to_string(),
                _ if up == 1 => format!("from v{}", var.0),
                Some(_) => format!("from {}.v{}", owner.map_or("", |o| o.name.as_str()), var.0),
                None => format!("from v{} up={up}", var.0),
            };
            self.line(1, &format!(".capture #{k} {name}: {} {from}", self.t(v.ty)));
        }
        // The group blocks made on entry, and a closure-form routine's own block copied
        // (`docs/spec/objects.md` 15.5)
        if let Some(v) = r.groups.own {
            self.line(1, &format!(".own-block v{}", v.0));
        }
        for (v, cells) in &r.groups.made {
            let list: Vec<String> = cells.iter().map(|c| format!("v{}", c.0)).collect();
            self.line(1, &format!(".group v{} [{}]", v.0, list.join(", ")));
        }
        for s in &r.body {
            self.stmt(s, 1);
        }
    }

    /// The `type` declarations of the program (`routine` is `None`) or of a routine
    /// (`docs/spec/semantics.md` chapter 9).
    fn type_decls(&mut self, routine: Option<RoutineId>) {
        let depth = usize::from(routine.is_some());
        let decls: Vec<TypeDeclInfo> = self
            .program
            .type_decls
            .iter()
            .filter(|d| {
                d.routine == routine && d.unit as usize + 1 == self.program.units.len().max(1)
            })
            .cloned()
            .collect();
        for d in decls {
            let start = self.out.len();
            self.type_decl(&d, depth);
            // The hint directives on its line (`docs/spec/attributes.md` 7)
            let mark = self.hint_mark(d.span);
            if let Some(eol) = self.out[start..].find('\n') {
                self.out.insert_str(start + eol, mark);
            }
        }
    }

    /// The lines of one `type` declaration (`docs/spec/semantics.md` chapter 9).
    fn type_decl(&mut self, d: &TypeDeclInfo, depth: usize) {
        if !d.defines {
            self.line(depth, &format!(".type {} = {}", d.name, self.t(d.ty)));
            return;
        }
        let types = &self.program.types;
        match d.ty {
            Ty::Record(id) => {
                self.line(depth, &format!(".type {} = record", d.name));
                self.attr_lines(d.span, depth + 1);
                let mut fields: Vec<(String, Option<TextRange>)> = types
                    .fields(d.ty)
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let weak = weak_mark(f.ty);
                        (
                            format!(".field {i} {}: {}{weak}", f.name, types.name(f.ty)),
                            Some(f.span),
                        )
                    })
                    .collect();
                fields.extend(member_lines(types, &self.program.routines, &self.funcs, id));
                self.member_block(depth + 1, fields);
            }
            Ty::Class(id) => {
                let parent = types
                    .parent(id)
                    .map_or("<error>".to_string(), |p| types.name(Ty::Class(p)));
                let mut head = format!(".type {} = class({parent})", d.name);
                // `docs/spec/objects.md` 15.1
                if types.slot_kind(Ty::Class(id)) == crate::bytecode::SLOT_REF {
                    head.push_str(" ref");
                }
                if types.class_is_abstract(id) {
                    head.push_str(" abstract");
                }
                if let super::types::TypeKind::Class { sealed: true, .. } = types.get(id).kind {
                    head.push_str(" sealed");
                }
                self.line(depth, &head);
                self.attr_lines(d.span, depth + 1);
                let mut lines: Vec<(String, Option<TextRange>)> = match &types.get(id).kind {
                    super::types::TypeKind::Class { fields, .. } => fields
                        .iter()
                        .map(|f| {
                            let weak = weak_mark(f.ty);
                            (
                                format!(".field {} {}: {}{weak}", f.slot, f.name, types.name(f.ty)),
                                Some(f.span),
                            )
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                lines.extend(member_lines(types, &self.program.routines, &self.funcs, id));
                lines.extend(implements_lines(types, &self.funcs, id));
                self.member_block(depth + 1, lines);
            }
            Ty::Intf(id) => {
                // `docs/spec/objects.md` 15.4
                let parent = types
                    .parent(id)
                    .map_or("<error>".to_string(), |p| types.name(Ty::Intf(p)));
                let mut head = format!(".type {} = interface({parent})", d.name);
                if let super::types::TypeKind::Interface { guid: Some(g), .. } = types.get(id).kind
                {
                    let _ = write!(head, " guid={}", crate::bytecode::guid_text(&g));
                }
                self.line(depth, &head);
                self.attr_lines(d.span, depth + 1);
                let lines = intf_member_lines(types, id);
                self.member_block(depth + 1, lines);
            }
            Ty::Helper(id) => {
                // `docs/spec/objects.md` 15.6
                let (is_class, target) = match &types.get(id).kind {
                    super::types::TypeKind::Helper {
                        is_class, target, ..
                    } => (*is_class, *target),
                    _ => (false, Ty::Error),
                };
                let kind = if is_class { "class" } else { "record" };
                let parent = types
                    .parent(id)
                    .map(|p| format!("({})", types.name(Ty::Helper(p))))
                    .unwrap_or_default();
                self.line(
                    depth,
                    &format!(
                        ".type {} = {kind} helper{parent} for {}",
                        d.name,
                        types.name(target)
                    ),
                );
                self.attr_lines(d.span, depth + 1);
                let lines = member_lines(types, &self.program.routines, &self.funcs, id);
                self.member_block(depth + 1, lines);
            }
            // `docs/spec/objects.md` 15.5
            Ty::Proc(id) | Ty::Callable(id) => {
                let text = types.proc_text(id, super::types::NameForm::Short, true);
                self.line(depth, &format!(".type {} = {text}", d.name));
            }
            Ty::ClassRef(_) => {
                let class = types.class_ref_target(d.ty).map_or(Ty::Error, Ty::Class);
                self.line(
                    depth,
                    &format!(".type {} = class of {}", d.name, types.name(class)),
                );
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
            // `docs/spec/types.md` 1.3: the original type
            Ty::Distinct(_) => {
                let base = types.origin(d.ty).unwrap_or(Ty::Error);
                self.line(
                    depth,
                    &format!(".type {} = type {}", d.name, types.name(base)),
                );
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

    fn stmt(&mut self, s: &Stmt, depth: usize) {
        let at = format!("@{}", self.line_no(s.span));
        match &s.kind {
            // Not shown: only a captured variable makes code there (a new cell)
            StmtKind::Declare(_) => {}
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
            StmtKind::PropertySet { name, call } => {
                // `docs/spec/objects.md` 15.1: the target shows the `write` method, the receiver
                // and the indices; the value is the last argument
                self.line(depth, &format!("assign {at}"));
                let ExprKind::Call { routine, args, .. } = &call.kind else {
                    unreachable!("PropertySet always holds the call of the write method");
                };
                self.line(
                    depth + 1,
                    &format!("property {name} {}", self.funcs.f(*routine)),
                );
                let (value, rest) = args.split_last().expect("the value is an argument");
                for a in rest {
                    if let CallArg::Value(e) = a {
                        self.expr(e, depth + 2);
                    }
                }
                if let CallArg::Value(v) = value {
                    self.expr(v, depth + 1);
                }
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
                enumerator,
                ..
            } => {
                let (head, pos) = match enumerator {
                    Some(_) => ("for-in-enum", "enum"),
                    None => ("for-in", "idx"),
                };
                self.line(
                    depth,
                    &format!("{head} {at} coll=v{} {pos}=v{}", coll_var.0, idx_var.0),
                );
                let last = enumerator
                    .as_ref()
                    .and_then(|e| e.last)
                    .map(|l| format!(" last=v{}", l.0));
                self.for_var(var, &last.unwrap_or_default(), depth + 1);
                self.line(depth + 1, "in");
                self.expr(coll, depth + 2);
                if let Some(e) = enumerator {
                    self.line(depth + 1, "get-enumerator");
                    self.expr(&e.get, depth + 2);
                    self.line(depth + 1, "move-next");
                    self.expr(&e.move_next, depth + 2);
                    self.line(depth + 1, "current");
                    self.expr(&e.current, depth + 2);
                }
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
            StmtKind::RequestLeakReport => self.line(depth, &format!("requestleakreport {at}")),
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
                probes,
                marks: _,
            } => {
                self.line(depth, &format!("assert {at} {}", escape_str(text)));
                self.expr(cond, depth + 1);
                if let Some(m) = message {
                    self.expr(m, depth + 1);
                }
                // `docs/spec/semantics.md` 5.4: the subexpressions a failure records
                for (k, p) in probes.iter().enumerate() {
                    let mark = p.mark.map_or(String::new(), |m| format!(" mark#{m}"));
                    let ty = self.program.types.name(p.ty);
                    self.line(
                        depth + 1,
                        &format!("probe #{k} {ty} {}{mark}", escape_str(&p.text)),
                    );
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
            StmtKind::Exchange {
                target,
                first,
                second,
                ..
            } => {
                self.line(depth, &format!("exchange {at}"));
                self.place(target, depth + 1);
                self.expr(first, depth + 1);
                self.expr(second, depth + 1);
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
            StmtKind::ArrayMoveRange {
                target,
                from,
                to,
                count,
            } => {
                self.line(depth, &format!("array-move-range {at}"));
                self.place(target, depth + 1);
                self.expr(from, depth + 1);
                self.expr(to, depth + 1);
                self.expr(count, depth + 1);
            }
            StmtKind::ArraySortRange {
                target,
                index,
                count,
            } => {
                self.line(depth, &format!("array-sort-range {at}"));
                self.place(target, depth + 1);
                self.expr(index, depth + 1);
                self.expr(count, depth + 1);
            }
            StmtKind::HashRelink {
                buckets,
                hashes,
                next,
                count,
            } => {
                self.line(depth, &format!("hash-relink {at}"));
                self.place(buckets, depth + 1);
                self.expr(hashes, depth + 1);
                self.place(next, depth + 1);
                self.expr(count, depth + 1);
            }
            StmtKind::FreeIfClass { value } => {
                self.line(depth, &format!("free-if-class {at}"));
                self.expr(value, depth + 1);
            }
            StmtKind::CheckDefaultOrder { value } => {
                self.line(depth, &format!("check-default-order {at}"));
                self.expr(value, depth + 1);
            }
            StmtKind::CheckClassType { value, what } => {
                self.line(depth, &format!("check-class-type {at}"));
                self.expr(value, depth + 1);
                self.expr(what, depth + 1);
            }
            StmtKind::Free { target, destroy } => {
                let flag = if *destroy { " destroy" } else { "" };
                self.line(depth, &format!("free {at}{flag}"));
                self.expr(target, depth + 1);
            }
            StmtKind::FreeAndNil { target } => {
                self.line(depth, &format!("free {at} and-nil"));
                self.place(target, depth + 1);
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
                let name = self.program.global(*g).name.clone();
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
            Place::ObjField {
                base,
                slot,
                ty,
                name,
            } => {
                self.line(depth, &format!("objfield {} {name} #{slot}", self.t(*ty)));
                self.expr(base, depth + 1);
            }
        }
    }

    /// An argument of a call: a value, `ref` / `out` and the place, or `temp` and the value
    /// copied to the temporary (`docs/spec/objects.md` 15.1).
    fn call_arg(&mut self, a: &CallArg, depth: usize) {
        match a {
            CallArg::Value(e) => self.expr(e, depth),
            CallArg::Ref { place, is_out } => {
                self.line(depth, if *is_out { "out" } else { "ref" });
                self.place(place, depth + 1);
            }
            CallArg::Temp(e) => {
                self.line(depth, "temp");
                self.expr(e, depth + 1);
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
            ExprKind::Const { .. } => self.line(depth, &format!("const {ty} -")),
            ExprKind::Var(v)
                if matches!(
                    self.vars[v.0 as usize].kind,
                    VarKind::SelfParam | VarKind::RecordSelf
                ) =>
            {
                self.line(depth, &format!("self {ty}{folded}"));
            }
            ExprKind::Var(v) => {
                let name = self.var_name(*v);
                self.line(depth, &format!("var v{} {name}: {ty}{folded}", v.0));
            }
            ExprKind::Global(g) => {
                let name = self.program.global(*g).name.clone();
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
            ExprKind::IndexRange { first, last } => {
                // `docs/spec/objects.md` 15.6: the bounds written
                let mut line = String::from("range-index");
                if first.is_some() {
                    line.push_str(" first");
                }
                if last.is_some() {
                    line.push_str(" last");
                }
                self.line(depth, &line);
                for b in first.iter().chain(last.iter()) {
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
            ExprKind::Call {
                routine,
                args,
                dispatch,
            } => {
                let r = &self.program.routines[routine.0 as usize];
                let ret = match r.ret {
                    Some(t) => self.t(t),
                    None => "-".to_string(),
                };
                let name = r.name.clone();
                // `docs/spec/objects.md` 15.1
                let head = match dispatch {
                    // `docs/spec/objects.md` 15.6
                    Dispatch::Routine if r.operator => {
                        // Operator names are case-insensitive (11.2)
                        let last = name.rsplit('.').next().unwrap_or(&name);
                        let conv = super::operators::is_conversion(last);
                        let word = if conv { "convert-op" } else { "operator" };
                        format!("{word} {}", self.funcs.f(*routine))
                    }
                    Dispatch::Routine | Dispatch::Method if r.helper => {
                        format!("helper-call {}", self.funcs.f(*routine))
                    }
                    Dispatch::Routine => format!("call {}", self.funcs.f(*routine)),
                    // `docs/spec/objects.md` 15.5
                    Dispatch::Group(b) => {
                        format!("call {} block {}", self.funcs.f(*routine), block_name(*b))
                    }
                    Dispatch::Method => format!("method {}", self.funcs.f(*routine)),
                    Dispatch::Virtual(k) => format!("virtual {k}"),
                    Dispatch::Inherited => format!("inherited {}", self.funcs.f(*routine)),
                    Dispatch::VirtualClass(k) => format!("class virtual {k}"),
                    Dispatch::Intf(intf, k) => {
                        // `docs/spec/objects.md` 15.4: the interface the receiver's type names
                        let m = name.rsplit('.').next().unwrap_or(&name);
                        let line = format!(
                            "intfcall {}.{m} #{k}: {ret}{folded}",
                            self.t(Ty::Intf(*intf))
                        );
                        self.line(depth, &line);
                        for a in args {
                            self.call_arg(a, depth + 1);
                        }
                        return;
                    }
                };
                self.line(depth, &format!("{head} {name}: {ret}{folded}"));
                for a in args {
                    self.call_arg(a, depth + 1);
                }
            }
            ExprKind::Key { op, args, func } => {
                // `docs/spec/equality.md` 9: a call of the built-in generic routine
                let name = match op {
                    KeyOp::Equals => "KeyEquals",
                    KeyOp::Hash => "KeyHash",
                    KeyOp::Compare => "KeyCompare",
                };
                let arg = args.first().map_or_else(String::new, |a| self.t(a.ty));
                let via = func.map_or_else(String::new, |f| format!(" via {}", self.funcs.f(f)));
                self.line(depth, &format!("call {name}<{arg}>{via}: {ty}{folded}"));
                for a in args {
                    self.expr(a, depth + 1);
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
            ExprKind::ExcMember { base, member } => {
                self.line(depth, &format!("member {ty} {}", member.name()));
                self.expr(base, depth + 1);
            }
            ExprKind::ClassCheck { operand, class, op } => {
                // `docs/spec/objects.md` 15.1, 15.4: a class reference's conversion names its
                // type; the target is a class or an interface
                let target = if self.program.types.is_interface(*class) {
                    self.t(Ty::Intf(*class))
                } else {
                    self.t(Ty::Class(*class))
                };
                let line = match op {
                    ClassCheckOp::Is => format!("is {target}"),
                    ClassCheckOp::As => format!("as {target}"),
                    ClassCheckOp::Cast => format!("as {target} cast"),
                    ClassCheckOp::RefCast => format!("as {ty} cast"),
                };
                self.line(depth, &line);
                self.expr(operand, depth + 1);
            }
            ExprKind::ClassName { base } => {
                self.line(depth, &format!("builtin ClassName string{folded}"));
                self.expr(base, depth + 1);
            }
            ExprKind::ClassInfo { op, args } => {
                self.line(depth, &format!("builtin {} {ty}", op.name()));
                for a in args {
                    self.expr(a, depth + 1);
                }
            }
            ExprKind::ClassRefValue(class) => {
                self.line(depth, &format!("classref {}", self.t(Ty::Class(*class))));
            }
            ExprKind::TypeInfo(t) => {
                self.line(depth, &format!("typeinfo {}", self.t(*t)));
            }
            // `docs/spec/objects.md` 15.5
            ExprKind::RoutineRef(r) => {
                let name = self.program.routines[r.0 as usize].name.clone();
                self.line(depth, &format!("routine-ref {} {name}", self.funcs.f(*r)));
            }
            ExprKind::MethodRef { base, routine, how } => {
                let name = self.program.routines[routine.0 as usize].name.clone();
                let head = match how {
                    MethodRefHow::Method => format!("method-ref {} {name}", self.funcs.f(*routine)),
                    MethodRefHow::Virtual(k) => format!("method-ref virtual {k} {name}"),
                    MethodRefHow::Class => {
                        format!("class-method-ref {} {name}", self.funcs.f(*routine))
                    }
                    MethodRefHow::ClassVirtual(k) => {
                        format!("class-method-ref virtual {k} {name}")
                    }
                };
                self.line(depth, &head);
                self.expr(base, depth + 1);
            }
            ExprKind::ToClosure(x) => {
                self.line(depth, &format!("to-closure {ty}"));
                self.expr(x, depth + 1);
            }
            ExprKind::Anon { func, .. } => {
                self.line(depth, &format!("anon {}", self.funcs.f(*func)))
            }
            ExprKind::NestedRef { func, block } => {
                let name = self.program.routines[func.0 as usize].name.clone();
                let b = block_name(*block);
                self.line(
                    depth,
                    &format!("nested-ref {} {name} block {b}", self.funcs.f(*func)),
                );
            }
            ExprKind::CallValue { callee, args } => {
                let head = if matches!(callee.ty, Ty::Proc(_)) {
                    "call-indirect"
                } else {
                    "call-closure"
                };
                let ret = if e.ty.is_error() {
                    "-".to_string()
                } else {
                    ty.clone()
                };
                self.line(depth, &format!("{head}: {ret}"));
                self.expr(callee, depth + 1);
                for a in args {
                    self.call_arg(a, depth + 1);
                }
            }
            ExprKind::New {
                class,
                ctor,
                args,
                of,
                dispatch,
                ..
            } => {
                // `docs/spec/objects.md` 15.1; `T.Create` of a generic body has its constructor
                // chosen by the instantiation (`docs/spec/generics.md` 8)
                let f = match (ctor, dispatch) {
                    _ if matches!(class, Ty::Param(_)) => "-".to_string(),
                    (_, Dispatch::Virtual(k)) => format!("virtual {k}"),
                    (Some(r), _) => self.funcs.f(*r),
                    (None, _) => "builtin".to_string(),
                };
                let what = if of.is_some() { "create-of" } else { "create" };
                self.line(depth, &format!("{what} {} {f}", self.t(*class)));
                if let Some(c) = of {
                    self.expr(c, depth + 1);
                }
                for a in args {
                    self.call_arg(a, depth + 1);
                }
            }
            ExprKind::RecordNew { call } => {
                // `docs/spec/objects.md` 15.1: the constructor's arguments after `Self`
                if let ExprKind::Call { routine, args, .. } = &call.kind {
                    self.line(depth, &format!("create {ty} {}", self.funcs.f(*routine)));
                    for a in args.iter().skip(1) {
                        self.call_arg(a, depth + 1);
                    }
                }
            }
            ExprKind::InheritedBuiltin { base, name, msg } => {
                self.line(depth, &format!("inherited builtin {name}: -"));
                self.expr(base, depth + 1);
                if let Some(m) = msg {
                    self.expr(m, depth + 1);
                }
            }
            ExprKind::ObjField { base, slot, name } => {
                self.line(depth, &format!("objfield {ty} {name} #{slot}"));
                self.expr(base, depth + 1);
            }
            ExprKind::Weak(inner) => {
                self.line(depth, &format!("weak {ty}"));
                self.expr(inner, depth + 1);
            }
            ExprKind::Probe { operand, probe } => {
                let what = match probe {
                    Probe::Value(k) => format!("value #{k}"),
                    Probe::Reached(k) => format!("reached mark#{k}"),
                };
                self.line(depth, &format!("probe {what} {ty}"));
                self.expr(operand, depth + 1);
            }
            ExprKind::TryStrToInt { s, target } => {
                self.line(depth, &format!("builtin TryStrToInt {ty}"));
                self.expr(s, depth + 1);
                self.line(depth + 1, "out");
                self.place(target, depth + 2);
            }
            ExprKind::Supports {
                operand,
                intf,
                target,
                class_ref,
            } => {
                // `docs/spec/objects.md` 15.4
                let kind = if *class_ref {
                    "supports-class"
                } else {
                    "supports"
                };
                self.line(depth, &format!("{kind} {}", self.t(Ty::Intf(*intf))));
                self.expr(operand, depth + 1);
                if let Some(t) = target {
                    self.line(depth + 1, "out");
                    self.place(t, depth + 2);
                }
            }
            ExprKind::Error => self.line(depth, "error"),
            // Not reached: no member reached through a type name reads it
            ExprKind::TypeName => self.line(depth, &format!("type {ty}")),
        }
    }

    /// The loop variable line of `for` / `for-in` (`suffix` follows a global's name).
    fn for_var(&mut self, var: &ForVar, suffix: &str, depth: usize) {
        let text = match var {
            ForVar::Local(v) => format!("var v{} {}", v.0, self.var_name(*v)),
            ForVar::Global(id) => {
                let name = &self.program.global(*id).name;
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

/// The type of a member of an exception object kept outside its slots
/// (`docs/spec/exceptions.md` 2.1).
pub(crate) fn exc_member_type(m: ExcMember) -> Ty {
    match m {
        ExcMember::StackTrace => Ty::String,
        ExcMember::Suppressed => {
            Ty::DynArray(super::types::TypeId(crate::bytecode::EXCEPTION_ARRAY_TYPE))
        }
        ExcMember::Values => {
            Ty::DynArray(super::types::TypeId(crate::bytecode::ASSERTION_VALUES_TYPE))
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
        Calc::Callable => "callable".to_string(),
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
        Builtin::ExceptObject => "ExceptObject",
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
        Builtin::IsRefCountedObject => "IsRefCountedObject",
        Builtin::IsSameInstance => "SameInstance",
        Builtin::ObjectClassName => "ObjectClassName",
        Builtin::IsOverridden => "IsOverridden",
        Builtin::SingleToBits => "SingleToBits",
        Builtin::BitsToSingle => "BitsToSingle",
        Builtin::DoubleToBits => "DoubleToBits",
        Builtin::BitsToDouble => "BitsToDouble",
        Builtin::CharClass => "CharClass",
        Builtin::UnicodeCategory => "UnicodeCategory",
        Builtin::CodePointClass => "CodePointClass",
        Builtin::NumericValue => "NumericValue",
        Builtin::ClockNow => "ClockNow",
        Builtin::ClockMono => "ClockMono",
        Builtin::ClockControl => "ClockControl",
        Builtin::ClockSleep => "ClockSleep",
        Builtin::AddSuppressed => "AddSuppressed",
        Builtin::LocalOffset => "LocalOffset",
        Builtin::RaiseConvertError => "RaiseConvertError",
        Builtin::BytesCopy => "BytesCopy",
        Builtin::BytesFill => "BytesFill",
        Builtin::ConsoleRead => "ConsoleRead",
        Builtin::ConsoleWrite => "ConsoleWrite",
        Builtin::ConsoleWriteError => "ConsoleWriteError",
        Builtin::TypeInfo => "TypeInfo",
        Builtin::RttiTypeField => "RttiTypeField",
        Builtin::RttiMemberField => "RttiMemberField",
        Builtin::RttiName => "RttiName",
        Builtin::RttiFindType => "RttiFindType",
        Builtin::RttiTypeCount => "RttiTypeCount",
        Builtin::RttiFieldGet => "RttiFieldGet",
        Builtin::RttiFieldSet => "RttiFieldSet",
        Builtin::RttiClassType => "RttiClassType",
        Builtin::RttiClassOf => "RttiClassOf",
        Builtin::RttiMethodRef => "RttiMethodRef",
        Builtin::RttiPropRef => "RttiPropRef",
        Builtin::RttiBridge => "RttiBridge",
        Builtin::RttiAttrFunc => "RttiAttrFunc",
        Builtin::RttiArgSet => "RttiArgSet",
        Builtin::TypeInfoNumber => "TypeInfoNumber",
        Builtin::TypeInfoOfNumber => "TypeInfoOfNumber",
        Builtin::ValueWrap => "ValueWrap",
        Builtin::ValueIs => "ValueIs",
        Builtin::ValueCast => "ValueCast",
        Builtin::ValueMatch => "ValueMatch",
        Builtin::ValueToString => "ValueToString",
        Builtin::ValueRead => "ValueRead",
        Builtin::ValueReadDouble => "ValueReadDouble",
        Builtin::ValueReadCurrency => "ValueReadCurrency",
        Builtin::ValueReadString => "ValueReadString",
        Builtin::TypeKindOf => "TypeKindOf",
        Builtin::ValueArrayLength => "ValueArrayLength",
        Builtin::ValueArrayType => "ValueArrayType",
        Builtin::ValueArrayBits => "ValueArrayBits",
        Builtin::ValueArrayRef => "ValueArrayRef",
        Builtin::ValueArrayElemType => "ValueArrayElemType",
        Builtin::ValueArrayValue => "ValueArrayValue",
        Builtin::ValueArraySetValue => "ValueArraySetValue",
        Builtin::ValueArraySet => "ValueArraySet",
        Builtin::FormatValues => "Format",
        Builtin::HashFind => "HashFind",
        Builtin::HashLookup => "HashLookup",
        Builtin::IsPlainKey => "IsPlainKey",
        Builtin::IsPlainOrder => "IsPlainOrder",
        Builtin::Succ => "Succ",
        Builtin::Pred => "Pred",
        Builtin::Sqrt => "Sqrt",
        Builtin::Sin => "Sin",
        Builtin::Cos => "Cos",
        Builtin::ArcTan => "ArcTan",
        Builtin::Ln => "Ln",
        Builtin::Exp => "Exp",
        Builtin::Int => "Int",
        Builtin::Frac => "Frac",
        Builtin::MathFunc1 => "MathFunc1",
        Builtin::MathFunc2 => "MathFunc2",
        Builtin::HashCombine => "HashCombine",
        Builtin::SameInstance => "TObject.Equals",
        Builtin::InstanceHash => "TObject.GetHashCode",
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
        Builtin::Supports => "Supports",
        Builtin::AnsiUpperCase => "AnsiUpperCase",
        Builtin::AnsiLowerCase => "AnsiLowerCase",
        Builtin::AnsiSameText => "AnsiSameText",
        Builtin::AnsiCompareText => "AnsiCompareText",
        Builtin::Eof => "Eof",
        Builtin::Eoln => "Eoln",
        Builtin::ToString => "ToString",
    }
}

/// A constant value in the `hir` and `ir` dumps: `1`, `True`, `#65`, `"a"`, and a record or static
/// array constant as `(X: 1; Name: "a")` / `(1, 2, 3)` (`docs/spec/semantics.md` chapter 9).
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
/// multi-dimensional array written as nested arrays (`docs/spec/semantics.md` chapter 9).
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

/// ` weak` after the type of a `[weak]` field or variable (`docs/spec/objects.md` 15.1).
fn weak_mark(ty: Ty) -> &'static str {
    if matches!(ty, Ty::Weak(_)) {
        " weak"
    } else {
        ""
    }
}

/// The `.classvar`, `.method` and `.property` lines of a class or a record
/// (`docs/spec/objects.md` 15.1).
/// The methods and properties of an interface (`docs/spec/objects.md` 15.4): the methods by
/// number, the properties with the numbers of their methods.
fn intf_member_lines(
    types: &super::types::TypeTable,
    id: super::types::TypeId,
) -> Vec<(String, Option<TextRange>)> {
    let mut lines = Vec::new();
    let number = |r: super::hir::RoutineId| {
        types
            .intf_methods(id)
            .iter()
            .find(|(_, m)| m.routine == Some(r))
            .and_then(|(_, m)| m.vmt)
    };
    for m in types.class_methods(id) {
        let mut line = format!(".method #{} {}", m.vmt.unwrap_or(0), m.name);
        if !m.params.is_empty() {
            let ps: Vec<String> = m
                .params
                .iter()
                .map(|(t, mode)| format!("{}{}", mode_prefix(*mode), types.name(*t)))
                .collect();
            let _ = write!(line, "({})", ps.join(", "));
        }
        if let Some(r) = m.ret {
            let _ = write!(line, ": {}", types.name(r));
        }
        lines.push((line, Some(m.span)));
    }
    for p in types.class_properties(id) {
        let acc = |a: &Option<super::types::Accessor>| match a {
            Some(super::types::Accessor::Method(r)) => {
                number(*r).map_or("-".to_string(), |k| format!("#{k}"))
            }
            _ => "-".to_string(),
        };
        let mut line = format!(".property {}", p.name);
        if !p.params.is_empty() {
            let ps: Vec<String> = p.params.iter().map(|t| types.name(*t)).collect();
            let _ = write!(line, "[{}]", ps.join(", "));
        }
        let _ = write!(line, ": {}", types.name(p.ty));
        if let Some(n) = p.index {
            let _ = write!(line, " index={n}");
        }
        let _ = write!(line, " read={} write={}", acc(&p.read), acc(&p.write));
        if p.is_default {
            line.push_str(" default");
        }
        lines.push((line, Some(p.span)));
    }
    lines
}

/// The interface table of a class (`docs/spec/objects.md` 15.4): `.implements IShape #0=f3
/// #1=virtual 4`, `-` for an abstract method.
fn implements_lines(
    types: &super::types::TypeTable,
    funcs: &FuncNumbers,
    id: super::types::TypeId,
) -> Vec<(String, Option<TextRange>)> {
    let vmt = types.class_vmt(id);
    types
        .class_intfs(id)
        .iter()
        .map(|e| {
            let mut line = format!(".implements {}", types.name(Ty::Intf(e.intf)));
            for (k, x) in e.entries.iter().enumerate() {
                let f = match x {
                    // A generic type's methods have no function (`docs/spec/generics.md` 8)
                    super::types::IntfEntry::Func(_) if types.is_generic(id) => "-".to_string(),
                    super::types::IntfEntry::Func(r) => funcs.f(*r),
                    super::types::IntfEntry::Virtual(v)
                        if vmt.get(*v as usize) == Some(&super::types::VmtEntry::Abstract) =>
                    {
                        "-".to_string()
                    }
                    super::types::IntfEntry::Virtual(v) => format!("virtual {v}"),
                    super::types::IntfEntry::Missing => "-".to_string(),
                };
                let _ = write!(line, " #{k}={f}");
            }
            (line, None)
        })
        .collect()
}

/// The mode of a parameter as written before its type in an interface method's line.
fn mode_prefix(mode: crate::syntax::ast::ParamMode) -> &'static str {
    match mode {
        crate::syntax::ast::ParamMode::Value => "",
        crate::syntax::ast::ParamMode::Const => "const ",
        crate::syntax::ast::ParamMode::Var => "var ",
        crate::syntax::ast::ParamMode::Out => "out ",
    }
}

fn member_lines(
    types: &super::types::TypeTable,
    routines: &[Routine],
    funcs: &FuncNumbers,
    id: super::types::TypeId,
) -> Vec<(String, Option<TextRange>)> {
    let mut lines = Vec::new();
    // `docs/spec/objects.md` 15.1
    for st in types.class_statics(id) {
        if let super::types::StaticKind::Var(g, ty) = st.kind {
            lines.push((
                format!(
                    ".classvar g{} {}: {}{}",
                    g.0,
                    st.name,
                    types.name(ty),
                    weak_mark(ty)
                ),
                Some(st.span),
            ));
        }
    }
    for m in types.class_methods(id) {
        let f = match m.routine {
            // A generic type's methods have no function (`docs/spec/generics.md` 8)
            // nor have the generic methods
            Some(r)
                if !m.is_abstract && !types.is_generic(id) && !routines[r.0 as usize].generic =>
            {
                funcs.f(r)
            }
            _ => "-".to_string(),
        };
        let mut line = format!(".method {f} {}", m.name);
        match m.self_kind {
            // `docs/spec/objects.md` 15.6
            _ if m.kind == super::types::MethodKind::Operator => line.push_str(" operator"),
            super::types::SelfKind::Static => line.push_str(" static"),
            super::types::SelfKind::Class => line.push_str(" class"),
            super::types::SelfKind::Instance => {}
        }
        match m.vmt {
            Some(k) if m.kind == super::types::MethodKind::Constructor => {
                let _ = write!(line, " constructor virtual {k}");
            }
            _ if m.kind == super::types::MethodKind::Constructor => {
                line.push_str(" constructor");
            }
            Some(k) if m.kind == super::types::MethodKind::Destructor => {
                let _ = write!(line, " destructor {k}");
            }
            Some(k) if m.is_override => {
                let _ = write!(line, " override {k}");
            }
            Some(k) => {
                let _ = write!(line, " virtual {k}");
            }
            None => {}
        }
        lines.push((line, Some(m.span)));
    }
    for p in types.class_properties(id) {
        let acc = |a: &Option<super::types::Accessor>| match a {
            Some(super::types::Accessor::Field { slot, .. }) => format!("#{slot}"),
            Some(super::types::Accessor::ClassVar(g)) => format!("g{}", g.0),
            Some(super::types::Accessor::Method(r)) => funcs.f(*r),
            None => "-".to_string(),
        };
        let mut line = String::from(".property ");
        if p.is_class {
            line.push_str("class ");
        }
        line.push_str(&p.name);
        if !p.params.is_empty() {
            let ps: Vec<String> = p.params.iter().map(|t| types.name(*t)).collect();
            let _ = write!(line, "[{}]", ps.join(", "));
        }
        let _ = write!(line, ": {}", types.name(p.ty));
        if let Some(n) = p.index {
            let _ = write!(line, " index={n}");
        }
        let _ = write!(line, " read={} write={}", acc(&p.read), acc(&p.write));
        if p.is_default {
            line.push_str(" default");
        }
        lines.push((line, Some(p.span)));
    }
    lines
}

/// The variable holding a group block in the dump (`docs/spec/objects.md` 15.5): `vN`, or `link`
/// for the closure of the routine.
fn block_name(block: Option<VarId>) -> String {
    match block {
        Some(v) => format!("v{}", v.0),
        None => "link".into(),
    }
}
