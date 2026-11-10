//! The `ir` dump (`docs/spec/bytecode.md` 5.1). The format of `puy check --dump ir` and of golden tests (`.ir`).

use super::*;
use crate::bytecode::{debug_units, op};
use crate::sema::dump::{const_value_text, var_kind_text};
use std::fmt::Write;

/// Version of the `ir` dump format (the first line).
const FORMAT_VERSION: u32 = 1;

/// Builds the `ir` dump. Each line ends with a single newline.
pub fn dump_ir(m: &Module) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "; puy ir {FORMAT_VERSION}");
    out.push_str(".global\n");
    for (i, g) in m.globals.iter().enumerate() {
        let _ = writeln!(out, "  g{i} {}: {}", g.name, m.types.name(g.ty));
    }
    for (i, f) in m.funcs.iter().enumerate() {
        function(&mut out, i, f, &m.types);
    }
    out
}

fn function(out: &mut String, index: usize, f: &Function, types: &TypeTable) {
    let _ = write!(out, ".func f{index} {} params={}", f.name, f.params);
    if let Some(o) = f.outer {
        let _ = write!(out, " outer=f{o}");
    }
    out.push('\n');
    if f.link {
        out.push_str("  .reg link\n");
    }
    for (i, v) in f.vars.iter().enumerate() {
        let name = if v.name.is_empty() { "-" } else { &v.name };
        let kind = var_kind_text(v.kind, v.is_for_var);
        let _ = writeln!(out, "  .reg v{i} {name}: {} {kind}", types.name(v.ty));
    }
    for (i, t) in f.temps.iter().enumerate() {
        let ty = t.ty.map(|t| types.name(t)).unwrap_or_else(|| "-".into());
        let kind = match t.kind {
            TempKind::Value => "temp",
            TempKind::Ref => "ref",
            TempKind::Cell => "cell",
            TempKind::State => "state",
            TempKind::Exception => "exception",
        };
        let _ = writeln!(out, "  .reg t{i}: {ty} {kind}");
    }
    for t in &f.tries {
        let kind = if t.finally { "finally" } else { "except" };
        let regs = match t.exc {
            Some(e) => format!("{}, {}", reg(t.reg), reg(e)),
            None => reg(t.reg),
        };
        let _ = writeln!(
            out,
            "  .try bb{}..bb{} {kind} bb{}..bb{} {regs}",
            t.body.0, t.body.1, t.handler.0, t.handler.1
        );
    }
    for (b, block) in f.blocks.iter().enumerate() {
        let _ = writeln!(out, "bb{b}:");
        let mut line = None;
        let mut set_line = |out: &mut String, l: u32| {
            if line != Some(l) {
                let _ = writeln!(out, "  .line {l}");
                line = Some(l);
            }
        };
        for inst in &block.insts {
            set_line(out, inst.line);
            let operands = match &inst.kind {
                InstKind::Plain(ops) => ops
                    .iter()
                    .map(|o| operand(o, types))
                    .collect::<Vec<_>>()
                    .join(", "),
                InstKind::Call {
                    dst, callee, args, ..
                } => {
                    let dst = dst.map(reg).unwrap_or_else(|| "-".into());
                    let args: Vec<String> = args.iter().map(|a| reg(*a)).collect();
                    format!("{dst}, {}({})", operand(callee, types), args.join(", "))
                }
            };
            instruction(out, inst.op, &operands);
        }
        set_line(out, block.term_line);
        match &block.term {
            Term::Jmp(t) => instruction(out, op::JMP, &format!("bb{t}")),
            Term::Branch {
                op,
                a,
                b,
                then,
                other,
            } => {
                let mut text = reg(*a);
                if let Some(b) = b {
                    let _ = write!(text, ", {}", reg(*b));
                }
                let _ = write!(text, ", bb{then}, bb{other}");
                instruction(out, *op, &text);
            }
            Term::Ret(r) => instruction(out, op::RET, &reg(*r)),
            Term::RetVoid => instruction(out, op::RET_VOID, ""),
            Term::Halt(r) => instruction(out, op::HALT, &reg(*r)),
            Term::Raise(r) => instruction(out, op::RAISE, &reg(*r)),
            Term::RaiseOuter(a, b) => {
                instruction(out, op::RAISE_OUTER, &format!("{}, {}", reg(*a), reg(*b)))
            }
            Term::EndFinally {
                state,
                next,
                targets,
            } => {
                let mut text = format!("{}, bb{next}", reg(*state));
                for t in targets {
                    let _ = write!(text, ", bb{t}");
                }
                instruction(out, op::END_FINALLY, &text);
            }
        }
    }
    // The list of managed registers (`lifetime.md` 3.8)
    if f.finally.is_empty() {
        out.push_str(".finally\n");
    } else {
        let regs: Vec<String> = f.finally.iter().map(|r| reg(*r)).collect();
        let _ = writeln!(out, ".finally {}", regs.join(", "));
    }
}

/// One instruction line: the name padded to 12 columns, then the operands (5).
fn instruction(out: &mut String, opc: u8, operands: &str) {
    let name = op::info(opc)
        .expect("IR instructions use bytecode opcodes")
        .name;
    if operands.is_empty() {
        let _ = writeln!(out, "  {name}");
    } else {
        let _ = writeln!(out, "  {name:<12} {operands}");
    }
}

fn reg(r: Reg) -> String {
    match r {
        Reg::Var(n) => format!("v{n}"),
        Reg::Temp(n) => format!("t{n}"),
        Reg::Link => "link".into(),
    }
}

fn operand(o: &Operand, types: &TypeTable) -> String {
    match o {
        Operand::Type(t) => types.full_name(*t),
        Operand::Agg(t, c) => format!("{} {}", types.full_name(*t), const_value_text(c, *t, types)),
        Operand::Reg(r) => reg(*r),
        Operand::Imm(v) | Operand::Int(v) => v.to_string(),
        Operand::Float(b) => crate::numfmt::f64_to_str(f64::from_bits(*b)),
        Operand::Str(units) => debug_units(units),
        Operand::U8(s) => format!(
            "utf8 {}",
            debug_units(&s.encode_utf16().collect::<Vec<_>>())
        ),
        Operand::Global(g) => format!("g{g}"),
        Operand::Func(f) => format!("f{f}"),
        Operand::Host(h) => format!("h{h}"),
    }
}
