//! Code generation (HIR → bytecode; `docs/spec/bytecode.md`).
//!
//! M1 keeps register allocation simple: a named variable lives in the register with the same
//! number as its `VarId` (parameters, `Result`, then locals; one register per variable), and
//! temporaries use the registers after them like a stack. Call arguments are laid out at the start of the temporaries.

use crate::bytecode::{self, ConstEntry, Function, GlobalEntry, HostEntry, Module, UnitEntry, op};
use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Stage};
use crate::directives::DirectiveOutput;
use crate::sema::hir::*;
use crate::sema::types::{Const, Ty};
use crate::source::{FileId, SourceFile, Span, TextRange};
use crate::syntax::ast::ParamMode;

/// Maximum number of registers (1.2).
const MAX_REGS: u32 = 65535;
/// Maximum length of the instruction stream (in words; 1.3).
const MAX_WORDS: usize = i32::MAX as usize;

/// Generates the module. Returns `None` on an error (E3400).
pub fn generate(
    p: &Program,
    path: &str,
    dir: &DirectiveOutput,
    src: &SourceFile,
    sink: &mut DiagnosticSink,
) -> Option<Module> {
    let mut mb = ModuleBuilder {
        dir,
        routine_index_offset: 1,
        consts: Vec::new(),
        hosts: Vec::new(),
        globals: p
            .globals
            .iter()
            .map(|g| GlobalEntry {
                name: g.name.clone(),
                kind: if g.ty == Ty::String { 1 } else { 0 },
            })
            .collect(),
        funcs: Vec::new(),
        program: p.name.clone(),
        src,
        sink,
        file: FileId(0),
        failed: false,
    };
    // Function table order: <main> is 0, <init> is 1 if present, then routines in declaration order
    let mut order: Vec<(&Routine, Option<RoutineId>)> = vec![(&p.main, None)];
    if let Some(init) = &p.init {
        order.push((init, None));
    }
    for (i, r) in p.routines.iter().enumerate() {
        order.push((r, Some(RoutineId(i as u32))));
    }
    // Routine indices are shifted by <main> and <init>
    let offset = if p.init.is_some() { 2 } else { 1 };
    mb.routine_index_offset = offset;
    let func_params: Vec<u16> = order.iter().map(|(r, _)| r.param_count as u16).collect();
    let _ = func_params;
    for (r, _) in &order {
        let f = FuncBuilder::new(&mut mb, r, p).generate();
        mb.funcs.push(f);
    }
    if mb.failed {
        return None;
    }
    let module = Module {
        consts: mb.consts,
        funcs: mb.funcs,
        globals: mb.globals,
        entry: 0,
        units: vec![UnitEntry {
            name: p.name.clone(),
            path: path.to_string(),
            init: p.init.as_ref().map(|_| 1),
            fini: None,
        }],
        hosts: mb.hosts,
    };
    Some(module)
}

struct ModuleBuilder<'a> {
    consts: Vec<ConstEntry>,
    hosts: Vec<HostEntry>,
    globals: Vec<GlobalEntry>,
    funcs: Vec<Function>,
    program: String,
    src: &'a SourceFile,
    sink: &'a mut DiagnosticSink,
    file: FileId,
    failed: bool,
    routine_index_offset: u32,
    dir: &'a DirectiveOutput,
}

impl ModuleBuilder<'_> {
    fn const_int(&mut self, v: i64) -> u32 {
        if let Some(i) = self.consts.iter().position(|c| *c == ConstEntry::Int(v)) {
            return i as u32;
        }
        self.consts.push(ConstEntry::Int(v));
        self.consts.len() as u32 - 1
    }

    fn const_str(&mut self, units: &[u16]) -> u32 {
        let entry = match String::from_utf16(units) {
            Ok(s) => ConstEntry::Str(s),
            Err(_) => ConstEntry::Utf16(units.to_vec()),
        };
        if let Some(i) = self.consts.iter().position(|c| *c == entry) {
            return i as u32;
        }
        self.consts.push(entry);
        self.consts.len() as u32 - 1
    }

    fn host(&mut self, name: &str, params: u16, ret: bool) -> u32 {
        if let Some(i) = self.hosts.iter().position(|h| h.name == name) {
            return i as u32;
        }
        self.hosts.push(HostEntry {
            name: name.to_string(),
            params,
            ret,
        });
        self.hosts.len() as u32 - 1
    }

    fn line_of(&self, span: TextRange) -> u32 {
        self.src.line_col(span.start()).line
    }
}

type Label = u32;

#[derive(Debug, Clone, Copy)]
enum Instr {
    Plain {
        op: u8,
        a: u32,
        b: u32,
        c: u32,
    },
    /// A jump (`b` is the second register of a compare-and-branch).
    Jump {
        op: u8,
        a: u32,
        b: u32,
        label: Label,
    },
}

struct FuncBuilder<'m, 'a> {
    mb: &'m mut ModuleBuilder<'a>,
    r: &'m Routine,
    program: &'m Program,
    instrs: Vec<(Instr, u32)>,
    labels: Vec<Option<usize>>,
    next_temp: u32,
    max_reg: u32,
    line: u32,
    loops: Vec<(Label, Label)>,
    too_many_regs: bool,
    /// `{$Q}` at the position of the current statement.
    current_overflow: bool,
}

impl<'m, 'a> FuncBuilder<'m, 'a> {
    fn new(mb: &'m mut ModuleBuilder<'a>, r: &'m Routine, program: &'m Program) -> Self {
        let nvars = r.vars.len() as u32;
        FuncBuilder {
            mb,
            r,
            program,
            instrs: Vec::new(),
            labels: Vec::new(),
            next_temp: nvars,
            max_reg: nvars.saturating_sub(1),
            line: 1,
            loops: Vec::new(),
            too_many_regs: false,
            current_overflow: false,
        }
    }

    // ---- Basics ----

    fn emit(&mut self, op: u8, a: u32, b: u32, c: u32) {
        self.instrs.push((Instr::Plain { op, a, b, c }, self.line));
    }

    fn emit_bx(&mut self, op: u8, a: u32, bx: u32) {
        self.emit(op, a, bx & 0xFF, bx >> 8);
    }

    fn emit_jump(&mut self, op: u8, a: u32, b: u32, label: Label) {
        self.instrs
            .push((Instr::Jump { op, a, b, label }, self.line));
    }

    fn new_label(&mut self) -> Label {
        self.labels.push(None);
        self.labels.len() as Label - 1
    }

    fn bind(&mut self, label: Label) {
        self.labels[label as usize] = Some(self.instrs.len());
    }

    fn alloc(&mut self) -> u32 {
        let r = self.next_temp;
        self.next_temp += 1;
        if r > self.max_reg {
            self.max_reg = r;
        }
        if r > MAX_REGS {
            self.too_many_regs = true;
        }
        r
    }

    fn mark(&self) -> u32 {
        self.next_temp
    }

    fn release(&mut self, mark: u32) {
        self.next_temp = mark;
    }

    fn set_line(&mut self, span: TextRange) -> u32 {
        let old = self.line;
        self.line = self.mb.line_of(span);
        old
    }

    fn reg_of(&self, v: VarId) -> u32 {
        v.0
    }

    fn is_ref_param(&self, v: VarId) -> bool {
        matches!(
            self.r.vars[v.0 as usize].kind,
            VarKind::Param(ParamMode::Var | ParamMode::Out)
        )
    }

    fn load_int(&mut self, dst: u32, v: i64) {
        if v == 0 {
            self.emit(op::LOAD_ZERO, dst, 0, 0);
        } else if (i16::MIN as i64..=i16::MAX as i64).contains(&v) {
            self.emit(
                op::LOAD_INT,
                dst,
                (v as u16 & 0xFF) as u32,
                ((v as u16) >> 8) as u32,
            );
        } else {
            let k = self.mb.const_int(v);
            self.emit_bx(op::LOAD_CONST, dst, k);
        }
    }

    fn load_str(&mut self, dst: u32, units: &[u16]) {
        if units.is_empty() {
            self.emit(op::LOAD_ZERO, dst, 0, 0);
        } else {
            let k = self.mb.const_str(units);
            self.emit_bx(op::LOAD_CONST, dst, k);
        }
    }

    fn load_const(&mut self, dst: u32, c: &Const, ty: Ty) {
        match c {
            Const::Int(v) => {
                let v = if ty == Ty::UInt64 {
                    *v as u64 as i64
                } else {
                    *v as i64
                };
                self.load_int(dst, v);
            }
            Const::Bool(b) => self.load_int(dst, *b as i64),
            Const::Char(ch) => self.load_int(dst, *ch as i64),
            Const::Str(s) => self.load_str(dst, s),
        }
    }

    // ---- Generation entry points ----

    fn generate(mut self) -> Function {
        let body: &[Stmt] = &self.r.body;
        for s in body {
            self.stmt(s);
        }
        // Final Ret / RetVoid
        self.line = self.mb.line_of(self.r.end_span);
        self.emit_return();
        let full_name = format!("{}.{}", self.mb.program, self.r.name);
        let regs = self.max_reg + 1;
        if self.too_many_regs || regs > MAX_REGS + 1 {
            self.mb.failed = true;
            let d = Diagnostic::error(
                Code::E(3400),
                format!(
                    "routine `{}` is too large: it needs more than 65535 registers",
                    self.r.name
                ),
                Stage::Codegen,
            )
            .with_primary(Span::new(self.mb.file, self.r.name_span), "")
            .with_note("split the routine into smaller ones");
            self.mb.sink.push(d);
        }
        let (code, lines) = self.layout();
        if code.len() > MAX_WORDS {
            self.mb.failed = true;
            let d = Diagnostic::error(
                Code::E(3400),
                format!(
                    "routine `{}` is too large: its code exceeds 2147483647 words",
                    self.r.name
                ),
                Stage::Codegen,
            )
            .with_primary(Span::new(self.mb.file, self.r.name_span), "")
            .with_note("split the routine into smaller ones");
            self.mb.sink.push(d);
        }
        let locals: Vec<(u16, String)> = self
            .r
            .vars
            .iter()
            .enumerate()
            .filter(|(_, v)| v.kind != VarKind::Hidden)
            .map(|(i, v)| (i as u16, v.name.clone()))
            .collect();
        Function {
            name: full_name,
            params: self.r.param_count as u16,
            regs: regs.min(MAX_REGS + 1) as u16,
            code,
            lines,
            locals,
            file: 0,
        }
    }

    fn emit_return(&mut self) {
        match self.r.result_var() {
            Some(rv) => self.emit(op::RET, self.reg_of(rv), 0, 0),
            None => self.emit(op::RET_VOID, 0, 0, 0),
        }
    }

    // ---- Layout and encoding ----

    /// Decides the size of each jump and turns the instructions into words.
    fn layout(&self) -> (Vec<u32>, Vec<(u32, u32)>) {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Form {
            Short,
            Wide,
            /// The compare-and-branch cannot reach: skip over a `Jmp` with the inverted condition (inverted branch + Jmp)
            Expanded,
        }
        let n = self.instrs.len();
        let mut forms: Vec<Form> = self
            .instrs
            .iter()
            .map(|(i, _)| {
                if plain_needs_wide(i) {
                    Form::Wide
                } else {
                    Form::Short
                }
            })
            .collect();
        let size = |f: Form, i: &Instr| -> usize {
            match (f, i) {
                (Form::Short, _) => 1,
                (Form::Wide, _) => 3,
                (Form::Expanded, _) => 6,
            }
        };
        let mut positions = vec![0usize; n + 1];
        loop {
            let mut changed = false;
            let mut pos = 0usize;
            for (i, (ins, _)) in self.instrs.iter().enumerate() {
                positions[i] = pos;
                pos += size(forms[i], ins);
            }
            positions[n] = pos;
            for (i, (ins, _)) in self.instrs.iter().enumerate() {
                let Instr::Jump { op, a, b, label } = ins else {
                    continue;
                };
                let target = positions[self.labels[*label as usize].expect("unbound label")];
                let fused = op::info(*op).is_some_and(|i| i.shape == op::Shape::ABsC);
                let regs_wide = *a > 255 || *b > 255;
                let needed = match forms[i] {
                    Form::Expanded => Form::Expanded,
                    _ => {
                        let next = positions[i] + 1;
                        let next_wide = positions[i] + 3;
                        let off_short = target as i64 - next as i64;
                        let off_wide = target as i64 - next_wide as i64;
                        if fused {
                            if !regs_wide && (i8::MIN as i64..=i8::MAX as i64).contains(&off_short)
                            {
                                Form::Short
                            } else if (i16::MIN as i64..=i16::MAX as i64).contains(&off_wide) {
                                Form::Wide
                            } else {
                                Form::Expanded
                            }
                        } else if !regs_wide
                            && (i16::MIN as i64..=i16::MAX as i64).contains(&off_short)
                        {
                            Form::Short
                        } else {
                            Form::Wide
                        }
                    }
                };
                // Only ever grow (so that it converges)
                let rank = |f: Form| match f {
                    Form::Short => 0,
                    Form::Wide => 1,
                    Form::Expanded => 2,
                };
                if rank(needed) > rank(forms[i]) {
                    forms[i] = needed;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // Encoding
        let mut code: Vec<u32> = Vec::with_capacity(positions[n]);
        let mut lines: Vec<(u32, u32)> = Vec::new();
        for (i, (ins, line)) in self.instrs.iter().enumerate() {
            if lines.last().is_none_or(|l| l.1 != *line) {
                lines.push((code.len() as u32, *line));
            }
            match ins {
                Instr::Plain { op, a, b, c } => {
                    if forms[i] == Form::Wide {
                        code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                        code.push(bytecode::encode(*op, *a, *b, *c));
                        // A table index continues with the upper 16 bits; a signed immediate is sign-extended from its original width.
                        let (ext_b, ext_c) = match op::info(*op).unwrap().shape {
                            op::Shape::ABx => {
                                let bx = b | (c << 8);
                                (bx >> 8, bx >> 16)
                            }
                            op::Shape::ASBxImm => {
                                let bx = (b | (c << 8)) as u16 as i16 as i32 as u32;
                                (bx >> 8, bx >> 16)
                            }
                            op::Shape::ABsCImm => (*b, *c as u8 as i8 as i16 as u16 as u32),
                            _ => (*b, *c),
                        };
                        code.push(bytecode::encode_ext(*a, ext_b, ext_c));
                    } else {
                        code.push(bytecode::encode(*op, *a, *b, *c));
                    }
                }
                Instr::Jump { op, a, b, label } => {
                    let target = positions[self.labels[*label as usize].unwrap()] as i64;
                    let fused = op::info(*op).is_some_and(|i| i.shape == op::Shape::ABsC);
                    match forms[i] {
                        Form::Short => {
                            let next = code.len() as i64 + 1;
                            let off = target - next;
                            if fused {
                                code.push(bytecode::encode(*op, *a, *b, (off as i8 as u8) as u32));
                            } else {
                                let o = off as i16 as u16 as u32;
                                code.push(bytecode::encode(*op, *a, o & 0xFF, o >> 8));
                            }
                        }
                        Form::Wide => {
                            let next = code.len() as i64 + 3;
                            let off = target - next;
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            if fused {
                                let o = off as i16 as u16 as u32;
                                code.push(bytecode::encode(*op, *a, *b, o & 0xFF));
                                code.push(bytecode::encode_ext(*a, *b, o));
                            } else {
                                let o = off as i32 as u32;
                                code.push(bytecode::encode(*op, *a, o & 0xFF, (o >> 8) & 0xFF));
                                code.push(bytecode::encode_ext(
                                    *a,
                                    (o >> 8) & 0xFFFF,
                                    (o >> 16) & 0xFFFF,
                                ));
                            }
                        }
                        Form::Expanded => {
                            // Skip over a 3-word (Wide) Jmp with the inverted condition; the inverted branch is also Wide, 3 words
                            let (inv, ia, ib) = invert_fused(*op, *a, *b);
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            let o = 3u32; // skip the next Jmp (3 words)
                            code.push(bytecode::encode(inv, ia, ib, o & 0xFF));
                            code.push(bytecode::encode_ext(ia, ib, o));
                            let next = code.len() as i64 + 3;
                            let off = (target - next) as i32 as u32;
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            code.push(bytecode::encode(op::JMP, 0, off & 0xFF, (off >> 8) & 0xFF));
                            code.push(bytecode::encode_ext(
                                0,
                                (off >> 8) & 0xFFFF,
                                (off >> 16) & 0xFFFF,
                            ));
                        }
                    }
                }
            }
        }
        if lines.is_empty() {
            lines.push((0, self.line));
        }
        (code, lines)
    }

    // ---- Statements ----

    fn stmt(&mut self, s: &Stmt) {
        let old = self.set_line(s.span);
        let mark = self.mark();
        let saved_overflow = self.current_overflow;
        self.current_overflow = self.mb.dir.switches.at(s.span.start()).overflow_checks;
        match &s.kind {
            StmtKind::Assign { target, value, .. } => self.assign(target, value),
            StmtKind::Block(list) => {
                for x in list {
                    self.stmt(x);
                }
            }
            StmtKind::Expr(e) => {
                let t = self.alloc();
                self.expr(e, t);
            }
            StmtKind::If {
                branches,
                else_body,
            } => {
                let end = self.new_label();
                for (cond, body) in branches {
                    let next = self.new_label();
                    let m = self.mark();
                    self.cond(cond, next, false);
                    self.release(m);
                    self.stmt(body);
                    self.emit_jump(op::JMP, 0, 0, end);
                    self.bind(next);
                }
                if let Some(e) = else_body {
                    self.stmt(e);
                }
                self.bind(end);
            }
            StmtKind::While { cond, body } => {
                let top = self.new_label();
                let end = self.new_label();
                self.bind(top);
                let m = self.mark();
                self.cond(cond, end, false);
                self.release(m);
                self.loops.push((end, top));
                self.stmt(body);
                self.loops.pop();
                self.emit_jump(op::JMP, 0, 0, top);
                self.bind(end);
            }
            StmtKind::Repeat { body, cond } => {
                let top = self.new_label();
                let cont = self.new_label();
                let end = self.new_label();
                self.bind(top);
                self.loops.push((end, cont));
                for x in body {
                    self.stmt(x);
                }
                self.loops.pop();
                self.bind(cont);
                let old = self.set_line(cond.span);
                self.cond(cond, top, false);
                self.line = old;
                self.bind(end);
            }
            StmtKind::For {
                var,
                from,
                to,
                downto,
                body,
                ..
            } => self.for_stmt(var, from, to, *downto, body),
            StmtKind::Break => {
                if let Some(&(end, _)) = self.loops.last() {
                    self.emit_jump(op::JMP, 0, 0, end);
                }
            }
            StmtKind::Continue => {
                if let Some(&(_, cont)) = self.loops.last() {
                    self.emit_jump(op::JMP, 0, 0, cont);
                }
            }
            StmtKind::Exit(value) => {
                if let Some(v) = value
                    && let Some(rv) = self.r.result_var()
                {
                    let dst = self.reg_of(rv);
                    self.expr_into_var(v, dst);
                }
                self.emit_return();
            }
            StmtKind::Halt(code) => {
                let t = self.alloc();
                match code {
                    Some(c) => self.expr(c, t),
                    None => self.emit(op::LOAD_ZERO, t, 0, 0),
                }
                self.emit(op::HALT, t, 0, 0);
            }
            StmtKind::IncDec {
                target,
                target_ty,
                delta,
                negative,
                calc,
                checks,
            } => self.inc_dec(
                target,
                *target_ty,
                delta.as_ref(),
                *negative,
                *calc,
                *checks,
            ),
            StmtKind::ReadLn(place) => {
                let a = self.alloc();
                let h = self.mb.host("Console.ReadLine", 0, true);
                self.emit_bx(op::CALL_HOST, a, h);
                if let Some(p) = place {
                    let ty = self.place_type(p);
                    let v = if ty == Ty::String {
                        a
                    } else {
                        let t = self.alloc();
                        let w = ty.width_code().unwrap_or(6) as u32;
                        self.emit(op::STR_TO_INT_IO, t, a, w);
                        t
                    };
                    self.store_place(p, v);
                }
            }
            StmtKind::Write { args, newline } => self.write(args, *newline),
            StmtKind::Assert {
                cond,
                message,
                text,
            } => {
                let skip = self.new_label();
                let m = self.mark();
                self.cond(cond, skip, true);
                self.release(m);
                let msg = self.alloc();
                let head: Vec<u16> = format!("assertion failed: {text}").encode_utf16().collect();
                self.load_str(msg, &head);
                if let Some(me) = message {
                    let t = self.alloc();
                    self.load_str(t, &" (".encode_utf16().collect::<Vec<_>>());
                    self.emit(op::CONCAT_STR, msg, msg, t);
                    self.expr(me, t);
                    self.emit(op::CONCAT_STR, msg, msg, t);
                    self.load_str(t, &")".encode_utf16().collect::<Vec<_>>());
                    self.emit(op::CONCAT_STR, msg, msg, t);
                }
                self.emit(op::RAISE_ERROR, msg, 0, 0);
                self.bind(skip);
            }
            StmtKind::Nop => {}
        }
        self.current_overflow = saved_overflow;
        self.release(mark);
        self.line = old;
    }

    fn place_type(&self, p: &Place) -> Ty {
        match p {
            Place::Var(v, _) => self.r.vars[v.0 as usize].ty,
            Place::Global(g) => self.program.globals[g.0 as usize].ty,
            Place::StrIndex { .. } => Ty::Char,
        }
    }

    /// Whether the variable's register may be written directly (not for reference parameters).
    fn direct_var_reg(&self, p: &Place) -> Option<u32> {
        match p {
            Place::Var(v, _) if !self.is_ref_param(*v) => Some(self.reg_of(*v)),
            _ => None,
        }
    }

    fn assign(&mut self, target: &Place, value: &Expr) {
        match target {
            Place::Var(..) | Place::Global(_) => {
                if let Some(reg) = self.direct_var_reg(target) {
                    self.expr_into_var(value, reg);
                } else {
                    let t = self.alloc();
                    self.expr(value, t);
                    self.store_place(target, t);
                }
            }
            Place::StrIndex { base, index, span } => {
                // Index, right-hand side, then the target (`semantics.md` 3.1; left to right)
                let i = self.alloc();
                self.expr(index, i);
                let v = self.alloc();
                self.expr(value, v);
                let old = self.set_line(*span);
                if let Some(reg) = self.direct_var_reg(base) {
                    self.emit(op::STR_SET_INDEX, reg, i, v);
                } else {
                    let t = self.alloc();
                    self.load_place(base, t);
                    self.emit(op::STR_SET_INDEX, t, i, v);
                    self.store_place(base, t);
                }
                self.line = old;
            }
        }
    }

    /// Puts an expression into a variable's register (through a temporary if the expression reads that variable).
    fn expr_into_var(&mut self, e: &Expr, reg: u32) {
        if expr_reads_reg(e, reg) {
            let t = self.alloc();
            self.expr(e, t);
            self.emit(op::MOVE, reg, t, 0);
        } else {
            self.expr(e, reg);
        }
    }

    /// Reads the value of a place into `dst` (`Move` for a variable).
    fn load_place(&mut self, p: &Place, dst: u32) {
        match p {
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                if self.is_ref_param(*v) {
                    self.emit(op::LOAD_REF, dst, r, 0);
                } else if r != dst {
                    self.emit(op::MOVE, dst, r, 0);
                }
            }
            Place::Global(g) => self.emit_bx(op::LOAD_GLOBAL, dst, g.0),
            Place::StrIndex { base, index, .. } => {
                let b = self.alloc();
                self.load_place(base, b);
                let i = self.alloc();
                self.expr(index, i);
                self.emit(op::STR_INDEX, dst, b, i);
            }
        }
    }

    /// Writes the value of `src` to a place.
    fn store_place(&mut self, p: &Place, src: u32) {
        match p {
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                if self.is_ref_param(*v) {
                    self.emit(op::STORE_REF, r, src, 0);
                } else if r != src {
                    self.emit(op::MOVE, r, src, 0);
                }
            }
            Place::Global(g) => self.emit_bx(op::STORE_GLOBAL, src, g.0),
            Place::StrIndex { .. } => unreachable!("string element is not stored this way"),
        }
    }

    fn for_stmt(&mut self, var: &ForVar, from: &Expr, to: &Expr, downto: bool, body: &Stmt) {
        let (counter, var_ty, global): (u32, Ty, Option<GlobalId>) = match var {
            ForVar::Local(v) => (self.reg_of(*v), self.r.vars[v.0 as usize].ty, None),
            ForVar::Global { id, counter } => (
                self.reg_of(*counter),
                self.program.globals[id.0 as usize].ty,
                Some(*id),
            ),
        };
        self.expr_into_var(from, counter);
        let limit = self.alloc();
        self.expr(to, limit);
        let unsigned = var_ty == Ty::UInt64;
        let end = self.new_label();
        let cont = self.new_label();
        let top = self.new_label();
        // Skip if the range is empty: limit < counter for `to`, counter < limit for `downto`
        let lt = if unsigned {
            op::JMP_IF_LT_UINT
        } else {
            op::JMP_IF_LT_INT
        };
        if downto {
            self.emit_jump(lt, counter, limit, end);
        } else {
            self.emit_jump(lt, limit, counter, end);
        }
        // When a global variable is the loop variable, write that iteration's value on each entry to the body,
        // and on exit (`Break` and the termination test) write back the value of the last entry
        // (even if a routine called from the body changes it, the value after the loop is the value of the last entry;
        // semantics.md 4.5). If the body is never entered, the original value is kept
        let brk = if global.is_some() {
            self.new_label()
        } else {
            end
        };
        self.bind(top);
        if let Some(g) = global {
            self.emit_bx(op::STORE_GLOBAL, counter, g.0);
        }
        self.loops.push((brk, cont));
        self.stmt(body);
        self.loops.pop();
        self.bind(cont);
        self.emit_jump(op::JMP_IF_EQ_INT, counter, limit, brk);
        let delta: u32 = if downto { 0xFF } else { 1 };
        self.emit(op::ADD_INT_I, counter, counter, delta);
        self.emit_jump(op::JMP, 0, 0, top);
        if let Some(g) = global {
            self.bind(brk);
            self.emit_bx(op::STORE_GLOBAL, counter, g.0);
        }
        self.bind(end);
    }

    fn inc_dec(
        &mut self,
        target: &Place,
        target_ty: Ty,
        delta: Option<&Expr>,
        negative: bool,
        calc: Ty,
        checks: Checks,
    ) {
        let direct = self.direct_var_reg(target);
        let t = match direct {
            Some(r) => r,
            None => {
                let t = self.alloc();
                self.load_place(target, t);
                t
            }
        };
        let is64 = matches!(calc, Ty::Int64 | Ty::UInt64);
        let small_const = match delta {
            None => Some(1i64),
            Some(d) => match &d.value {
                Some(Const::Int(v)) if (-128..=127).contains(v) => Some(*v as i64),
                _ => None,
            },
        };
        match small_const {
            Some(v) if !(is64 && checks.overflow) => {
                let v = if negative { -v } else { v };
                if (i8::MIN as i64..=i8::MAX as i64).contains(&v) {
                    self.emit(op::ADD_INT_I, t, t, (v as i8 as u8) as u32);
                } else {
                    let d = self.alloc();
                    self.load_int(d, v);
                    self.emit(op::ADD_INT, t, t, d);
                }
            }
            _ => {
                let d = self.alloc();
                match delta {
                    Some(e) => self.expr(e, d),
                    None => self.load_int(d, 1),
                }
                let opc = match (calc, checks.overflow, negative) {
                    (Ty::Int64, true, false) => op::ADD_INT_CHK,
                    (Ty::Int64, true, true) => op::SUB_INT_CHK,
                    (Ty::UInt64, true, false) => op::ADD_UINT_CHK,
                    (Ty::UInt64, true, true) => op::SUB_UINT_CHK,
                    (_, _, false) => op::ADD_INT,
                    (_, _, true) => op::SUB_INT,
                };
                self.emit(opc, t, t, d);
            }
        }
        self.narrow_arith(t, calc, checks.overflow);
        // From the computation type to the target type (`{$R}` of assignment)
        let dest_ty = if target_ty == Ty::Char {
            Ty::Char
        } else {
            target_ty
        };
        self.convert_int(t, t, calc, dest_ty, checks.range);
        if direct.is_none() {
            self.store_place(target, t);
        }
    }

    fn write(&mut self, args: &[WriteArg], newline: bool) {
        // Consecutive constant strings (without width) are concatenated at compile time
        let a = self.alloc();
        let mut started = false;
        let mut pending: Vec<u16> = Vec::new();
        let flush = |this: &mut Self, pending: &mut Vec<u16>, started: &mut bool| {
            if pending.is_empty() {
                return;
            }
            if *started {
                let t = this.alloc();
                this.load_str(t, pending);
                this.emit(op::CONCAT_STR, a, a, t);
            } else {
                this.load_str(a, pending);
                *started = true;
            }
            pending.clear();
        };
        for arg in args {
            if arg.width.is_none()
                && let Some(c) = &arg.value.value
                && let Some(units) = const_as_string(c, arg.value.ty)
            {
                pending.extend(units);
                continue;
            }
            flush(self, &mut pending, &mut started);
            let s = if started { self.alloc() } else { a };
            let old = self.set_line(arg.value.span);
            self.string_value(&arg.value, s);
            if let Some(w) = &arg.width {
                let wr = self.alloc();
                self.expr(w, wr);
                self.emit(op::STR_PAD_LEFT, s, s, wr);
            }
            self.line = old;
            if started {
                self.emit(op::CONCAT_STR, a, a, s);
            }
            started = true;
        }
        if newline {
            pending.push(10);
        }
        flush(self, &mut pending, &mut started);
        let h = self.mb.host("Console.Write", 1, false);
        self.emit_bx(op::CALL_HOST, a, h);
    }

    /// Converts a `Write` argument to a string and puts it in `dst`.
    fn string_value(&mut self, e: &Expr, dst: u32) {
        self.expr(e, dst);
        match e.ty {
            Ty::String => {}
            Ty::Char => self.emit(op::STR_FROM_CHAR, dst, dst, 0),
            Ty::Boolean => self.emit(op::BOOL_TO_STR, dst, dst, 0),
            Ty::UInt64 => self.emit(op::UINT_TO_STR, dst, dst, 0),
            _ => self.emit(op::INT_TO_STR, dst, dst, 0),
        }
    }

    // ---- Integer helpers ----

    /// Narrows the result when the computation type is narrow (`{$Q}`).
    fn narrow_arith(&mut self, reg: u32, calc: Ty, overflow: bool) {
        if matches!(calc, Ty::Int64 | Ty::UInt64) {
            return;
        }
        let w = calc.width_code().unwrap_or(4) as u32;
        let opc = if overflow { op::NARROW_OVF } else { op::NARROW };
        self.emit(opc, reg, reg, w);
    }

    /// Integer type conversion (`{$R}` of assignment compatibility). Nothing to do for a wider type.
    fn convert_int(&mut self, dst: u32, src: u32, from: Ty, to: Ty, range: bool) {
        if from == to {
            if dst != src {
                self.emit(op::MOVE, dst, src, 0);
            }
            return;
        }
        let (flo, fhi) = from.range().unwrap_or((i64::MIN as i128, i64::MAX as i128));
        let (tlo, thi) = to.range().unwrap_or((i64::MIN as i128, i64::MAX as i128));
        if tlo <= flo && fhi <= thi {
            if dst != src {
                self.emit(op::MOVE, dst, src, 0);
            }
            return;
        }
        let w = to.width_code().unwrap_or(6) as u32;
        let opc = if range { op::NARROW_RNG } else { op::NARROW };
        self.emit(opc, dst, src, w);
    }

    // ---- Expressions ----

    /// Puts the value of an expression into `dst`.
    fn expr(&mut self, e: &Expr, dst: u32) {
        let old = self.set_line(e.span);
        let mark = self.mark();
        if let Some(c) = &e.value
            && !matches!(e.kind, ExprKind::Call { .. } | ExprKind::Builtin { .. })
        {
            self.load_const(dst, c, e.ty);
            self.release(mark);
            self.line = old;
            return;
        }
        match &e.kind {
            ExprKind::Const => {
                // value has already been handled above
                self.emit(op::LOAD_ZERO, dst, 0, 0);
            }
            ExprKind::Var(v) => self.load_place(&Place::Var(*v, e.span), dst),
            ExprKind::Global(g) => self.emit_bx(op::LOAD_GLOBAL, dst, g.0),
            ExprKind::Binary { first, steps } => self.binary(first, steps, dst),
            ExprKind::Unary {
                op: uop,
                operand,
                calc,
            } => {
                self.expr(operand, dst);
                match (uop, calc) {
                    (UnOp::Not, Calc::Bool) => self.emit(op::NOT_BOOL, dst, dst, 0),
                    (UnOp::Not, Calc::Int(t)) => {
                        self.emit(op::NOT_INT, dst, dst, 0);
                        if !matches!(t, Ty::Int64 | Ty::UInt64) {
                            let w = t.width_code().unwrap() as u32;
                            self.emit(op::NARROW, dst, dst, w);
                        }
                    }
                    (UnOp::Neg, Calc::Int(t)) => {
                        let checks = self.checks_here(e.span);
                        if *t == Ty::Int64 && checks.overflow {
                            self.emit(op::NEG_INT_CHK, dst, dst, 0);
                        } else {
                            self.emit(op::NEG_INT, dst, dst, 0);
                            self.narrow_arith(dst, *t, checks.overflow);
                        }
                    }
                    _ => {}
                }
            }
            ExprKind::StrIndex { base, index } => {
                let b = self.expr_any(base);
                let i = self.expr_any(index);
                self.emit(op::STR_INDEX, dst, b, i);
            }
            ExprKind::Call { routine, args } => self.call(*routine, args, dst),
            ExprKind::Builtin { func, args } => self.builtin(*func, args, dst, e.span),
            ExprKind::Convert { operand, kind } => {
                self.expr(operand, dst);
                match kind {
                    Conversion::Int { to, check } => {
                        self.convert_int(dst, dst, operand.ty, *to, *check)
                    }
                    Conversion::CharToStr => self.emit(op::STR_FROM_CHAR, dst, dst, 0),
                    Conversion::IntToChar { check } => {
                        let opc = if *check { op::NARROW_RNG } else { op::NARROW };
                        self.emit(opc, dst, dst, 3);
                    }
                    Conversion::IntToBool => {
                        let z = self.alloc();
                        self.emit(op::LOAD_ZERO, z, 0, 0);
                        self.emit(op::EQ_INT, dst, dst, z);
                        self.emit(op::NOT_BOOL, dst, dst, 0);
                    }
                    Conversion::OrdToInt => {}
                }
            }
            ExprKind::Error => self.emit(op::LOAD_ZERO, dst, 0, 0),
        }
        self.release(mark);
        self.line = old;
    }

    /// A register holding the expression's value (the variable's register for a variable, otherwise a temporary).
    fn expr_any(&mut self, e: &Expr) -> u32 {
        if e.value.is_none()
            && let ExprKind::Var(v) = &e.kind
            && !self.is_ref_param(*v)
        {
            return self.reg_of(*v);
        }
        let t = self.alloc();
        self.expr(e, t);
        t
    }

    fn checks_here(&self, _span: TextRange) -> Checks {
        // Expression switches are those in effect at the start of the statement; unary operations follow the statement context
        Checks {
            overflow: self.current_overflow,
            range: false,
        }
    }

    /// A left fold.
    fn binary(&mut self, first: &Expr, steps: &[BinStep], dst: u32) {
        // If a right operand reads the dst variable, accumulate in a temporary
        let acc = if steps.iter().any(|s| expr_reads_reg(&s.rhs, dst)) {
            self.alloc()
        } else {
            dst
        };
        self.expr(first, acc);
        let mut cur_ty = first.ty;
        for step in steps {
            let old = self.set_line(step.span);
            match step.calc {
                Calc::Bool => match step.op {
                    BinOp::And | BinOp::Or => {
                        let skip = self.new_label();
                        let jop = if step.op == BinOp::And {
                            op::JMP_IF_FALSE
                        } else {
                            op::JMP_IF_TRUE
                        };
                        self.emit_jump(jop, acc, 0, skip);
                        self.expr(&step.rhs, acc);
                        self.bind(skip);
                    }
                    BinOp::Xor => {
                        let r = self.expr_any(&step.rhs);
                        self.emit(op::XOR_INT, acc, acc, r);
                    }
                    _ => {
                        let r = self.expr_any(&step.rhs);
                        self.compare_int(step.op, acc, acc, r, false);
                    }
                },
                Calc::Char => {
                    let r = self.expr_any(&step.rhs);
                    self.compare_int(step.op, acc, acc, r, false);
                }
                Calc::Str => {
                    let r = self.expr_any(&step.rhs);
                    match step.op {
                        BinOp::Add => self.emit(op::CONCAT_STR, acc, acc, r),
                        BinOp::Eq => self.emit(op::EQ_STR, acc, acc, r),
                        BinOp::Ne => {
                            self.emit(op::EQ_STR, acc, acc, r);
                            self.emit(op::NOT_BOOL, acc, acc, 0);
                        }
                        _ => {
                            self.emit(op::CMP_STR, acc, acc, r);
                            let z = self.alloc();
                            self.emit(op::LOAD_ZERO, z, 0, 0);
                            self.compare_int(step.op, acc, acc, z, false);
                        }
                    }
                }
                Calc::Int(calc) => {
                    let r = self.expr_any(&step.rhs);
                    let unsigned = calc == Ty::UInt64;
                    if step.op.is_comparison() {
                        self.compare_int(step.op, acc, acc, r, unsigned);
                    } else {
                        let checks = self.checks_here(step.span);
                        self.int_arith(step.op, calc, checks.overflow, acc, acc, r);
                    }
                }
            }
            cur_ty = step.ty;
            self.line = old;
        }
        let _ = cur_ty;
        if acc != dst {
            self.emit(op::MOVE, dst, acc, 0);
        }
    }

    /// Puts the value of a comparison (`Boolean`) into `dst`.
    fn compare_int(&mut self, bop: BinOp, dst: u32, a: u32, b: u32, unsigned: bool) {
        let (lt, le) = if unsigned {
            (op::LT_UINT, op::LE_UINT)
        } else {
            (op::LT_INT, op::LE_INT)
        };
        match bop {
            BinOp::Eq => self.emit(op::EQ_INT, dst, a, b),
            BinOp::Ne => {
                self.emit(op::EQ_INT, dst, a, b);
                self.emit(op::NOT_BOOL, dst, dst, 0);
            }
            BinOp::Lt => self.emit(lt, dst, a, b),
            BinOp::Le => self.emit(le, dst, a, b),
            BinOp::Gt => self.emit(lt, dst, b, a),
            BinOp::Ge => self.emit(le, dst, b, a),
            _ => unreachable!(),
        }
    }

    /// An integer binary operation (the result is narrowed to the computation type).
    fn int_arith(&mut self, bop: BinOp, calc: Ty, overflow: bool, dst: u32, a: u32, b: u32) {
        let is64 = matches!(calc, Ty::Int64 | Ty::UInt64);
        let unsigned = calc == Ty::UInt64;
        match bop {
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                let opc = match (bop, is64 && overflow, unsigned) {
                    (BinOp::Add, true, false) => op::ADD_INT_CHK,
                    (BinOp::Sub, true, false) => op::SUB_INT_CHK,
                    (BinOp::Mul, true, false) => op::MUL_INT_CHK,
                    (BinOp::Add, true, true) => op::ADD_UINT_CHK,
                    (BinOp::Sub, true, true) => op::SUB_UINT_CHK,
                    (BinOp::Mul, true, true) => op::MUL_UINT_CHK,
                    (BinOp::Add, false, _) => op::ADD_INT,
                    (BinOp::Sub, false, _) => op::SUB_INT,
                    _ => op::MUL_INT,
                };
                self.emit(opc, dst, a, b);
                self.narrow_arith(dst, calc, overflow);
            }
            BinOp::Div => {
                let opc = if unsigned {
                    op::DIV_UINT
                } else if calc == Ty::Int64 && overflow {
                    op::DIV_INT_CHK
                } else {
                    op::DIV_INT
                };
                self.emit(opc, dst, a, b);
                self.narrow_arith(dst, calc, overflow);
            }
            BinOp::Mod => {
                let opc = if unsigned { op::MOD_UINT } else { op::MOD_INT };
                self.emit(opc, dst, a, b);
            }
            BinOp::And => self.emit(op::AND_INT, dst, a, b),
            BinOp::Or => self.emit(op::OR_INT, dst, a, b),
            BinOp::Xor => self.emit(op::XOR_INT, dst, a, b),
            BinOp::Shl | BinOp::Shr => {
                let left = bop == BinOp::Shl;
                if is64 {
                    self.emit(if left { op::SHL_INT } else { op::SHR_INT }, dst, a, b);
                } else {
                    self.emit(if left { op::SHL_INT32 } else { op::SHR_INT32 }, dst, a, b);
                    if calc == Ty::Cardinal {
                        self.emit(op::NARROW, dst, dst, 5);
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    /// Conditional branch: jumps to `label` when the expression equals `jump_if`.
    fn cond(&mut self, e: &Expr, label: Label, jump_if: bool) {
        let old = self.set_line(e.span);
        let mark = self.mark();
        if let Some(Const::Bool(b)) = &e.value {
            if *b == jump_if {
                self.emit_jump(op::JMP, 0, 0, label);
            }
            self.release(mark);
            self.line = old;
            return;
        }
        match &e.kind {
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
                calc: Calc::Bool,
            } => self.cond(operand, label, !jump_if),
            ExprKind::Binary { first, steps }
                if steps.last().is_some_and(|s| s.op.is_comparison()) =>
            {
                // If the last step is a comparison, compute its left side (`X and 1` in `X and 1 = 0`) first
                // so that the comparison and branch become one instruction
                let step = steps.last().unwrap();
                let a = if steps.len() == 1 {
                    self.expr_any(first)
                } else {
                    let prefix = &steps[..steps.len() - 1];
                    let lhs = Expr {
                        kind: ExprKind::Binary {
                            first: first.clone(),
                            steps: prefix.to_vec(),
                        },
                        ty: prefix.last().unwrap().ty,
                        value: None,
                        span: TextRange::new(
                            first.span.start(),
                            prefix.last().unwrap().rhs.span.end(),
                        ),
                    };
                    self.expr_any(&lhs)
                };
                let b = self.expr_any(&step.rhs);
                let old2 = self.set_line(step.span);
                match step.calc {
                    Calc::Str if matches!(step.op, BinOp::Eq | BinOp::Ne) => {
                        let jop = if (step.op == BinOp::Eq) == jump_if {
                            op::JMP_IF_EQ_STR
                        } else {
                            op::JMP_IF_NE_STR
                        };
                        self.emit_jump(jop, a, b, label);
                    }
                    Calc::Str => {
                        let t = self.alloc();
                        self.emit(op::CMP_STR, t, a, b);
                        let z = self.alloc();
                        self.emit(op::LOAD_ZERO, z, 0, 0);
                        self.fused_compare(step.op, t, z, false, label, jump_if);
                    }
                    Calc::Int(c) => {
                        self.fused_compare(step.op, a, b, c == Ty::UInt64, label, jump_if)
                    }
                    Calc::Bool | Calc::Char => {
                        self.fused_compare(step.op, a, b, false, label, jump_if)
                    }
                }
                self.line = old2;
            }
            ExprKind::Binary { first, steps }
                if !steps.is_empty()
                    && steps
                        .iter()
                        .all(|s| s.calc == Calc::Bool && s.op == BinOp::And) =>
            {
                if !jump_if {
                    self.cond(first, label, false);
                    for s in steps {
                        self.cond(&s.rhs, label, false);
                    }
                } else {
                    let skip = self.new_label();
                    self.cond(first, skip, false);
                    for (i, s) in steps.iter().enumerate() {
                        if i + 1 < steps.len() {
                            self.cond(&s.rhs, skip, false);
                        } else {
                            self.cond(&s.rhs, label, true);
                        }
                    }
                    self.bind(skip);
                }
            }
            ExprKind::Binary { first, steps }
                if !steps.is_empty()
                    && steps
                        .iter()
                        .all(|s| s.calc == Calc::Bool && s.op == BinOp::Or) =>
            {
                if jump_if {
                    self.cond(first, label, true);
                    for s in steps {
                        self.cond(&s.rhs, label, true);
                    }
                } else {
                    let skip = self.new_label();
                    self.cond(first, skip, true);
                    for (i, s) in steps.iter().enumerate() {
                        if i + 1 < steps.len() {
                            self.cond(&s.rhs, skip, true);
                        } else {
                            self.cond(&s.rhs, label, false);
                        }
                    }
                    self.bind(skip);
                }
            }
            _ => {
                let t = self.expr_any(e);
                let jop = if jump_if {
                    op::JMP_IF_TRUE
                } else {
                    op::JMP_IF_FALSE
                };
                self.emit_jump(jop, t, 0, label);
            }
        }
        self.release(mark);
        self.line = old;
    }

    /// A fused compare-and-branch.
    fn fused_compare(
        &mut self,
        bop: BinOp,
        a: u32,
        b: u32,
        unsigned: bool,
        label: Label,
        jump_if: bool,
    ) {
        // If jump_if is false, branch on the negated condition
        let (bop, a, b) = if jump_if {
            (bop, a, b)
        } else {
            match bop {
                BinOp::Eq => (BinOp::Ne, a, b),
                BinOp::Ne => (BinOp::Eq, a, b),
                BinOp::Lt => (BinOp::Le, b, a),
                BinOp::Le => (BinOp::Lt, b, a),
                BinOp::Gt => (BinOp::Le, a, b),
                BinOp::Ge => (BinOp::Lt, a, b),
                _ => unreachable!(),
            }
        };
        let (lt, le) = if unsigned {
            (op::JMP_IF_LT_UINT, op::JMP_IF_LE_UINT)
        } else {
            (op::JMP_IF_LT_INT, op::JMP_IF_LE_INT)
        };
        match bop {
            BinOp::Eq => self.emit_jump(op::JMP_IF_EQ_INT, a, b, label),
            BinOp::Ne => self.emit_jump(op::JMP_IF_NE_INT, a, b, label),
            BinOp::Lt => self.emit_jump(lt, a, b, label),
            BinOp::Le => self.emit_jump(le, a, b, label),
            BinOp::Gt => self.emit_jump(lt, b, a, label),
            BinOp::Ge => self.emit_jump(le, b, a, label),
            _ => unreachable!(),
        }
    }

    // ---- Calls ----

    fn call(&mut self, routine: RoutineId, args: &[CallArg], dst: u32) {
        let base = self.alloc();
        let info_params = self.program.routines[routine.0 as usize].param_count as usize;
        // Allocate the arguments contiguously from base
        for _ in 1..info_params {
            self.alloc();
        }
        for (i, a) in args.iter().enumerate() {
            let reg = base + i as u32;
            match a {
                CallArg::Value(e) => self.expr(e, reg),
                CallArg::Ref { place, .. } => self.make_ref(place, reg),
            }
        }
        let f = routine.0 + self.mb.routine_index_offset;
        self.emit_bx(op::CALL, base, f);
        if dst != base {
            self.emit(op::MOVE, dst, base, 0);
        }
    }

    /// Creates a reference slot for a `var` / `out` argument in `dst`.
    fn make_ref(&mut self, place: &Place, dst: u32) {
        match place {
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                if self.is_ref_param(*v) {
                    // Pass the reference on
                    self.emit(op::MOVE, dst, r, 0);
                } else {
                    self.emit(op::MAKE_REF, dst, r, 0);
                }
            }
            Place::Global(g) => self.emit_bx(op::MAKE_GLOBAL_REF, dst, g.0),
            Place::StrIndex { .. } => unreachable!("string element cannot be a var argument"),
        }
    }

    fn builtin(&mut self, func: Builtin, args: &[Expr], dst: u32, span: TextRange) {
        match func {
            Builtin::Length => {
                let s = self.expr_any(&args[0]);
                self.emit(op::STR_LEN, dst, s, 0);
            }
            Builtin::Copy => {
                let b = self.alloc();
                self.alloc();
                self.alloc();
                for (i, a) in args.iter().enumerate() {
                    self.expr(a, b + i as u32);
                }
                self.emit(op::STR_COPY, dst, b, 0);
            }
            Builtin::Pos | Builtin::StrToIntDef => {
                let b = self.alloc();
                self.alloc();
                for (i, a) in args.iter().enumerate() {
                    self.expr(a, b + i as u32);
                }
                let opc = if func == Builtin::Pos {
                    op::STR_POS
                } else {
                    op::STR_TO_INT_DEF
                };
                self.emit(opc, dst, b, 0);
            }
            Builtin::Ord => self.expr(&args[0], dst),
            Builtin::Chr => {
                // sema turns this into Convert, so it never gets here
                self.expr(&args[0], dst);
            }
            Builtin::Abs => {
                let x = &args[0];
                self.expr(x, dst);
                if x.ty == Ty::UInt64 {
                    return;
                }
                let z = self.alloc();
                self.emit(op::LOAD_ZERO, z, 0, 0);
                let skip = self.new_label();
                self.emit_jump(op::JMP_IF_LE_INT, z, dst, skip);
                let checks = self.checks_here(span);
                let calc = if x.ty.fits_in_integer() {
                    Ty::Integer
                } else {
                    x.ty
                };
                if calc == Ty::Int64 && checks.overflow {
                    self.emit(op::NEG_INT_CHK, dst, dst, 0);
                } else {
                    self.emit(op::NEG_INT, dst, dst, 0);
                    self.narrow_arith(dst, calc, checks.overflow);
                }
                self.bind(skip);
            }
            Builtin::ParamCount => self.emit(op::ARG_COUNT, dst, 0, 0),
            Builtin::ParamStr => {
                let i = self.expr_any(&args[0]);
                self.emit(op::ARG_GET, dst, i, 0);
            }
            Builtin::IntToStr
            | Builtin::UIntToStr
            | Builtin::StrToInt
            | Builtin::StrToInt64
            | Builtin::UpperCase
            | Builtin::LowerCase
            | Builtin::Trim => {
                let x = self.expr_any(&args[0]);
                let opc = match func {
                    Builtin::IntToStr => op::INT_TO_STR,
                    Builtin::UIntToStr => op::UINT_TO_STR,
                    Builtin::StrToInt => op::STR_TO_INT,
                    Builtin::StrToInt64 => op::STR_TO_INT64,
                    Builtin::UpperCase => op::STR_UPPER_ASCII,
                    Builtin::LowerCase => op::STR_LOWER_ASCII,
                    _ => op::STR_TRIM,
                };
                self.emit(opc, dst, x, 0);
            }
            Builtin::GetEnvironmentVariable => {
                let a = self.alloc();
                self.expr(&args[0], a);
                let h = self.mb.host("Env.Get", 1, true);
                self.emit_bx(op::CALL_HOST, a, h);
                if dst != a {
                    self.emit(op::MOVE, dst, a, 0);
                }
            }
        }
    }
}

/// Converts a constant to a string for `Write` (integers, `Boolean`, `Char`, `string`).
fn const_as_string(c: &Const, ty: Ty) -> Option<Vec<u16>> {
    Some(match c {
        Const::Str(s) => s.clone(),
        Const::Char(ch) => vec![*ch],
        Const::Bool(b) => if *b { "TRUE" } else { "FALSE" }.encode_utf16().collect(),
        Const::Int(v) => {
            let text = if ty == Ty::UInt64 {
                (*v as u64).to_string()
            } else {
                v.to_string()
            };
            text.encode_utf16().collect()
        }
    })
}

/// Whether an instruction's operand does not fit in 8 bits.
fn plain_needs_wide(i: &Instr) -> bool {
    match i {
        Instr::Plain { op: opc, a, b, c } => {
            let shape = op::info(*opc).map(|i| i.shape).unwrap_or(op::Shape::None);
            match shape {
                op::Shape::ABx => *a > 255 || (b | (c << 8)) > 0xFFFF,
                op::Shape::ASBxImm => *a > 255,
                op::Shape::ABsC | op::Shape::ABsCImm => *a > 255 || *b > 255,
                _ => *a > 255 || *b > 255 || *c > 255,
            }
        }
        Instr::Jump { .. } => false,
    }
}

/// The inverted condition of a compare-and-branch.
fn invert_fused(opc: u8, a: u32, b: u32) -> (u8, u32, u32) {
    match opc {
        op::JMP_IF_EQ_INT => (op::JMP_IF_NE_INT, a, b),
        op::JMP_IF_NE_INT => (op::JMP_IF_EQ_INT, a, b),
        op::JMP_IF_LT_INT => (op::JMP_IF_LE_INT, b, a),
        op::JMP_IF_LE_INT => (op::JMP_IF_LT_INT, b, a),
        op::JMP_IF_LT_UINT => (op::JMP_IF_LE_UINT, b, a),
        op::JMP_IF_LE_UINT => (op::JMP_IF_LT_UINT, b, a),
        op::JMP_IF_EQ_STR => (op::JMP_IF_NE_STR, a, b),
        op::JMP_IF_NE_STR => (op::JMP_IF_EQ_STR, a, b),
        _ => unreachable!(),
    }
}

/// Whether the expression reads the variable in register `reg`.
fn expr_reads_reg(e: &Expr, reg: u32) -> bool {
    match &e.kind {
        ExprKind::Var(v) => v.0 == reg,
        ExprKind::Const | ExprKind::Global(_) | ExprKind::Error => false,
        ExprKind::Binary { first, steps } => {
            expr_reads_reg(first, reg) || steps.iter().any(|s| expr_reads_reg(&s.rhs, reg))
        }
        ExprKind::Unary { operand, .. } | ExprKind::Convert { operand, .. } => {
            expr_reads_reg(operand, reg)
        }
        ExprKind::StrIndex { base, index } => {
            expr_reads_reg(base, reg) || expr_reads_reg(index, reg)
        }
        ExprKind::Call { args, .. } => args.iter().any(|a| match a {
            CallArg::Value(v) => expr_reads_reg(v, reg),
            CallArg::Ref { place, .. } => place_reads_reg(place, reg),
        }),
        ExprKind::Builtin { args, .. } => args.iter().any(|a| expr_reads_reg(a, reg)),
    }
}

fn place_reads_reg(p: &Place, reg: u32) -> bool {
    match p {
        Place::Var(v, _) => v.0 == reg,
        Place::Global(_) => false,
        Place::StrIndex { base, index, .. } => {
            place_reads_reg(base, reg) || expr_reads_reg(index, reg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::{BuildConfig, SwitchTable, Switches};
    use crate::driver::{CompileOptions, compile};
    use crate::source::MemorySourceProvider;

    fn dump(src: &str) -> String {
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "Hello.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        assert!(
            out.diagnostics.iter().all(|d| !d.is_error()),
            "{:#?}",
            out.diagnostics
        );
        crate::bytecode::disassemble(out.stages.module.as_ref().expect("module"))
    }

    #[test]
    fn hello() {
        let d = dump("program Hello;\n\nbegin\n  WriteLn('Hello, world');\nend.\n");
        let expected = "\
; puyc format 1
.const
  0: str \"Hello, world\\n\"
.global
.host
  0: Console.Write params=1 ret=0
.unit Hello file=\"Hello.pas\" init=- fini=- entry=0
.func 0 Hello.<main> params=0 regs=1
  .line 4
  0000  LoadConst    r0, k0
  0001  CallHost     r0, h0
  .line 5
  0002  RetVoid
";
        assert_eq!(d, expected);
    }

    #[test]
    fn arithmetic_and_calls() {
        let d = dump(
            "program P;\nvar Total: Integer = 1;\nfunction Sq(X: Integer): Integer;\nbegin\n  Result := X * X;\nend;\nbegin\n  Total := Sq(Total) + 2;\n  WriteLn(Total);\nend.\n",
        );
        eprintln!("{d}");
        assert!(d.contains(".func 1 P.<init>"));
        assert!(d.contains("MulInt       r1, r1, r0"));
        assert!(d.contains("NarrowOvf    r1, r1, 4"));
        assert!(d.contains("Call         r1, f2"));
        assert!(d.contains("StoreGlobal  r0, g0") || d.contains("StoreGlobal  r1, g0"));
    }

    #[test]
    fn loops_and_conditions() {
        let d = dump(
            "var S := 0;\nfor var I := 1 to 10 do\n  if (I mod 2 = 0) and (I > 4) then S := S + I;\nwhile S > 0 do Dec(S);\nrepeat Inc(S) until S >= 3;\nWriteLn(S);\n",
        );
        eprintln!("{d}");
        assert!(d.contains("JmpIfLtInt"));
        assert!(d.contains("JmpIfEqInt"));
        assert!(d.contains("AddIntI"));
        assert!(d.contains("ModInt"));
        assert!(d.ends_with("RetVoid\n"));
    }

    #[test]
    fn verifies() {
        for src in [
            "program P;\nprocedure Swap(var A, B: Integer);\nvar T: Integer;\nbegin T := A; A := B; B := T; end;\nvar X, Y: Integer;\nbegin X := 1; Y := 2; Swap(X, Y); WriteLn(X, ' ', Y); end.",
            "uses System.SysUtils;\nvar S := 'abc';\nS[1] := 'x';\nWriteLn(Copy(S, 1, 2), Pos('b', S), UpperCase(S), IntToStr(Length(S)), StrToIntDef('1', 0), ParamStr(0), ParamCount, GetEnvironmentVariable('HOME'));\nvar N := 0; ReadLn(N); ReadLn; var L := ''; ReadLn(L); Assert(N >= 0, 'neg'); if N = 1 then Halt(3);",
            "var U: UInt64 := 1; var I: Int64 := -1; U := U * 2; I := I - Abs(I) + (I shl 3); var C: Cardinal := 7; C := C shr 1; var B: Byte := 200; Inc(B, 5); var Ch := 'a'; Inc(Ch); WriteLn(U, I, C, B, Ch, Chr(66), Ord(Ch), Boolean(1), Char(65), string(Ch));",
            "{$Q-}{$R-}\nvar B: Byte := 200; Inc(B, 100); var I := 2147483647; I := I + 1; WriteLn(B, I);",
        ] {
            let mut provider = MemorySourceProvider::new();
            let out = compile(
                "t.pas",
                src.as_bytes().to_vec(),
                &mut provider,
                &CompileOptions::default(),
            );
            assert!(
                out.diagnostics.iter().all(|d| !d.is_error()),
                "{src}\n{:#?}",
                out.diagnostics
            );
            let bytes = out.module.expect("module");
            let m = match crate::bytecode::Module::decode(&bytes, crate::VERSION) {
                Ok(m) => m,
                Err(e) => panic!(
                    "{e}\n{}",
                    crate::bytecode::disassemble(out.stages.module.as_ref().unwrap())
                ),
            };
            assert_eq!(&m, out.stages.module.as_ref().unwrap());
        }
    }

    #[test]
    fn wide_table_operands() {
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            Vec::new(),
            &mut provider,
            &CompileOptions::default(),
        );
        let program = out.stages.hir.unwrap();
        let dir = DirectiveOutput {
            tokens: Vec::new(),
            switches: SwitchTable::new(Switches::initial(&BuildConfig::default())),
            placements: Vec::new(),
            blocks: Vec::new(),
            suppress_after: None,
            include_positions: Vec::new(),
        };
        let mut sink = DiagnosticSink::new();
        let mut mb = ModuleBuilder {
            consts: Vec::new(),
            hosts: Vec::new(),
            globals: Vec::new(),
            funcs: Vec::new(),
            program: program.name.clone(),
            src: out.sources.get(FileId(0)),
            sink: &mut sink,
            file: FileId(0),
            failed: false,
            routine_index_offset: 1,
            dir: &dir,
        };
        for opc in [
            op::LOAD_CONST,
            op::LOAD_GLOBAL,
            op::STORE_GLOBAL,
            op::MAKE_GLOBAL_REF,
            op::CALL,
            op::CALL_HOST,
        ] {
            for idx in [65535, 65536, 0x1234_5678, u32::MAX] {
                let mut builder = FuncBuilder::new(&mut mb, &program.main, &program);
                builder.emit_bx(opc, 300, idx);
                let (code, _) = builder.layout();
                assert_eq!(code.len(), 3);
                assert_eq!(code[0], op::WIDE as u32);
                assert_eq!(code[1], opc as u32 | (44 << 8) | ((idx & 0xFFFF) << 16));
                assert_eq!(code[2], (1 << 8) | (idx & 0xFFFF_0000));
            }
        }
    }

    #[test]
    fn long_chain_and_wide_jumps() {
        // A 300-term concatenation and a branch that jumps over a long body
        let mut src = String::from("var S := 'x'");
        for _ in 0..300 {
            src.push_str(" + 'y'");
        }
        src.push_str(";\nvar N := 0;\nif N = 0 then begin\n");
        for i in 0..200 {
            src.push_str(&format!("  S := S + '{i}';\n"));
        }
        src.push_str("end;\nWriteLn(S);\n");
        let d = dump(&src);
        assert!(d.contains("w.JmpIfNeInt") || d.contains("JmpIfNeInt"));
    }
}
