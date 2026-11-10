//! IR: typed virtual registers and basic blocks (`docs/design/compiler-pipeline.md` 3.5,
//! `docs/design/lifetime.md` 3.10).
//!
//! `lower` builds the IR from the HIR, `codegen` turns it into bytecode, and `dump_ir` prints it
//! (`puy check --dump ir`; `docs/spec/bytecode.md` 5.1). Instruction names are the bytecode names:
//! an instruction maps to one bytecode instruction except for calls (whose arguments codegen lays out)
//! and the block terminators (whose fall-through jumps codegen drops).

mod dump;
pub mod lower;

pub use dump::dump_ir;

use crate::bytecode::HostEntry;
use crate::sema::hir::VarKind;
use crate::sema::types::{Const, Ty, TypeTable};
use crate::source::TextRange;

/// A virtual register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reg {
    /// A named variable (`vN`; the number is the HIR `VarId`).
    Var(u32),
    /// A temporary created by `lower` (`tN`; an index into `Function::temps`).
    Temp(u32),
    /// The static link of a nested routine (`link`; the hidden first argument, register 0;
    /// `docs/spec/semantics.md` 4.9).
    Link,
}

/// What a temporary holds (the kind column of `.reg tN`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempKind {
    /// The value of an expression (`temp`).
    Value,
    /// A reference slot made for a `var` / `out` argument (`ref`).
    Ref,
    /// A cell: the reference of a field or element passed to a `var` / `out` parameter (`cell`;
    /// `docs/design/lifetime.md` 3.7). It owns its heap block until the routine ends.
    Cell,
    /// The state register of a `try..finally` (`state`; `docs/spec/exceptions.md` 6.3). An integer
    /// with a register of its own; the [`TempKind::Exception`] temporary created right after it
    /// gets the next register.
    State,
    /// The exception held by an `except` block, or propagated through a `finally` block
    /// (`exception`). It owns its value.
    Exception,
}

#[derive(Debug, Clone)]
pub struct Temp {
    /// `None` for reference slots and for the destination of a procedure call (`-` in the dump).
    pub ty: Option<Ty>,
    pub kind: TempKind,
    /// Where codegen puts the temporary (not shown in the dump).
    ///
    /// `Some(depth)`: the depth in the stack of temporaries when it was created. Codegen gives the
    /// temporary the register after the named variables and the owning temporaries plus this depth,
    /// so these temporaries are reused like a stack (the M1 rule). Call arguments and frames live here,
    /// at the top of the frame, so that a call only overwrites them. A managed value in such a
    /// temporary is owned by nobody: it is borrowed (a `const` argument) or handed to the callee
    /// (a value argument; `docs/design/lifetime.md` 3.3).
    ///
    /// `None`: a register of its own that holds one reference of a managed value and is released at
    /// the end of the routine (one register per expression position; `docs/design/lifetime.md` 3.4).
    /// It is listed in [`Function::finally`].
    pub slot: Option<u32>,
}

/// A named variable (the `.reg vN` lines; the same numbering and kinds as `.var` in the `hir` dump).
#[derive(Debug, Clone)]
pub struct VarReg {
    /// Empty for hidden variables.
    pub name: String,
    pub ty: Ty,
    pub kind: VarKind,
    pub is_for_var: bool,
}

/// An operand of a plain instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Reg(Reg),
    /// An immediate (`LoadInt` value, `AddIntI` delta, the width of `Narrow` and `StrToIntIO`).
    Imm(i64),
    /// An integer constant (codegen puts it in the constant pool).
    Int(i64),
    /// A floating-point constant as the bits of an `f64` (codegen puts it in the constant pool).
    Float(u64),
    /// A string constant in UTF-16 code units (codegen puts it in the constant pool).
    Str(Vec<u16>),
    /// A `UTF8String` constant (codegen puts it in the constant pool as kind 7).
    U8(String),
    Global(u32),
    Func(u32),
    Host(u32),
    /// A record or static array type (`Unique`; codegen puts it in the type section).
    Type(Ty),
    /// A record or static array constant (codegen puts it and its elements in the constant pool).
    Agg(Ty, Const),
}

#[derive(Debug, Clone)]
pub enum InstKind {
    /// Operands in the order of the bytecode instruction's fields. A consecutive register range
    /// (`StrCopy`, `StrPos`, `StrToIntDef`) lists each register of the range.
    Plain(Vec<Operand>),
    /// `Call` / `CallHost`. `callee` is `Operand::Func` or `Operand::Host`.
    Call {
        dst: Option<Reg>,
        callee: Operand,
        args: Vec<Reg>,
        /// Where the callee's frame starts: the arguments are in it and the following temporaries,
        /// and the result comes back in it. The first argument, or a temporary of its own when there are none.
        frame: Reg,
    },
}

#[derive(Debug, Clone)]
pub struct Inst {
    pub op: u8,
    pub kind: InstKind,
    pub line: u32,
}

/// Index of a block in `Function::blocks` (`bbN`; the order codegen lays the blocks out in).
pub type BlockId = u32;

/// The instruction that ends a block.
#[derive(Debug, Clone)]
pub enum Term {
    Jmp(BlockId),
    /// `JmpIfTrue` / `JmpIfFalse` (`b` is `None`) and the compare-and-branch instructions.
    /// Goes to `then` when the condition holds and to `other` when it does not.
    Branch {
        op: u8,
        a: Reg,
        b: Option<Reg>,
        then: BlockId,
        other: BlockId,
    },
    Ret(Reg),
    RetVoid,
    Halt(Reg),
    /// `Raise`: raises the exception in the register (`docs/spec/exceptions.md` 6.2).
    Raise(Reg),
    /// `RaiseOuter A B`: raises `A` with `B` as its `InnerException`.
    RaiseOuter(Reg, Reg),
    /// `EndFinally` (`docs/spec/exceptions.md` 6.2): the end of a `finally` block. State 0 goes on
    /// to `next`; state `k + 2` goes to `targets[k - 1]` (codegen puts a `Jmp` to each after the
    /// instruction).
    EndFinally {
        state: Reg,
        next: BlockId,
        targets: Vec<BlockId>,
    },
}

/// An entry of the handler table (`docs/spec/exceptions.md` 6.3), with inclusive block ranges in
/// layout order.
#[derive(Debug, Clone)]
pub struct TryEntry {
    pub finally: bool,
    pub body: (BlockId, BlockId),
    /// The `except` block (with the dispatch of the handlers), or the `finally` block (its last
    /// block ends with `EndFinally`).
    pub handler: (BlockId, BlockId),
    /// `except`: the exception. `finally`: the state; the exception is the next temporary.
    pub reg: Reg,
    /// `finally`: the exception being propagated.
    pub exc: Option<Reg>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub insts: Vec<Inst>,
    pub term: Term,
    pub term_line: u32,
}

#[derive(Debug, Clone)]
pub struct Function {
    /// Fully qualified name (`Hello.Greet`).
    pub name: String,
    /// The unit it belongs to (index into `Module::units`).
    pub file: u32,
    /// Short name (for diagnostics).
    pub short_name: String,
    /// Position of the name in the heading (for diagnostics).
    pub name_span: TextRange,
    /// The number of arguments, the static link included.
    pub params: u32,
    /// Takes the static link (`Reg::Link`) as its first argument; the named variables follow it.
    pub link: bool,
    /// The function whose frame the static link points to: the one the routine is nested in
    /// (`docs/spec/semantics.md` 4.9). Set when `link` is.
    pub outer: Option<u32>,
    pub vars: Vec<VarReg>,
    pub temps: Vec<Temp>,
    /// `bb0` is the entry.
    pub blocks: Vec<Block>,
    /// Registers released by the implicit `finally` when the routine ends, in release order
    /// (`docs/design/lifetime.md` 3.8): managed value parameters, `Result` and locals in declaration
    /// order, then the owning temporaries in creation order.
    pub finally: Vec<Reg>,
    /// The handler table, inner entries first.
    pub tries: Vec<TryEntry>,
}

#[derive(Debug, Clone)]
pub struct Global {
    pub name: String,
    pub ty: Ty,
    /// `System.ExitCode` (`bytecode::GLOBAL_EXIT_CODE`).
    pub exit_code: bool,
}

/// The IR of a module.
#[derive(Debug, Clone)]
pub struct Module {
    /// Program name.
    pub program: String,
    /// Records and static arrays (names for the dump, slots for the type section).
    pub types: TypeTable,
    pub globals: Vec<Global>,
    pub hosts: Vec<HostEntry>,
    /// In function table order (`bytecode.md` 3.2, `units.md` 6.1).
    pub funcs: Vec<Function>,
    /// The units in initialization order, the program last (`units.md` 6.3).
    pub units: Vec<UnitEntry>,
    /// Index of `<main>` in `funcs`.
    pub entry: u32,
}

/// A unit of the module.
#[derive(Debug, Clone)]
pub struct UnitEntry {
    pub name: String,
    /// The source path (`bytecode.md` 3.2).
    pub path: String,
    /// Index of its `<init>` in `funcs`.
    pub init: Option<u32>,
    /// Index of its `<fini>` in `funcs`.
    pub fini: Option<u32>,
    /// The unit's global variables (`units.md` 6.3).
    pub first_global: u32,
    pub global_count: u32,
}
