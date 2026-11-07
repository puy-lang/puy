//! HIR: the tree with names and types resolved (`docs/design/compiler-pipeline.md` 3.4).
//!
//! Left-leaning chains (chains of left operands of binary operators, `else if` chains) are stored as sequences.

use super::types::{Const, Ty};
use crate::source::{TextRange, TextSize};
use crate::syntax::ast::ParamMode;

/// The whole program.
#[derive(Debug)]
pub struct Program {
    /// Program name (prefix of the routines' fully qualified names).
    pub name: String,
    pub globals: Vec<Global>,
    /// Routines (in declaration order; a `forward` routine is placed at its first heading).
    pub routines: Vec<Routine>,
    /// `<main>`.
    pub main: Routine,
    /// `<init>` (assigns initial values to global variables). `None` if there are none.
    pub init: Option<Routine>,
    /// Whether `System.SysUtils` is in `uses`.
    pub uses_sysutils: bool,
}

#[derive(Debug, Clone)]
pub struct Global {
    pub name: String,
    pub ty: Ty,
    pub span: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlobalId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoutineId(pub u32);

/// Index of a variable within a routine (parameters, `Result`, locals, inline variables, hidden variables).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VarKind {
    Param(ParamMode),
    Result,
    Local,
    /// A hidden (unnamed) variable, e.g. the iteration count of a `for` whose loop variable is a global.
    Hidden,
}

#[derive(Debug, Clone)]
pub struct Var {
    /// Name (empty for hidden variables).
    pub name: String,
    pub ty: Ty,
    pub kind: VarKind,
    /// Position of the declaring identifier.
    pub span: TextRange,
    /// The declaration node (`VAR_DECL` / `INLINE_VAR_DECL` / `FOR_VAR`; for building fix edits).
    pub decl: Option<crate::syntax::SyntaxNode>,
    /// Declared with `for var`.
    pub is_for_var: bool,
    /// Used as a `for` loop variable (exempt from W0001).
    pub is_loop_var: bool,
}

#[derive(Debug)]
pub struct Routine {
    /// Short name (`Square`, `<main>`, `<init>`).
    pub name: String,
    pub vars: Vec<Var>,
    /// Number of parameters (at the start of `vars`).
    pub param_count: u32,
    /// Return type (`None` for procedures). `Result` is `vars[param_count]`.
    pub ret: Option<Ty>,
    pub body: Vec<Stmt>,
    /// Position of the line with the body's final `end` (or EOF) (for the line number of the final `Ret`).
    pub end_span: TextRange,
    /// Right after the body's `begin` (where `Result := 0;` is inserted). For a script's `<main>`, before the first statement.
    pub body_begin: Option<TextSize>,
    /// Position of the name in the heading (for diagnostics).
    pub name_span: TextRange,
    /// Contains syntax errors (control-flow analysis is skipped).
    pub has_syntax_errors: bool,
    /// Contains semantic errors (code generation is skipped).
    pub has_errors: bool,
}

impl Routine {
    pub fn result_var(&self) -> Option<VarId> {
        self.ret.map(|_| VarId(self.param_count))
    }
}

/// An assignment target.
#[derive(Debug, Clone)]
pub enum Place {
    /// A local variable (the second field is where it was written; used for `read here` in definite assignment).
    Var(VarId, TextRange),
    Global(GlobalId),
    /// `S[I]` (where `S` is a variable).
    StrIndex {
        base: Box<Place>,
        index: Box<Expr>,
        /// Position of `[` (the line for `ERangeError`).
        span: TextRange,
    },
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    /// Start position of the statement (for the line table and for looking up switch states).
    pub span: TextRange,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Assign {
        target: Place,
        value: Expr,
        /// Range check on assignment (`{$R+}` when storing into a narrow type).
        check: Checks,
        /// Initial value of a declaration (not counted as a reference for W0001).
        is_init: bool,
    },
    Block(Vec<Stmt>),
    /// A call whose value is discarded (including built-ins).
    Expr(Expr),
    If {
        /// A sequence of `(condition, body)` (an `else if` chain).
        branches: Vec<(Expr, Stmt)>,
        else_body: Option<Box<Stmt>>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    Repeat {
        body: Vec<Stmt>,
        cond: Expr,
    },
    For {
        var: ForVar,
        /// Position of `FOR_VAR` (for `assigned by this loop` in E2004).
        var_span: TextRange,
        from: Expr,
        to: Expr,
        downto: bool,
        body: Box<Stmt>,
        checks: Checks,
    },
    Break,
    Continue,
    /// `Exit` / `Exit(value)`.
    Exit(Option<Expr>),
    Halt(Option<Expr>),
    /// `Inc` / `Dec`.
    IncDec {
        target: Place,
        /// Type of the target (for `Char`, the range is 0–65535).
        target_ty: Ty,
        delta: Option<Expr>,
        negative: bool,
        /// Computation type.
        calc: Ty,
        checks: Checks,
    },
    /// `ReadLn([variable])`.
    ReadLn(Option<Place>),
    /// `Write` / `WriteLn`.
    Write {
        args: Vec<WriteArg>,
        newline: bool,
    },
    Assert {
        cond: Expr,
        message: Option<Expr>,
        /// Source text of the condition.
        text: String,
    },
    /// Does nothing (`Assert` under `{$C-}`, an empty `Write`).
    Nop,
}

#[derive(Debug, Clone)]
pub enum ForVar {
    Local(VarId),
    /// A global variable inside `<main>`. `counter` is a hidden variable.
    Global {
        id: GlobalId,
        counter: VarId,
    },
}

#[derive(Debug, Clone)]
pub struct WriteArg {
    pub value: Expr,
    pub width: Option<Expr>,
}

/// Switches in effect at that statement (`{$Q}` / `{$R}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checks {
    pub overflow: bool,
    pub range: bool,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
    /// The constant value (when folded).
    pub value: Option<Const>,
    pub span: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }

    pub fn text(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "div",
            BinOp::Mod => "mod",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::Xor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => "shr",
            BinOp::Eq => "=",
            BinOp::Ne => "<>",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
}

/// One step of a binary operation.
#[derive(Debug, Clone)]
pub struct BinStep {
    pub op: BinOp,
    pub rhs: Expr,
    /// Kind of operation (determined by the operand types).
    pub calc: Calc,
    /// Result type of this step.
    pub ty: Ty,
    /// Position of the operator.
    pub span: TextRange,
}

/// Kind of operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Calc {
    /// Integer operation (`calc` is the computation type; narrow types are computed in 64 bits and narrowed with the `Narrow` family).
    Int(Ty),
    Bool,
    /// String concatenation / comparison (a `Char` is converted to `string` first).
    Str,
    /// Comparison between `Char`s (by code unit value).
    Char,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    /// A literal or constant (value in `value`).
    Const,
    Var(VarId),
    Global(GlobalId),
    /// Left-folded sequence: `first op1 rhs1 op2 rhs2 …`.
    Binary {
        first: Box<Expr>,
        steps: Vec<BinStep>,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        calc: Calc,
    },
    /// `S[I]` (read).
    StrIndex {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// A routine call.
    Call {
        routine: RoutineId,
        args: Vec<CallArg>,
    },
    /// A built-in call.
    Builtin {
        func: Builtin,
        args: Vec<Expr>,
    },
    /// Type conversion (implicit conversions for assignment compatibility, and explicit `T(expr)`).
    Convert {
        operand: Box<Expr>,
        kind: Conversion,
    },
    /// An expression whose type could not be determined.
    Error,
}

#[derive(Debug, Clone)]
pub enum CallArg {
    Value(Expr),
    /// Actual argument for a `var` / `out` parameter.
    Ref {
        place: Place,
        is_out: bool,
    },
}

/// Built-in functions (those that return a value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Length,
    Copy,
    Pos,
    Ord,
    Chr,
    Abs,
    ParamCount,
    ParamStr,
    IntToStr,
    UIntToStr,
    StrToInt,
    StrToInt64,
    StrToIntDef,
    UpperCase,
    LowerCase,
    Trim,
    GetEnvironmentVariable,
}

/// Kind of conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    /// Integer type → integer type. `NarrowRng` if `check`, otherwise `Narrow` (nothing if the value fits in the result type).
    Int { to: Ty, check: bool },
    /// `Char` → `string`.
    CharToStr,
    /// Integer → `Char` (`Chr`, `Char(N)`). The range is that of `Word`.
    IntToChar { check: bool },
    /// Integer → `Boolean` (`Boolean(N)`; nonzero is `True`).
    IntToBool,
    /// `Char` / `Boolean` → integer (the ordinal as is; does nothing).
    OrdToInt,
}
