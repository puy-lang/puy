//! HIR: the tree with names and types resolved (`docs/design/compiler-pipeline.md` 3.4).
//!
//! Left-leaning chains (chains of left operands of binary operators, `else if` chains) are stored as sequences.

use super::types::{Const, Ty, TypeTable};
use crate::source::{FileId, TextRange, TextSize};
use crate::syntax::ast::ParamMode;

/// The whole program.
#[derive(Debug, Clone)]
pub struct Program {
    /// Program name (prefix of the routines' fully qualified names).
    pub name: String,
    pub globals: Vec<Global>,
    /// The `class var`s of the generic types (`docs/spec/generics.md` 2.5), which only the
    /// bodies of the generic types use: numbered after `globals`, they have no global variable
    /// (each instantiation has its own in `globals`).
    pub generic_globals: Vec<Global>,
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
    /// The attribute classes written on declarations (`docs/spec/attributes.md` 7), shown by the
    /// `hir` dump only.
    pub attrs: Vec<AttrUse>,
    /// The declarations marked `deprecated` / `experimental` (`docs/spec/attributes.md` 6, 7),
    /// shown by the `hir` dump only.
    pub hints: Vec<HintMark>,
    /// The instantiations of generic types, in the order they were asked for
    /// (`docs/spec/generics.md` 8).
    pub instances: Vec<InstanceInfo>,
    /// The routines the analysis made, in the order they were made: the generated `Equals` of
    /// the reference counted classes and the generated `Equals` / `GetHashCode` of the
    /// `[ValueEquality]` classes (`docs/spec/equality.md` 8.3), the generated `<eq:…>` /
    /// `<hash:…>` functions, and those of the instantiations with a body. Their functions come
    /// after every unit's (`docs/spec/generics.md` 7).
    pub instance_order: Vec<RoutineId>,
    /// The generated `<eq:…>` / `<hash:…>` functions (`docs/spec/equality.md` 8.3), in the order
    /// they were made, for the `hir` dump (9).
    pub generated: Vec<GeneratedInfo>,
    /// Where each routine of an instantiation that the work list copied comes from: the
    /// functions made from the same generic routine may be shared (`docs/spec/generics.md` 7).
    pub origins: std::collections::HashMap<RoutineId, GenericOrigin>,
}

/// Where a routine of an instantiation comes from (`docs/spec/generics.md` 7, 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericOrigin {
    /// The generic routine its body was copied from: only functions with the same one are
    /// shared.
    pub def: RoutineId,
    /// The full name of the generic routine, with the declared type parameters
    /// (`Hello.TList<T>.Add`): the name of a shared function.
    pub generic_name: String,
    /// The full name of the instantiation it belongs to (`Hello.TList<Hello.TFoo>`,
    /// `Hello.Max<Integer>`), for the `; shared by` line.
    pub instance: String,
}

/// A generated `<eq:…>` / `<hash:…>` function for the `hir` dump (`docs/spec/equality.md` 9).
#[derive(Debug, Clone)]
pub struct GeneratedInfo {
    /// The unit whose routine asked for it first.
    pub unit: u32,
    pub routine: RoutineId,
}

/// An instantiation of a generic type for the `hir` dump (`docs/spec/generics.md` 8).
#[derive(Debug, Clone)]
pub struct InstanceInfo {
    /// The name in diagnostics (`TBox<Integer>`).
    pub name: String,
    /// The unit whose analysis asked for it first.
    pub unit: u32,
    /// Its routines with a body, in the order they were made.
    pub routines: Vec<RoutineId>,
}

/// The hint directives of a declaration, as the `hir` dump shows them.
#[derive(Debug, Clone)]
pub struct HintMark {
    /// The declaration's name.
    pub decl: crate::source::Span,
    pub deprecated: bool,
    pub experimental: bool,
}

/// An attribute class written on a declaration (`docs/spec/attributes.md` 4.2, 7): checked where
/// it is written, but no instance is created (M3).
#[derive(Debug, Clone)]
pub struct AttrUse {
    /// The unit declaring it (index into `Program::units`).
    pub unit: u32,
    /// The name of the declaration it is written on (a declaration of several names has one
    /// `AttrUse` per name).
    pub decl: TextRange,
    /// The fully qualified name of the attribute class.
    pub class: String,
    /// The constructor chosen; `None` for the built-in `TObject.Create`.
    pub ctor: Option<RoutineId>,
    /// The arguments, constants or class references.
    pub args: Vec<Expr>,
}

/// A unit of the compilation, or the program (`docs/spec/units.md` chapter 5).
#[derive(Debug, Clone)]
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

/// A declared type for the `hir` dump (`docs/spec/semantics.md` chapter 9).
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
    /// The declared name.
    pub span: TextRange,
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

impl Program {
    /// A global variable, or a `class var` of a generic type (numbered after the globals).
    pub fn global(&self, id: GlobalId) -> &Global {
        let i = id.0 as usize;
        match self.globals.get(i) {
            Some(g) => g,
            None => &self.generic_globals[i - self.globals.len()],
        }
    }
}

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
    /// `Self` of a method (`docs/spec/objects.md` 2.3): the first parameter. It borrows the
    /// instance (`docs/spec/objects.md` 4.1) and cannot be assigned.
    SelfParam,
    /// `Self` of a method of a record (`docs/spec/objects.md` chapter 6): the first parameter,
    /// passed like a `var` parameter (its register holds a reference slot), and it can be
    /// assigned.
    RecordSelf,
    /// A variable of an enclosing routine that a nested routine uses (`docs/spec/semantics.md`
    /// 4.9): variable `var` of the routine `up` levels out. It holds a reference to that
    /// variable, made when the nested routine is entered, and is used like a `var` parameter.
    Outer {
        up: u32,
        var: VarId,
    },
    /// Only in the `ir` dump's registers: the register holding the cell of the variable `var`
    /// (`docs/spec/objects.md` 14.9), which it owns (`cell`) or borrows (`capture`).
    Cell {
        var: VarId,
        owned: bool,
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
    /// Written in its routine or a nested one: assigned, passed to a `var` / `out` parameter, a
    /// field or element of it written, or a loop variable (what is E0707 for a `const`
    /// parameter). A managed value parameter that is not written (nor captured) borrows its
    /// argument like a `const` one (`docs/design/lifetime.md` 3.3).
    pub written: bool,
    /// Captured by an anonymous method (`docs/spec/objects.md` 9.6): its value lives in a cell
    /// (14.9).
    pub cell: bool,
}

#[derive(Debug, Clone)]
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
    /// An abstract method (`docs/spec/objects.md` 2.3): it has no body and no function.
    pub is_abstract: bool,
    /// A method of a built-in class (`TObject.ToString`): `System`'s, named without a unit.
    pub builtin: bool,
    /// A method of a generic type, or a routine nested in one (`docs/spec/generics.md` 3.3):
    /// checked once with its type parameters, and it has no function (7). Set by
    /// `sema::finish`.
    pub generic: bool,
    /// The class whose destructor this is (`docs/spec/objects.md` 3.2): its body starts with
    /// `DtorEnter` (14.8).
    pub dtor_of: Option<super::types::TypeId>,
    /// A method of a helper (`docs/spec/objects.md` 10.8): its calls are `helper-call` in the
    /// `hir` dump (15.6).
    pub helper: bool,
    /// A `class operator` (`docs/spec/objects.md` chapter 11): its calls are `operator` /
    /// `convert-op` in the `hir` dump (15.6).
    pub operator: bool,
    /// The function of an anonymous method (`docs/spec/objects.md` 9.6): it takes the closure as
    /// its hidden first argument (14.9).
    pub closure: bool,
    /// The variables (of kind `Outer`) a closure's function takes from its closure, in the order
    /// of its captured cells (`CapGet`).
    pub captures: Vec<VarId>,
    /// The group blocks of closure-form nested routines (`docs/spec/objects.md` 9.7, 14.9).
    pub groups: GroupBlocks,
    /// A routine of an instantiation of a generic type: its full name (`Hello.TBox<Integer>.Get`;
    /// `docs/spec/generics.md` 4.1). Its function comes after every unit's (7).
    pub instance: Option<String>,
}

/// What a routine does for the group blocks of closure-form nested routines
/// (`docs/spec/objects.md` 9.7, 14.9).
#[derive(Debug, Clone, Default)]
pub struct GroupBlocks {
    /// The group blocks the routine makes on entry, after the cells: the hidden variable holding
    /// the block, and the variables of this routine whose cells the block captures, in order.
    pub made: Vec<(VarId, Vec<VarId>)>,
    /// A closure-form nested routine: the index in its group's block of each of `captures`.
    pub slots: Vec<u32>,
    /// A closure-form nested routine: the hidden variable its own block is copied to on entry,
    /// for the routines nested in it that call a routine of its group.
    pub own: Option<VarId>,
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
    /// `O.F`: a field of a class instance (`docs/spec/objects.md` 2.2). The instance is shared, so
    /// the place reads `base` (the handle) and reads or writes the instance's slot; a field or
    /// element place inside it has this place as its root.
    ObjField {
        base: Box<Expr>,
        /// The slot number (the parent's fields first).
        slot: u32,
        ty: Ty,
        name: String,
    },
}

/// A member of an exception object that the VM keeps outside its slots
/// (`docs/spec/exceptions.md` 2.1, `docs/spec/objects.md` 14.8): read with `ExcInfo`. The other
/// members are properties of the built-in classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExcMember {
    StackTrace,
    Suppressed,
}

impl ExcMember {
    pub fn name(self) -> &'static str {
        match self {
            ExcMember::StackTrace => "StackTrace",
            ExcMember::Suppressed => "Suppressed",
        }
    }

    /// The member named `lower` (lowercase).
    pub fn from_name(lower: &str) -> Option<ExcMember> {
        match lower {
            "stacktrace" => Some(ExcMember::StackTrace),
            "suppressed" => Some(ExcMember::Suppressed),
            _ => None,
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
    /// `for X in C` over an array, a string or a set (`docs/spec/types.md` chapter 5), or with
    /// `GetEnumerator` (`docs/spec/objects.md` chapter 13).
    ForIn {
        var: ForVar,
        /// Position of `FOR_VAR` (for `assigned by this loop` in E2004).
        var_span: TextRange,
        /// The collection, evaluated once on entry.
        coll: Expr,
        /// The hidden variable holding the collection during the loop.
        coll_var: VarId,
        /// The hidden variable with the position of the next element (0-based for arrays,
        /// 1-based for strings), or the enumerator with `enumerator`.
        idx_var: VarId,
        /// The element type (`Char` for a string).
        elem: Ty,
        /// The conversion of an element to the loop variable's type (assignment compatibility).
        conv: Option<Conversion>,
        body: Box<Stmt>,
        /// The calls of the enumerator when the collection has no iteration of its own.
        enumerator: Option<Box<Enumerator>>,
    },
    /// The declaration of an inline variable without an initial value: a captured one gets a new
    /// cell each time it runs (`docs/spec/objects.md` 9.6).
    Declare(VarId),
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
    /// `Exchange(target, first, second)`: the indices are 0-based for a dynamic array and as
    /// written (of the index type) for a static array (`docs/spec/types.md` chapter 7).
    Exchange {
        target: Place,
        first: Expr,
        second: Expr,
        /// The call (the line for `ERangeError`).
        span: TextRange,
    },
    /// `ArrayMoveRange(target, from, to, count)` (`docs/spec/bytecode.md` 2.22; the units of the
    /// standard library only): the `count` elements at `from` of the dynamic array move to `to`,
    /// the elements between shifting into the room they leave (a rotation; no count changes).
    ArrayMoveRange {
        target: Place,
        from: Expr,
        to: Expr,
        count: Expr,
    },
    /// `FreeIfClass(value)` (`docs/spec/collections.md` 12.1; the units of the standard library
    /// only): `value.Free` when its type is a class type, only its evaluation otherwise.
    FreeIfClass {
        value: Expr,
    },
    /// `CheckDefaultOrder(value)` (`docs/spec/collections.md` 10.2, 12.1; the units of the
    /// standard library only): only the evaluation of `value` when its type has a default order,
    /// `EInvalidOperation` as from `KeyCompare` otherwise.
    CheckDefaultOrder {
        value: Expr,
    },
    /// `CheckClassType(value, what)` (`docs/spec/collections.md` 9.3, 12.1; the units of the
    /// standard library only): only the evaluation of `value` and `what` when the type of `value`
    /// is a class type, `EInvalidOperation` with `what` and the name of the type otherwise.
    CheckClassType {
        value: Expr,
        what: Expr,
    },
    /// `Delete(target, index, count)`.
    Delete {
        target: Place,
        index: Expr,
        count: Expr,
    },
    /// `O.P := X` with a property whose `write` is a method (`docs/spec/objects.md` 2.6): the
    /// call of the method, whose arguments are the receiver (none for a static method), the
    /// `index N` value, the indices and last the value. They are evaluated in that order.
    PropertySet {
        /// The property's name (for the dump).
        name: String,
        call: Expr,
    },
    /// `O.Free` / `O.Destroy` (`docs/spec/objects.md` 3.2): the destruction of the instance
    /// `target` refers to. `Free` does nothing for `nil`; `Destroy` raises `EAccessViolation`.
    Free {
        target: Expr,
        destroy: bool,
    },
    /// `FreeAndNil(target)` (`docs/spec/objects.md` 3.2): the value is copied, `target` set to
    /// `nil`, then the copy freed. A field or element is taken out and back like a `var`
    /// argument.
    FreeAndNil {
        target: Place,
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

/// The enumerator of a `for..in` (`docs/spec/objects.md` 13.2): its calls, each with a hidden
/// variable as the receiver.
#[derive(Debug, Clone)]
pub struct Enumerator {
    /// `C.GetEnumerator` on the collection's hidden variable.
    pub get: Expr,
    /// `E.MoveNext` on the enumerator's hidden variable.
    pub move_next: Expr,
    /// `E.Current`, converted to the loop variable's type.
    pub current: Expr,
    /// For a global loop variable, the hidden variable holding the element given to it last, which
    /// it gets again when the loop ends (`docs/spec/semantics.md` 4.5).
    pub last: Option<VarId>,
    pub checks: Checks,
    pub cleanup: EnumCleanup,
}

/// How a `for..in` lets go of its enumerator, by the enumerator's static type
/// (`docs/spec/objects.md` 13.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumCleanup {
    /// A class managed by hand: `Free`.
    Free,
    /// A reference counted class or a record: the hidden variable lets go of it.
    Release,
    /// An interface or exactly `TObject`: by its contents, either of the two.
    ByContents,
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
    /// `=` / `<>` of two values of a method reference or anonymous method type, or of one and
    /// `nil` (`CallableEqual`; `docs/spec/objects.md` 9.5). Two procedural type values compare
    /// as `Calc::Ref` (the function indexes).
    Callable,
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
    /// A range written as an index of a default array property, `X[L..H]`
    /// (`docs/spec/objects.md` 12.3, 14.10): a new `TIndexRange` with `First` / `Last` from the
    /// bounds written (`Integer`) and `HasFirst` / `HasLast` telling which were.
    IndexRange {
        first: Option<Box<Expr>>,
        last: Option<Box<Expr>>,
    },
    /// An array constructor `[…]` (`docs/spec/types.md` chapter 4); the expression's type is the
    /// array type the context gives (a dynamic array for an open array parameter).
    ArrayCtor(Vec<Expr>),
    /// A set constructor `[E, L..H]` (`docs/spec/types.md` 8.3) of the expression's set type.
    SetCtor(Vec<SetCtorElem>),
    /// A routine call. The call of a method passes the receiver (`Self`) as the first argument.
    Call {
        routine: RoutineId,
        args: Vec<CallArg>,
        dispatch: Dispatch,
    },
    /// `O.ClassName` (`docs/spec/objects.md` chapter 5): the name of the instance's class.
    ClassName {
        base: Box<Expr>,
    },
    /// A built-in call.
    Builtin {
        func: Builtin,
        args: Vec<Expr>,
    },
    /// The key equality `KeyEquals(A, B)` or the key hash `KeyHash(X)` of the arguments' type
    /// (`docs/spec/equality.md` 1.1, 8.3), expanded by its kind ([`super::types::KeyKind`]), or
    /// the default order `KeyCompare(A, B)` (`docs/spec/collections.md` 10.2), expanded by
    /// [`super::types::OrderKind`]. `func` is the routine a record or an array calls (the
    /// generated `<eq:…>` / `<hash:…>`, or a record's own `Equal` / `GetHashCode`), or the
    /// generated `<cmp:…>` of a type with its own `LessThan`; `None` until the type has no type
    /// parameter in it and the analysis has found it (`sema::finish`).
    Key {
        op: KeyOp,
        args: Vec<Expr>,
        func: Option<RoutineId>,
    },
    /// Type conversion (implicit conversions for assignment compatibility, and explicit `T(expr)`).
    Convert {
        operand: Box<Expr>,
        kind: Conversion,
    },
    /// A member of an exception object read (`docs/spec/exceptions.md` 2.1).
    ExcMember {
        base: Box<Expr>,
        member: ExcMember,
    },
    /// `TFoo.Create(…)`: a new instance of `class` with every field at its default value, on which
    /// the constructor `ctor` runs with `args` (after `Self`), then `AfterConstruction`
    /// (`docs/spec/objects.md` 3.1). `ctor` is `None` for the built-in constructors: `TObject.Create`,
    /// which does nothing (no argument), and `Exception.Create(Msg)` / `CreateFmt`, whose one
    /// argument is the message written to `Message` (8.2); `after` is whether `AfterConstruction`
    /// of the class is not the built-in one.
    New {
        class: Ty,
        ctor: Option<RoutineId>,
        args: Vec<CallArg>,
        after: bool,
        /// `C.Create(…)` (`docs/spec/objects.md` 3.1): the class reference whose class is created
        /// (`NewOf`); `class` is then its static type's class, and `AfterConstruction` is always
        /// called through the table (a descendant may override it).
        of: Option<Box<Expr>>,
        /// How the constructor is called: statically (`Method`), or through the table of the new
        /// instance's class (`Virtual`, a virtual constructor).
        dispatch: Dispatch,
    },
    /// `TPoint.Create(…)` (`docs/spec/objects.md` chapter 6): `call` is the call of the record's
    /// constructor whose first argument is the temporary (`CallArg::Temp`, a default value) the
    /// constructor runs on; the value of the expression is that temporary after the call.
    RecordNew {
        call: Box<Expr>,
    },
    /// A member of `TObject` about classes (`docs/spec/objects.md` chapter 5) whose body is the
    /// VM's: the intrinsic `op` on `args`.
    ClassInfo {
        op: ClassInfoOp,
        args: Vec<Expr>,
    },
    /// `inherited Create` / `inherited Destroy` / … calling a method whose body is the VM's and
    /// does nothing (`docs/spec/objects.md` 14.3): no code. `name` is the method's full name.
    /// `inherited Create(Msg)` / `inherited CreateFmt(…)` of `Exception` (8.2) write `msg` to
    /// `Message` of `base`.
    InheritedBuiltin {
        base: Box<Expr>,
        name: String,
        msg: Option<Box<Expr>>,
    },
    /// A class written as a value where a class reference is expected (`C := TCircle`;
    /// `docs/spec/objects.md` 2.8): the class `class` (the expression's type is a class reference).
    ClassRefValue(super::types::TypeId),
    /// `X is T`, `X as T` and the conversions by type name `T(X)` of an instance or a class
    /// reference (`docs/spec/objects.md` 2.8, 2.9): checked at run time against the class
    /// `class` (for a class reference, the class its type is `class of`).
    ClassCheck {
        operand: Box<Expr>,
        class: super::types::TypeId,
        op: ClassCheckOp,
    },
    /// `O.F` read: a field of a class instance (`docs/spec/objects.md` 2.2).
    ObjField {
        base: Box<Expr>,
        /// The slot number (the parent's fields first).
        slot: u32,
        name: String,
    },
    /// The read of a `[weak]` variable or field (`docs/spec/objects.md` 4.4), whose type is
    /// `Ty::Weak`: a counted reference to the instance, or `nil` once its destruction has
    /// started.
    Weak(Box<Expr>),
    /// `TryStrToInt(S, Value)` (`docs/spec/semantics.md` 5.3): `Value` is an `out` argument of
    /// type `Integer`.
    TryStrToInt {
        s: Box<Expr>,
        target: Place,
    },
    /// `Supports(X, IA[, V])` (`docs/spec/objects.md` 7.6): whether the instance `X` (a class,
    /// `TObject` or interface value) implements the interface `intf`; `V` is written with `X` as
    /// `IA` or `nil`. With `class_ref`, `X` is a class reference (`Supports(C, IA)`) and there is
    /// no `V`.
    Supports {
        operand: Box<Expr>,
        intf: super::types::TypeId,
        target: Option<Place>,
        class_ref: bool,
    },
    /// A routine or a static method as a procedural value (`docs/spec/objects.md` 9.2): the
    /// function index + 1.
    RoutineRef(RoutineId),
    /// A method reference (`docs/spec/objects.md` 9.3, 14.9): the receiver (an instance, or a
    /// class reference for a class method) and the method. `routine` names the method for the
    /// dump; a built-in virtual method (`ToString`) has a routine without a body.
    MethodRef {
        base: Box<Expr>,
        routine: RoutineId,
        how: MethodRefHow,
    },
    /// A procedural value (`Ty::Proc`) as a value of an anonymous method type: a callable value
    /// block of the routine variant (`RoutineRef`; `docs/spec/objects.md` 9.2, 14.9).
    ToClosure(Box<Expr>),
    /// An anonymous method (`docs/spec/objects.md` 9.6, 14.9): `MakeClosure` of the function of
    /// `func` with the cells of `captures`, variables of the routine being analyzed in the order
    /// of `func`'s `Routine::captures`.
    Anon {
        func: RoutineId,
        captures: Vec<VarId>,
    },
    /// A closure-form nested routine as a value of an anonymous method type
    /// (`docs/spec/objects.md` 9.7, 14.9): `ClosureWith` of its group's block, read from `block`
    /// (`None`: the closure of the routine being analyzed, which is of the same group).
    NestedRef {
        func: RoutineId,
        block: Option<VarId>,
    },
    /// A call of a procedural value (`docs/spec/objects.md` 9.4): `CallIndirect` for a
    /// procedural type, `CallClosure` for a method reference or anonymous method type.
    CallValue {
        callee: Box<Expr>,
        args: Vec<CallArg>,
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

/// Which of the key routines an [`ExprKind::Key`] is (`docs/spec/equality.md` chapter 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyOp {
    /// `KeyEquals(A, B)`: `Boolean`.
    Equals,
    /// `KeyHash(X)`: `Int64`.
    Hash,
    /// `KeyCompare(A, B)`: `Integer` (`docs/spec/collections.md` 10.2).
    Compare,
}

/// What a [`ExprKind::ClassCheck`] does (`docs/spec/objects.md` 2.8, 2.9, 14.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassCheckOp {
    /// `X is T`: whether the instance is of the class or a descendant (`IsClass`).
    Is,
    /// `X as T`: the instance, or `EInvalidCast` (`AsClass`).
    As,
    /// `T(X)` of an instance: checked as `as` (`AsClass`).
    Cast,
    /// `TShapeClass(C)` of a class reference: the class reference, or `EInvalidCast`
    /// (`AsClassRef`).
    RefCast,
}

/// The members of `TObject` about classes (`docs/spec/objects.md` chapter 5, 14.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassInfoOp {
    /// `O.ClassType`: the class of the instance (`ClassType`).
    ClassType,
    /// `C.ClassName`: the declared name of the class (`ClassRefName`).
    ClassRefName,
    /// `C.ClassParent` (`ClassParent`).
    ClassParent,
    /// `C.InheritsFrom(D)` (`ClassInherits`).
    InheritsFrom,
    /// `O.RefCount` (`RefCount`).
    RefCount,
}

impl ClassInfoOp {
    /// The name in the `hir` dump (`builtin ClassType TClass`).
    pub fn name(self) -> &'static str {
        match self {
            ClassInfoOp::ClassType => "ClassType",
            ClassInfoOp::ClassRefName => "ClassName",
            ClassInfoOp::ClassParent => "ClassParent",
            ClassInfoOp::InheritsFrom => "InheritsFrom",
            ClassInfoOp::RefCount => "RefCount",
        }
    }
}

/// How a method reference finds its function (`docs/spec/objects.md` 9.3, 14.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodRefHow {
    /// The method itself (`MethodRef`).
    Method,
    /// Entry `k` of the instance's class's table (`VirtualRef`).
    Virtual(u32),
    /// A class method (`ClassMethodRef`).
    Class,
    /// Entry `k` of the class reference's class's table (`ClassVirtualRef`).
    ClassVirtual(u32),
}

/// How a call reaches the routine (`docs/spec/objects.md` 2.4, 14.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dispatch {
    /// A routine (`Call`).
    Routine,
    /// A method called statically (`CallMethod`; checks `Self`).
    Method,
    /// A virtual method: the entry `K` of the instance's class's table (`CallVirtual`). The
    /// routine is the one the static type names (its parameters).
    Virtual(u32),
    /// `inherited`: the parent's method called statically (`CallMethod`).
    Inherited,
    /// A virtual class method: the entry `K` of the table of the class the class reference (the
    /// first argument) points to (`CallVirtualClass`).
    VirtualClass(u32),
    /// An interface's method: method `K` of the interface, in the interface table of the
    /// instance's class (`CallIntf`; `docs/spec/objects.md` 7.5, 14.7).
    Intf(super::types::TypeId, u32),
    /// A closure-form nested routine (`Call` with its group's block as the hidden first argument;
    /// `docs/spec/objects.md` 9.7, 14.9): the variable holding the block (`None`: the closure of
    /// the calling routine, which is of the same group).
    Group(Option<VarId>),
}

#[derive(Debug, Clone)]
pub enum CallArg {
    Value(Expr),
    /// Actual argument for a `var` / `out` parameter.
    Ref {
        place: Place,
        is_out: bool,
    },
    /// A value copied to a temporary whose reference is passed to a `var` parameter: `Self` of a
    /// record method called on an expression that cannot be written (`docs/spec/objects.md`
    /// chapter 6), and the new value of a record constructor. Writes go to the copy.
    Temp(Expr),
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
    /// `IsRefCountedObject(X)` (`docs/spec/collections.md` 12.1, 9.2; the units of the standard
    /// library only): whether `X` holds a live instance of a reference counted class, expanded by
    /// the type of `X` (`TObject` and the reference counted classes look at the instance; other
    /// types are `False`).
    IsRefCountedObject,
    /// `SameInstance(X, Y)` (`docs/spec/collections.md` 9.3, 12.1; the units of the standard
    /// library only): for a class type, whether `X` and `Y` are the same instance; `False` for the
    /// other types.
    IsSameInstance,
    /// `ObjectClassName(X)` (`docs/spec/collections.md` 9.3, 12.1; the units of the standard
    /// library only): `X.ClassName` for a class type, `''` for the other types.
    ObjectClassName,
    /// `IsOverridden(M)` (`docs/spec/collections.md` 12.1; the units of the standard library
    /// only): whether the class of `Self` puts another function than the method `M` in the entry
    /// of `M` in its virtual method table. The arguments are `Self`, the entry (a constant) and
    /// the method as a [`ExprKind::RoutineRef`] (`IsOverridden`; `docs/spec/bytecode.md` 2.22).
    IsOverridden,
    /// `ExceptObject`: the exception of the `except` block being run (`docs/spec/objects.md` 8.4).
    ExceptObject,
    /// `Succ(X)` / `Pred(X)` (`docs/spec/semantics.md` 5.3). Only a name: sema lowers the call to
    /// `X + 1` / `X - 1` converted back to the type of `X`.
    Succ,
    Pred,
    /// `Sqrt(X)` of a `Double` (`docs/spec/semantics.md` 5.3).
    Sqrt,
    /// `HashCombine(H, X)`: `HashMix` (`docs/spec/equality.md` chapter 4). Never folded (hash values
    /// differ between processes).
    HashCombine,
    /// `inherited Equals(Obj)` reaching the built-in body of `TObject`: whether `Self` and `Obj`
    /// are the same instance, `EqInt` (`docs/spec/equality.md` 8.2). Not a name of `System`.
    SameInstance,
    /// `inherited GetHashCode` reaching the built-in body of `TObject`: `HashInt` of the handle
    /// (`docs/spec/equality.md` 8.2). Not a name of `System`.
    InstanceHash,
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
    /// Only while resolving: a call becomes [`ExprKind::Supports`].
    Supports,
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
    /// `X.ToString` of the built-in helpers (`docs/spec/objects.md` 10.7): the text of an integer,
    /// real (a `Currency` is converted to `Double` first, as `FloatToStr`), `Boolean` (`'True'` /
    /// `'False'`), `Char` or enumeration value.
    ToString,
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
