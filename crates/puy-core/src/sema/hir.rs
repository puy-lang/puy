//! HIR: the tree with names and types resolved (`docs/design/compiler-pipeline.md` 3.4).
//!
//! Left-leaning chains (chains of left operands of binary operators, `else if` chains) are stored as sequences.

use super::types::{Const, Ty, TypeTable};
use crate::source::{FileId, TextRange, TextSize};
use crate::syntax::ast::ParamMode;

/// The whole program.
#[derive(Debug)]
pub struct Program {
    /// Program name (prefix of the routines' fully qualified names).
    pub name: String,
    pub globals: Vec<Global>,
    /// The `System` global `ExitCode` (`docs/spec/semantics.md` 5.3), if the program uses it.
    pub exit_code: Option<GlobalId>,
    /// Routines (in declaration order; a `forward` routine is placed at its first heading).
    pub routines: Vec<Routine>,
    /// `<main>`.
    pub main: Routine,
    /// `<init>` (assigns initial values to global variables). `None` if there are none.
    pub init: Option<Routine>,
    /// Whether `System.SysUtils` is in `uses`.
    pub uses_sysutils: bool,
    /// Records and static arrays (`docs/spec/types.md` 1.2).
    pub types: TypeTable,
    /// The units of the compilation in initialization order (`docs/spec/units.md` 5.2); the
    /// file passed to `puy` is the last. Routines and globals refer to them by index.
    pub units: Vec<UnitHir>,
    /// The `type` sections to show in the `hir` dump, in declaration order: (routine index or
    /// `None` for the program, name, type).
    pub type_decls: Vec<TypeDeclInfo>,
}

/// A unit of the compilation, or the program (`docs/spec/units.md` chapter 5).
#[derive(Debug)]
pub struct UnitHir {
    /// The name (the unit's name as written in its heading, or the program's name).
    pub name: String,
    pub file: FileId,
    /// A program or a script (`<main>` is `Program::main`), as opposed to a unit.
    pub is_program: bool,
    /// `<init>`: the initial values of the unit's global variables, then the `initialization`
    /// statements. For the program it is `Program::init`.
    pub init: Option<Routine>,
    /// `<fini>`: the `finalization` statements of a unit.
    pub fini: Option<Routine>,
    /// The unit's global variables: they are numbered `first_global..first_global + global_count`
    /// (`docs/spec/units.md` 6.2, 6.3; includes `ExitCode` if this file used it first).
    pub first_global: u32,
    pub global_count: u32,
    /// The units named in `uses`, as written (for the `hir` dump).
    pub uses: Vec<String>,
}

/// A declared type for the `hir` dump (`docs/spec/semantics.md` 9.1).
#[derive(Debug, Clone)]
pub struct TypeDeclInfo {
    /// The unit declaring it (index into `Program::units`).
    pub unit: u32,
    /// `None` for a type of the program; otherwise the routine's short name.
    pub routine: Option<String>,
    pub name: String,
    /// The type the name denotes.
    pub ty: Ty,
    /// The declaration defines this type (a `record` / `array`), as opposed to naming another type.
    pub defines: bool,
    /// A dynamic array declared as `TArray<T>` (the `hir` dump shows the written form).
    pub generic: bool,
}

#[derive(Debug, Clone)]
pub struct Global {
    pub name: String,
    pub ty: Ty,
    /// The declaring identifier (`None` for `ExitCode`).
    pub span: Option<TextRange>,
    /// The unit declaring it (`None` for `ExitCode` of `System`).
    pub unit: Option<u32>,
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
    /// The `E` of `on E: T do` (`docs/spec/exceptions.md` 5.2). It borrows the exception the
    /// `except` block holds, so it is not released on its own.
    Exception,
    /// A variable of an enclosing routine that a nested routine uses (`docs/spec/semantics.md`
    /// 4.9): variable `var` of the routine `up` levels out. It holds a reference to that
    /// variable, made when the nested routine is entered, and is used like a `var` parameter.
    Outer {
        up: u32,
        var: VarId,
    },
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
    /// Used by a nested routine (`docs/spec/semantics.md` 4.9): a `const` argument is not lent
    /// without `AddRef` (`docs/design/lifetime.md` 3.3), and W0002 does not look at it.
    pub captured: bool,
    /// Read by a nested routine (a read for W0001 and W0017).
    pub read_by_nested: bool,
    /// Where a nested routine first writes it (it cannot be a `for` loop variable; E0707).
    pub written_by_nested: Option<TextRange>,
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
    /// The unit declaring it (index into `Program::units`).
    pub unit: u32,
    /// The routine it is nested in (`docs/spec/semantics.md` 4.9).
    pub parent: Option<RoutineId>,
    /// Takes the static link as a hidden first argument (`docs/spec/semantics.md` 4.9). Set by
    /// `sema::finish`.
    pub link: bool,
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
    /// `R.F` (a field of a record place).
    Field {
        base: Box<Place>,
        /// The field's number (0-based, declaration order).
        index: u32,
        /// The field's type.
        ty: Ty,
        /// The field name (for the dump).
        name: String,
    },
    /// `A[I]` (an element of a static array, dynamic array or open array place).
    Elem {
        base: Box<Place>,
        index: Box<Expr>,
        /// The element type.
        ty: Ty,
        /// Position of `[` (the line for `ERangeError`).
        span: TextRange,
    },
    /// `E.Message` written to (`docs/spec/exceptions.md` 2.1): the exception object is a reference,
    /// so the place reads `base` and writes the object's slot.
    ExcMember {
        base: Box<Expr>,
        member: ExcMember,
    },
}

/// A member of an exception object (`docs/spec/exceptions.md` 2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExcMember {
    Message,
    StackTrace,
    Suppressed,
    InnerException,
    ClassName,
    /// `EConvertError.Value`.
    Value,
    /// `EInOutError.ErrorCode`.
    ErrorCode,
}

impl ExcMember {
    pub fn name(self) -> &'static str {
        match self {
            ExcMember::Message => "Message",
            ExcMember::StackTrace => "StackTrace",
            ExcMember::Suppressed => "Suppressed",
            ExcMember::InnerException => "InnerException",
            ExcMember::ClassName => "ClassName",
            ExcMember::Value => "Value",
            ExcMember::ErrorCode => "ErrorCode",
        }
    }
}

impl Place {
    /// The variable or global the place starts from.
    pub fn root(&self) -> &Place {
        match self {
            Place::StrIndex { base, .. } | Place::Field { base, .. } | Place::Elem { base, .. } => {
                base.root()
            }
            _ => self,
        }
    }
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
        /// The hidden variable counting the iterations when the loop variable is a global.
        counter: Option<VarId>,
    },
    /// `for X in C` over an array or a string (`docs/spec/types.md` chapter 5).
    ForIn {
        var: ForVar,
        /// Position of `FOR_VAR` (for `assigned by this loop` in E2004).
        var_span: TextRange,
        /// The collection, evaluated once on entry.
        coll: Expr,
        /// The hidden variable holding the collection during the loop.
        coll_var: VarId,
        /// The hidden variable with the position of the next element (0-based for arrays,
        /// 1-based for strings).
        idx_var: VarId,
        /// The element type (`Char` for a string).
        elem: Ty,
        /// The conversion of an element to the loop variable's type (assignment compatibility).
        conv: Option<Conversion>,
        body: Box<Stmt>,
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
    /// `Read(variables)` / `ReadLn([variables])` (`docs/spec/semantics.md` 6.2).
    Read {
        targets: Vec<Place>,
        newline: bool,
    },
    /// `Write` / `WriteLn`.
    Write {
        args: Vec<WriteArg>,
        newline: bool,
        /// Written to standard error (`Write(ErrOutput, …)`; `docs/spec/semantics.md` 6.1).
        err: bool,
    },
    /// `Flush(Output)`: writes out the standard output buffer.
    Flush,
    Assert {
        cond: Expr,
        message: Option<Expr>,
        /// Source text of the condition.
        text: String,
    },
    /// `SetLength(target, N[, M…])` (`docs/spec/types.md` 2.3, chapter 7): a string or a dynamic
    /// array; one length per dimension of nested dynamic arrays.
    SetLength {
        target: Place,
        lens: Vec<Expr>,
    },
    /// `raise X` (`docs/spec/exceptions.md` 5.1). `None` for `raise;` (re-raises the exception of the
    /// lexically innermost `except` block).
    Raise(Option<Expr>),
    /// `Exception.RaiseOuterException(E)` inside an `except` block (`docs/spec/exceptions.md` 5.4).
    RaiseOuter(Expr),
    /// `try..except` / `try..finally` (`docs/spec/exceptions.md` 5.2, 5.3).
    Try {
        body: Vec<Stmt>,
        handler: TryHandler,
    },
    /// `case` (`docs/spec/semantics.md` 4.10). The selector is evaluated once; the labels are
    /// compared in order, and the first arm with a matching label runs.
    Case {
        selector: Expr,
        arms: Vec<CaseArm>,
        /// The `else` part, if any.
        else_body: Option<Vec<Stmt>>,
    },
    /// `Include(S, X)` / `Exclude(S, X)` (`docs/spec/types.md` 8.3): `X` is the ordinal value;
    /// `check` (`{$R+}`) checks it against the set's range first.
    SetElem {
        target: Place,
        elem: Expr,
        exclude: bool,
        check: bool,
    },
    /// `Insert(source, target, index)` (`docs/spec/semantics.md` 5.3, `docs/spec/types.md` 8.6).
    /// The source has the target's type (a string), or is an element of the dynamic array or,
    /// with `whole`, an array of the same element type.
    Insert {
        source: Expr,
        target: Place,
        index: Expr,
        whole: bool,
    },
    /// `Delete(target, index, count)`.
    Delete {
        target: Place,
        index: Expr,
        count: Expr,
    },
    /// Does nothing (`Assert` under `{$C-}`, an empty `Write`).
    Nop,
}

/// `CaseLabel {, CaseLabel} : Statement`.
#[derive(Debug, Clone)]
pub struct CaseArm {
    pub labels: Vec<CaseLabel>,
    pub body: Option<Box<Stmt>>,
}

/// A label of a `case` arm: a constant of the selector's type, or a range of them.
#[derive(Debug, Clone)]
pub struct CaseLabel {
    pub low: Expr,
    pub high: Option<Expr>,
}

/// What follows the body of a `try`.
#[derive(Debug, Clone)]
pub enum TryHandler {
    /// `except`. A bare `except` has no handlers and its statements in `else_body`; with handlers,
    /// `else_body` is the `else` part, if any.
    Except {
        handlers: Vec<Handler>,
        else_body: Option<Vec<Stmt>>,
    },
    Finally(Vec<Stmt>),
}

/// `on [E:] T do Statement`.
#[derive(Debug, Clone)]
pub struct Handler {
    /// The exception variable (`VarKind::Exception`).
    pub var: Option<VarId>,
    /// The exception class (`Ty::Exception`, or `Ty::Error` after E0733).
    pub class: Ty,
    pub body: Option<Box<Stmt>>,
    /// Position of `on` (the line of the dispatch).
    pub span: TextRange,
}

#[derive(Debug, Clone, Copy)]
pub enum ForVar {
    Local(VarId),
    /// A global variable inside `<main>`.
    Global(GlobalId),
}

#[derive(Debug, Clone)]
pub struct WriteArg {
    pub value: Expr,
    pub width: Option<Expr>,
    /// `D` of `X:W:D` (a real or `Currency`; `docs/spec/semantics.md` 6.1).
    pub precision: Option<Expr>,
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
    /// `/` (`docs/spec/types.md` 8.4): always a real or `Currency` division.
    Slash,
    Mod,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    /// `X in S` (`docs/spec/types.md` 8.3).
    In,
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
            BinOp::Slash => "/",
            BinOp::Mod => "mod",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::Xor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => "shr",
            BinOp::In => "in",
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
    /// Concatenation / comparison of two `UTF8String`s (by bytes; `docs/spec/types.md` 8.5).
    U8,
    /// Comparison between `Char`s (by code unit value).
    Char,
    /// Comparison between values of the same enumeration type (by ordinal; `docs/spec/types.md` 8.1).
    Ord,
    /// `=` / `<>` of a dynamic array and `nil` (`docs/spec/types.md` 2.3): the references compared.
    Ref,
    /// Set operations and comparisons (`docs/spec/types.md` 8.3). For `+`, `-`, `*` both operands
    /// have the result's type; a comparison may have a small and a big set of different types.
    Set,
    /// `X in S`: the right operand is a set, the left one an ordinal.
    SetIn,
    /// A real operation in `Single` or `Double` (`docs/spec/types.md` 8.4): both operands have that
    /// type (IEEE 754; no `{$Q}` / `{$R}`).
    Real(Ty),
    /// A `Currency` operation: both operands are `Currency` (overflow is always `EOverflow`).
    Curr,
    /// `+` of two dynamic arrays of one element type (`docs/spec/types.md` 8.6): a new dynamic
    /// array (the step's type).
    Arr,
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
    /// `R.F` (read; `docs/spec/types.md` 2.1).
    Field {
        base: Box<Expr>,
        /// The field's number (0-based, declaration order).
        index: u32,
        name: String,
    },
    /// `A[I]` (read; an element of a static array, `docs/spec/types.md` 2.2, or of a dynamic array
    /// or open array, 2.3, 2.4: the base's type tells which).
    Elem {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// `X[L..H]` (`docs/spec/types.md` 8.7): a copy of part of a string or an array (the
    /// expression's type is the string type or the element type's dynamic array). A bound left out
    /// is `None`; the bounds are `Int64`, or the index type of a static array whose index type is
    /// not an integer.
    Slice {
        base: Box<Expr>,
        low: Option<Box<Expr>>,
        high: Option<Box<Expr>>,
    },
    /// An array constructor `[…]` (`docs/spec/types.md` chapter 4); the expression's type is the
    /// array type the context gives (a dynamic array for an open array parameter).
    ArrayCtor(Vec<Expr>),
    /// A set constructor `[E, L..H]` (`docs/spec/types.md` 8.3) of the expression's set type.
    SetCtor(Vec<SetCtorElem>),
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
    /// `T.Create(Msg)`: a new exception object of class `class` (`docs/spec/exceptions.md` 5.6;
    /// the expression's type may be an ancestor after a conversion).
    NewExc {
        class: Ty,
        msg: Box<Expr>,
    },
    /// A member of an exception object read (`docs/spec/exceptions.md` 2.1).
    ExcMember {
        base: Box<Expr>,
        member: ExcMember,
    },
    /// `TryStrToInt(S, Value)` (`docs/spec/semantics.md` 5.3): `Value` is an `out` argument of
    /// type `Integer`.
    TryStrToInt {
        s: Box<Expr>,
        target: Place,
    },
    /// An expression whose type could not be determined.
    Error,
}

/// An element of a set constructor: the ordinal `low`, or the range `low..high`. `check` (`{$R+}`)
/// checks a non-empty range (or the element) against the set's range.
#[derive(Debug, Clone)]
pub struct SetCtorElem {
    pub low: Expr,
    pub high: Option<Expr>,
    pub check: bool,
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
    /// `Length` of a string or an array (`docs/spec/types.md` chapter 7).
    Length,
    /// `Copy` of a string, or of an array (`docs/spec/types.md` chapter 7; 1 to 3 arguments).
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
    /// `Default(T)` (`docs/spec/types.md` chapter 7): the default value of the expression's type.
    Default,
    /// `Low(X)` / `High(X)`. Folded into a constant by semantic analysis (static arrays and
    /// ordinal types; `docs/spec/types.md` chapter 7), except `High` of a dynamic or open array.
    Low,
    High,
    /// `Assigned(X)` of a dynamic array or an exception: `X <> nil` (`docs/spec/types.md` chapter 7,
    /// `docs/spec/exceptions.md` chapter 1).
    Assigned,
    /// `Succ(X)` / `Pred(X)` (`docs/spec/semantics.md` 5.3). Only a name: sema lowers the call to
    /// `X + 1` / `X - 1` converted back to the type of `X`.
    Succ,
    Pred,
    /// `Sqrt(X)` of a `Double` (`docs/spec/semantics.md` 5.3).
    Sqrt,
    /// `Sqr(X)`: `X * X` in the type of `X` (integers: the computation type of `*`); lower emits
    /// the multiplication, evaluating `X` once.
    Sqr,
    /// `Pi`: folded into a constant.
    Pi,
    /// `Trunc(X)` / `Round(X)` of a real or a `Currency` (the argument keeps its type).
    Trunc,
    Round,
    /// `FloatToStr(X)` of a `Single` or a `Double` (the argument keeps its type).
    FloatToStr,
    StrToFloat,
    /// `Odd(X)` of an integer: `X and 1` (`docs/spec/semantics.md` 5.3).
    Odd,
    /// `UpCase(C)` of a `Char` (ASCII letters only).
    UpCase,
    /// `SizeOf(T)`: only a name; folded into a constant.
    SizeOf,
    /// `Concat(…)`: only a name; sema builds the `+` chain.
    Concat,
    /// `Format(Fmt, […])` (`docs/spec/semantics.md` 5.3): the format, then the elements of the
    /// `array of const`, each of its own type.
    Format,
    /// `IntToHex(X, Digits)`: `X` of any integer type (its width decides negative values).
    IntToHex,
    StrToBool,
    /// `BoolToStr(B[, UseBoolStrs])`: always with both arguments (sema adds `False`).
    BoolToStr,
    TrimLeft,
    TrimRight,
    SameText,
    CompareText,
    CompareStr,
    /// `StringReplace(S, Old, New, Flags)` (`Flags` of `TReplaceFlags`).
    StringReplace,
    StringOfChar,
    QuotedStr,
    /// Only while resolving: a call becomes [`ExprKind::TryStrToInt`].
    TryStrToInt,
    /// `AnsiUpperCase(S[, Mapping])` / `AnsiLowerCase(S[, Mapping])` (`docs/spec/semantics.md`
    /// 5.3): always with both arguments (sema adds `TCaseMapping.cmSimple`).
    AnsiUpperCase,
    AnsiLowerCase,
    /// `AnsiSameText(S1, S2[, Fold])` / `AnsiCompareText(S1, S2[, Fold])`: always with the three
    /// arguments (sema adds `TCaseFold.cfLower`).
    AnsiSameText,
    AnsiCompareText,
    /// `Eof` / `Eoln` of standard input (`docs/spec/semantics.md` 6.2).
    Eof,
    Eoln,
}

/// Kind of conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    /// Integer type → integer type. `NarrowRng` if `check`, otherwise `Narrow` (nothing if the value fits in the result type).
    Int { to: Ty, check: bool },
    /// `Char` → `string`.
    CharToStr,
    /// `string` → `UTF8String` (`docs/spec/types.md` 8.5): lone surrogates become U+FFFD.
    StrToU8,
    /// `UTF8String` → `string` (lossless).
    U8ToStr,
    /// Integer → `Char` (`Chr`, `Char(N)`). The range is that of `Word`.
    IntToChar { check: bool },
    /// Integer → `Boolean` (`Boolean(N)`; nonzero is `True`).
    IntToBool,
    /// `Char` / `Boolean` / an enumeration → integer (the ordinal as is; does nothing).
    OrdToInt,
    /// A set → the set type `to` (`docs/spec/types.md` 8.3): another representation or size;
    /// `check` (`{$R+}`) checks that every element is in the range of `to`.
    Set { to: Ty, check: bool },
    /// An ordinal → the enumeration or subrange type `to` (`docs/spec/types.md` 8.1, 8.2): the
    /// value is kept; `check` (`{$R+}`) checks it against the range of `to` (`ChkRange`).
    Range { to: Ty, check: bool },
    /// An integer, a real or a `Currency` → the real or `Currency` type `to` (`docs/spec/types.md`
    /// 8.4): rounded to nearest even; to `Currency` out of range is `EOverflow`.
    Num { to: Ty },
}
