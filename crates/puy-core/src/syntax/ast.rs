//! Typed accessors over the CST (`docs/design/compiler-pipeline.md` 3.3). No separate tree is built.

use super::{SyntaxKind, SyntaxNode, SyntaxToken};
use SyntaxKind::*;

pub trait AstNode: Sized {
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;
}

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            fn cast(node: SyntaxNode) -> Option<Self> {
                (node.kind() == $kind).then_some($name(node))
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

fn child<N: AstNode>(node: &SyntaxNode) -> Option<N> {
    node.children().find_map(N::cast)
}

fn children<N: AstNode>(node: &SyntaxNode) -> impl Iterator<Item = N> {
    node.children().filter_map(N::cast)
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == kind)
}

fn tokens(node: &SyntaxNode, kind: SyntaxKind) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(move |t| t.kind() == kind)
}

// ---- File ----

ast_node!(SourceFile, SOURCE_FILE);
ast_node!(Program, PROGRAM);
ast_node!(ProgramHeader, PROGRAM_HEADER);
ast_node!(Script, SCRIPT);
ast_node!(UsesClause, USES_CLAUSE);
ast_node!(UsesItem, USES_ITEM);
ast_node!(QualifiedName, QUALIFIED_NAME);
ast_node!(ConstSection, CONST_SECTION);
ast_node!(ConstDecl, CONST_DECL);
ast_node!(ResStringSection, RESOURCESTRING_SECTION);
ast_node!(VarSection, VAR_SECTION);
ast_node!(VarDecl, VAR_DECL);
ast_node!(IdentList, IDENT_LIST);
ast_node!(TypeRef, TYPE_REF);
/// A routine declaration (`ROUTINE_DECL`), or a heading in the `interface` section of a unit
/// (`ROUTINE_HEADER_DECL`, which has no body; `docs/spec/units.md` chapter 1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RoutineDecl(SyntaxNode);

impl AstNode for RoutineDecl {
    fn cast(node: SyntaxNode) -> Option<Self> {
        matches!(node.kind(), ROUTINE_DECL | ROUTINE_HEADER_DECL).then_some(RoutineDecl(node))
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}
ast_node!(Unit, UNIT);
ast_node!(UnitHeader, UNIT_HEADER);
ast_node!(InterfaceSection, INTERFACE_SECTION);
ast_node!(ImplementationSection, IMPLEMENTATION_SECTION);
ast_node!(InitSection, INIT_SECTION);
ast_node!(FinalSection, FINAL_SECTION);
ast_node!(RoutineHeader, ROUTINE_HEADER);
ast_node!(ParamList, PARAM_LIST);
ast_node!(ParamGroup, PARAM_GROUP);
ast_node!(ReturnType, RETURN_TYPE);
ast_node!(RoutineDirective, ROUTINE_DIRECTIVE);
ast_node!(RoutineBody, ROUTINE_BODY);
ast_node!(CompoundStmt, COMPOUND_STMT);
ast_node!(AssignStmt, ASSIGN_STMT);
ast_node!(ExprStmt, EXPR_STMT);
ast_node!(InlineVarDecl, INLINE_VAR_DECL);
ast_node!(InlineConstDecl, INLINE_CONST_DECL);
ast_node!(IfStmt, IF_STMT);
ast_node!(ElseBranch, ELSE_BRANCH);
ast_node!(WhileStmt, WHILE_STMT);
ast_node!(RepeatStmt, REPEAT_STMT);
ast_node!(ForStmt, FOR_STMT);
ast_node!(ForVar, FOR_VAR);
ast_node!(Literal, LITERAL);
ast_node!(NameRef, NAME_REF);
ast_node!(ParenExpr, PAREN_EXPR);
ast_node!(UnaryExpr, UNARY_EXPR);
ast_node!(BinaryExpr, BINARY_EXPR);
ast_node!(CallExpr, CALL_EXPR);
ast_node!(ArgList, ARG_LIST);
ast_node!(Arg, ARG);
ast_node!(IndexExpr, INDEX_EXPR);
ast_node!(FieldExpr, FIELD_EXPR);
ast_node!(DerefExpr, DEREF_EXPR);
ast_node!(NilExpr, NIL_EXPR);
ast_node!(InheritedExpr, INHERITED_EXPR);
ast_node!(ErrorNode, ERROR_NODE);
ast_node!(TypeSection, TYPE_SECTION);
ast_node!(TypeDecl, TYPE_DECL);
ast_node!(RecordType, RECORD_TYPE);
ast_node!(FieldDecl, FIELD_DECL);
ast_node!(ArrayType, ARRAY_TYPE);
ast_node!(IndexRange, INDEX_RANGE);
ast_node!(Slice, SLICE);
ast_node!(RecordConst, RECORD_CONST);
ast_node!(FieldConst, FIELD_CONST);
ast_node!(ArrayConst, ARRAY_CONST);
ast_node!(ArrayCtor, ARRAY_CTOR);
ast_node!(GenericTypeRef, GENERIC_TYPE_REF);
ast_node!(RaiseStmt, RAISE_STMT);
ast_node!(ClassType, CLASS_TYPE);
ast_node!(TryStmt, TRY_STMT);
ast_node!(ExceptBlock, EXCEPT_BLOCK);
ast_node!(FinallyBlock, FINALLY_BLOCK);
ast_node!(ExceptionHandler, EXCEPTION_HANDLER);
ast_node!(HandlerElse, HANDLER_ELSE);
ast_node!(EnumType, ENUM_TYPE);
ast_node!(EnumValue, ENUM_VALUE);
ast_node!(SubrangeType, SUBRANGE_TYPE);
ast_node!(SetType, SET_TYPE);
ast_node!(CaseStmt, CASE_STMT);
ast_node!(CaseArm, CASE_ARM);
ast_node!(CaseLabel, CASE_LABEL);
ast_node!(CaseElse, CASE_ELSE);

impl SourceFile {
    pub fn program(&self) -> Option<Program> {
        child(&self.0)
    }
    pub fn script(&self) -> Option<Script> {
        child(&self.0)
    }
    pub fn unit(&self) -> Option<Unit> {
        child(&self.0)
    }
}

impl Unit {
    pub fn header(&self) -> Option<UnitHeader> {
        child(&self.0)
    }
    pub fn interface(&self) -> Option<InterfaceSection> {
        child(&self.0)
    }
    pub fn implementation(&self) -> Option<ImplementationSection> {
        child(&self.0)
    }
    /// `initialization` (or the old form `begin`).
    pub fn initialization(&self) -> Option<InitSection> {
        child(&self.0)
    }
    pub fn finalization(&self) -> Option<FinalSection> {
        child(&self.0)
    }
    /// The final `end`.
    pub fn end_token(&self) -> Option<SyntaxToken> {
        token(&self.0, END_KW)
    }
    /// The `unit` keyword.
    pub fn unit_token(&self) -> Option<SyntaxToken> {
        self.header().and_then(|h| token(&h.0, UNIT_KW))
    }
}

impl UnitHeader {
    pub fn name(&self) -> Option<QualifiedName> {
        child(&self.0)
    }
    pub fn directives(&self) -> impl Iterator<Item = RoutineDirective> {
        children(&self.0)
    }
}

impl InterfaceSection {
    /// The declarations, including `uses` (headings are `Item::Routine`).
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.0.children().filter_map(Item::cast)
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        token(&self.0, INTERFACE_KW)
    }
}

impl InitSection {
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
    /// `initialization` or `begin`.
    pub fn keyword(&self) -> Option<SyntaxToken> {
        token(&self.0, INITIALIZATION_KW).or_else(|| token(&self.0, BEGIN_KW))
    }
}

impl FinalSection {
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        token(&self.0, FINALIZATION_KW)
    }
}

impl ImplementationSection {
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.0.children().filter_map(Item::cast)
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        token(&self.0, IMPLEMENTATION_KW)
    }
}

impl Program {
    pub fn header(&self) -> Option<ProgramHeader> {
        child(&self.0)
    }
    pub fn uses_clauses(&self) -> impl Iterator<Item = UsesClause> {
        children(&self.0)
    }
    /// Items of the declaration part (sections and routines, including `uses`), in order.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.0.children().filter_map(Item::cast)
    }
    pub fn main_block(&self) -> Option<CompoundStmt> {
        child(&self.0)
    }
}

impl ProgramHeader {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
}

impl Script {
    pub fn items(&self) -> impl Iterator<Item = ScriptItem> {
        self.0.children().filter_map(ScriptItem::cast)
    }
}

/// An item of the declaration part.
#[derive(Debug, Clone)]
pub enum Item {
    Uses(UsesClause),
    Const(ConstSection),
    ResString(ResStringSection),
    Var(VarSection),
    Type(TypeSection),
    Routine(RoutineDecl),
    Error(ErrorNode),
}

impl Item {
    pub fn cast(node: SyntaxNode) -> Option<Item> {
        Some(match node.kind() {
            USES_CLAUSE => Item::Uses(UsesClause(node)),
            CONST_SECTION => Item::Const(ConstSection(node)),
            RESOURCESTRING_SECTION => Item::ResString(ResStringSection(node)),
            VAR_SECTION => Item::Var(VarSection(node)),
            TYPE_SECTION => Item::Type(TypeSection(node)),
            ROUTINE_DECL | ROUTINE_HEADER_DECL => Item::Routine(RoutineDecl(node)),
            ERROR_NODE => Item::Error(ErrorNode(node)),
            _ => return None,
        })
    }
}

/// A top-level item of a script.
#[derive(Debug, Clone)]
pub enum ScriptItem {
    Uses(UsesClause),
    Const(ConstSection),
    ResString(ResStringSection),
    Var(VarSection),
    Type(TypeSection),
    Routine(RoutineDecl),
    Stmt(Stmt),
    Error(ErrorNode),
}

impl ScriptItem {
    pub fn cast(node: SyntaxNode) -> Option<ScriptItem> {
        Some(match node.kind() {
            USES_CLAUSE => ScriptItem::Uses(UsesClause(node)),
            CONST_SECTION => ScriptItem::Const(ConstSection(node)),
            RESOURCESTRING_SECTION => ScriptItem::ResString(ResStringSection(node)),
            VAR_SECTION => ScriptItem::Var(VarSection(node)),
            TYPE_SECTION => ScriptItem::Type(TypeSection(node)),
            ROUTINE_DECL => ScriptItem::Routine(RoutineDecl(node)),
            ERROR_NODE => ScriptItem::Error(ErrorNode(node)),
            _ => ScriptItem::Stmt(Stmt::cast(node)?),
        })
    }
}

impl UsesClause {
    pub fn items(&self) -> impl Iterator<Item = UsesItem> {
        children(&self.0)
    }
    pub fn uses_token(&self) -> Option<SyntaxToken> {
        token(&self.0, USES_KW)
    }
    pub fn semicolon(&self) -> Option<SyntaxToken> {
        token(&self.0, SEMICOLON)
    }
}

impl UsesItem {
    pub fn name(&self) -> Option<QualifiedName> {
        child(&self.0)
    }
    /// The `as` alias.
    pub fn alias(&self) -> Option<SyntaxToken> {
        token(&self.0, AS_KW).and_then(|_| tokens(&self.0, IDENT).next())
    }
    pub fn in_token(&self) -> Option<SyntaxToken> {
        token(&self.0, IN_KW)
    }
}

impl QualifiedName {
    pub fn idents(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, IDENT)
    }
    /// The spelling joined with `.`.
    pub fn text(&self) -> String {
        self.idents()
            .map(|t| t.text().to_string())
            .collect::<Vec<_>>()
            .join(".")
    }
    pub fn last_ident(&self) -> Option<SyntaxToken> {
        self.idents().last()
    }
}

impl ConstSection {
    pub fn decls(&self) -> impl Iterator<Item = ConstDecl> {
        children(&self.0)
    }
}

impl ResStringSection {
    pub fn decls(&self) -> impl Iterator<Item = ConstDecl> {
        children(&self.0)
    }
}

impl ConstDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn type_ref(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl VarSection {
    pub fn decls(&self) -> impl Iterator<Item = VarDecl> {
        children(&self.0)
    }
    pub fn var_token(&self) -> Option<SyntaxToken> {
        token(&self.0, VAR_KW)
    }
}

impl VarDecl {
    pub fn names(&self) -> Vec<SyntaxToken> {
        child::<IdentList>(&self.0)
            .map(|l| l.idents().collect())
            .unwrap_or_default()
    }
    pub fn type_ref(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.0, EQ)
    }
    pub fn initializer(&self) -> Option<Expr> {
        self.eq_token()?;
        self.0.children().find_map(Expr::cast)
    }
    pub fn absolute_token(&self) -> Option<SyntaxToken> {
        token(&self.0, ABSOLUTE_KW)
    }
    pub fn semicolon(&self) -> Option<SyntaxToken> {
        token(&self.0, SEMICOLON)
    }
}

impl IdentList {
    pub fn idents(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.0, IDENT)
    }
}

/// A type written in a declaration: a name, or an anonymous `record` / `array` type (`docs/spec/types.md` chapter 1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeSpec {
    Ref(TypeRef),
    /// `TArray<T>` (`docs/spec/types.md` chapter 1).
    Generic(GenericTypeRef),
    Record(RecordType),
    Array(ArrayType),
    /// `class(Parent)`: an exception class (`docs/spec/exceptions.md` chapter 3).
    Class(ClassType),
    /// `(Red, Green, Blue)` (`docs/spec/types.md` 8.1).
    Enum(EnumType),
    /// `0..9` (`docs/spec/types.md` 8.2).
    Subrange(SubrangeType),
    /// `set of T` (`docs/spec/types.md` 8.3).
    Set(SetType),
}

impl TypeSpec {
    pub fn cast(node: SyntaxNode) -> Option<TypeSpec> {
        Some(match node.kind() {
            TYPE_REF => TypeSpec::Ref(TypeRef(node)),
            GENERIC_TYPE_REF => TypeSpec::Generic(GenericTypeRef(node)),
            RECORD_TYPE => TypeSpec::Record(RecordType(node)),
            ARRAY_TYPE => TypeSpec::Array(ArrayType(node)),
            CLASS_TYPE => TypeSpec::Class(ClassType(node)),
            ENUM_TYPE => TypeSpec::Enum(EnumType(node)),
            SUBRANGE_TYPE => TypeSpec::Subrange(SubrangeType(node)),
            SET_TYPE => TypeSpec::Set(SetType(node)),
            _ => return None,
        })
    }
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            TypeSpec::Ref(n) => &n.0,
            TypeSpec::Generic(n) => &n.0,
            TypeSpec::Record(n) => &n.0,
            TypeSpec::Array(n) => &n.0,
            TypeSpec::Class(n) => &n.0,
            TypeSpec::Enum(n) => &n.0,
            TypeSpec::Subrange(n) => &n.0,
            TypeSpec::Set(n) => &n.0,
        }
    }
    /// The type name, if the type is written as a name.
    pub fn as_ref(&self) -> Option<&TypeRef> {
        match self {
            TypeSpec::Ref(r) => Some(r),
            _ => None,
        }
    }
    /// The spelling as written (for fix messages).
    pub fn text(&self) -> String {
        match self {
            TypeSpec::Ref(r) => r.text(),
            _ => self.syntax().text().to_string(),
        }
    }
}

impl TypeSection {
    pub fn decls(&self) -> impl Iterator<Item = TypeDecl> {
        children(&self.0)
    }
}

impl TypeDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn type_spec(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
}

impl EnumType {
    pub fn values(&self) -> impl Iterator<Item = EnumValue> {
        children(&self.0)
    }
}

impl EnumValue {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    /// The ordinal written after `=`.
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl SubrangeType {
    /// The two bounds.
    pub fn bounds(&self) -> (Option<Expr>, Option<Expr>) {
        let mut it = self.0.children().filter_map(Expr::cast);
        (it.next(), it.next())
    }
}

impl RecordType {
    pub fn packed_token(&self) -> Option<SyntaxToken> {
        token(&self.0, PACKED_KW)
    }
    pub fn record_token(&self) -> Option<SyntaxToken> {
        token(&self.0, RECORD_KW)
    }
    pub fn fields(&self) -> impl Iterator<Item = FieldDecl> {
        children(&self.0)
    }
}

impl FieldDecl {
    pub fn names(&self) -> Vec<SyntaxToken> {
        child::<IdentList>(&self.0)
            .map(|l| l.idents().collect())
            .unwrap_or_default()
    }
    pub fn type_spec(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
}

impl ArrayType {
    pub fn array_token(&self) -> Option<SyntaxToken> {
        token(&self.0, ARRAY_KW)
    }
    /// The index ranges in brackets (empty for `array of T`).
    pub fn ranges(&self) -> Vec<IndexRange> {
        children(&self.0).collect()
    }
    /// Whether the type has brackets (a static array).
    pub fn has_brackets(&self) -> bool {
        token(&self.0, L_BRACK).is_some()
    }
    pub fn element(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
}

impl SetType {
    pub fn set_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SET_KW)
    }
    /// The base type after `of`.
    pub fn base(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
}

impl IndexRange {
    /// `L..H`: the two bounds. A single expression (a type name) has only `low`.
    pub fn bounds(&self) -> (Option<Expr>, Option<Expr>) {
        let mut it = self.0.children().filter_map(Expr::cast);
        (it.next(), it.next())
    }
    pub fn is_range(&self) -> bool {
        token(&self.0, DOTDOT).is_some()
    }
}

impl RecordConst {
    pub fn fields(&self) -> impl Iterator<Item = FieldConst> {
        children(&self.0)
    }
}

impl FieldConst {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl ArrayConst {
    pub fn elements(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

/// One element of an array constructor: an expression, or a range `L..H` (a set constructor, M2b).
#[derive(Debug, Clone)]
pub enum CtorElem {
    Expr(Expr),
    Range(IndexRange),
}

impl ArrayCtor {
    pub fn l_brack(&self) -> Option<SyntaxToken> {
        token(&self.0, L_BRACK)
    }
    pub fn elements(&self) -> Vec<CtorElem> {
        self.0
            .children()
            .filter_map(|n| match n.kind() {
                INDEX_RANGE => Some(CtorElem::Range(IndexRange(n))),
                _ => Expr::cast(n).map(CtorElem::Expr),
            })
            .collect()
    }
}

impl ClassType {
    /// The parent class.
    pub fn parent(&self) -> Option<TypeRef> {
        child(&self.0)
    }
    pub fn class_token(&self) -> Option<SyntaxToken> {
        token(&self.0, CLASS_KW)
    }
}

impl RaiseStmt {
    /// The raised expression (`None` for `raise;`).
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    pub fn raise_token(&self) -> Option<SyntaxToken> {
        token(&self.0, RAISE_KW)
    }
}

impl TryStmt {
    pub fn try_token(&self) -> Option<SyntaxToken> {
        token(&self.0, TRY_KW)
    }
    /// The statements of the body (between `try` and `except` / `finally`).
    pub fn body(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
    pub fn except_block(&self) -> Option<ExceptBlock> {
        child(&self.0)
    }
    pub fn finally_block(&self) -> Option<FinallyBlock> {
        child(&self.0)
    }
}

impl ExceptBlock {
    pub fn except_token(&self) -> Option<SyntaxToken> {
        token(&self.0, EXCEPT_KW)
    }
    /// The statements of a bare `except` (no handlers).
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
    pub fn handlers(&self) -> impl Iterator<Item = ExceptionHandler> {
        children(&self.0)
    }
    pub fn else_branch(&self) -> Option<HandlerElse> {
        child(&self.0)
    }
}

impl FinallyBlock {
    pub fn finally_token(&self) -> Option<SyntaxToken> {
        token(&self.0, FINALLY_KW)
    }
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
}

impl ExceptionHandler {
    pub fn on_token(&self) -> Option<SyntaxToken> {
        token(&self.0, ON_KW)
    }
    /// The exception variable (`E` in `on E: T do`).
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn type_ref(&self) -> Option<TypeRef> {
        child(&self.0)
    }
    pub fn body(&self) -> Option<Stmt> {
        self.0.children().find_map(Stmt::cast)
    }
}

impl HandlerElse {
    pub fn else_token(&self) -> Option<SyntaxToken> {
        token(&self.0, ELSE_KW)
    }
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
}

impl GenericTypeRef {
    /// The generic type's name (`TArray`).
    pub fn name(&self) -> Option<QualifiedName> {
        child(&self.0)
    }
    /// The type argument.
    pub fn arg(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
    pub fn lt_token(&self) -> Option<SyntaxToken> {
        token(&self.0, LT)
    }
}

impl TypeRef {
    pub fn name(&self) -> Option<QualifiedName> {
        child(&self.0)
    }
    pub fn string_token(&self) -> Option<SyntaxToken> {
        token(&self.0, STRING_KW)
    }
    /// The spelling of the type name (including `string`).
    pub fn text(&self) -> String {
        match self.name() {
            Some(q) => q.text(),
            None => self
                .string_token()
                .map(|t| t.text().to_string())
                .unwrap_or_default(),
        }
    }
}

impl RoutineDecl {
    pub fn header(&self) -> Option<RoutineHeader> {
        child(&self.0)
    }
    pub fn directives(&self) -> impl Iterator<Item = RoutineDirective> {
        children(&self.0)
    }
    pub fn body(&self) -> Option<RoutineBody> {
        child(&self.0)
    }
    pub fn has_forward(&self) -> bool {
        self.directives().any(|d| d.kind() == Some(FORWARD_KW))
    }
    /// Whether it contains a syntax error (has an `ERROR` node).
    pub fn has_error_node(&self) -> bool {
        self.0.descendants().any(|n| n.kind() == ERROR_NODE)
    }
}

impl RoutineHeader {
    pub fn is_function(&self) -> bool {
        token(&self.0, FUNCTION_KW).is_some()
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        token(&self.0, PROCEDURE_KW).or_else(|| token(&self.0, FUNCTION_KW))
    }
    pub fn name(&self) -> Option<QualifiedName> {
        child(&self.0)
    }
    pub fn param_list(&self) -> Option<ParamList> {
        child(&self.0)
    }
    pub fn params(&self) -> Vec<ParamGroup> {
        self.param_list()
            .map(|l| l.groups().collect())
            .unwrap_or_default()
    }
    pub fn return_type(&self) -> Option<TypeSpec> {
        child::<ReturnType>(&self.0).and_then(|r| r.0.children().find_map(TypeSpec::cast))
    }
}

impl ParamList {
    pub fn groups(&self) -> impl Iterator<Item = ParamGroup> {
        children(&self.0)
    }
}

/// How a parameter is passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamMode {
    Value,
    Const,
    Var,
    Out,
}

impl ParamGroup {
    pub fn mode(&self) -> ParamMode {
        if token(&self.0, CONST_KW).is_some() {
            ParamMode::Const
        } else if token(&self.0, VAR_KW).is_some() {
            ParamMode::Var
        } else if token(&self.0, OUT_KW).is_some() {
            ParamMode::Out
        } else {
            ParamMode::Value
        }
    }
    pub fn names(&self) -> Vec<SyntaxToken> {
        child::<IdentList>(&self.0)
            .map(|l| l.idents().collect())
            .unwrap_or_default()
    }
    pub fn type_ref(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
    /// The `DEFAULT_VALUE` node (`"=" Expr`; `docs/spec/semantics.md` 4.8).
    pub fn default_node(&self) -> Option<SyntaxNode> {
        self.0.children().find(|n| n.kind() == DEFAULT_VALUE)
    }
    /// The expression of the default value.
    pub fn default_value(&self) -> Option<Expr> {
        self.default_node()?.children().find_map(Expr::cast)
    }
}

impl RoutineDirective {
    pub fn kind(&self) -> Option<SyntaxKind> {
        self.0.first_token().map(|t| t.kind())
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        self.0.first_token()
    }
}

impl RoutineBody {
    pub fn const_sections(&self) -> impl Iterator<Item = ConstSection> {
        children(&self.0)
    }
    pub fn var_sections(&self) -> impl Iterator<Item = VarSection> {
        children(&self.0)
    }
    /// Sections in order (`const` and `var` intermixed).
    pub fn sections(&self) -> impl Iterator<Item = Item> {
        self.0.children().filter_map(Item::cast)
    }
    pub fn block(&self) -> Option<CompoundStmt> {
        child(&self.0)
    }
}

// ---- Statements ----

#[derive(Debug, Clone)]
pub enum Stmt {
    Compound(CompoundStmt),
    Assign(AssignStmt),
    Expr(ExprStmt),
    InlineVar(InlineVarDecl),
    InlineConst(InlineConstDecl),
    If(IfStmt),
    While(WhileStmt),
    Repeat(RepeatStmt),
    For(ForStmt),
    Raise(RaiseStmt),
    Try(TryStmt),
    Case(CaseStmt),
    Error(ErrorNode),
}

impl Stmt {
    pub fn cast(node: SyntaxNode) -> Option<Stmt> {
        Some(match node.kind() {
            COMPOUND_STMT => Stmt::Compound(CompoundStmt(node)),
            ASSIGN_STMT => Stmt::Assign(AssignStmt(node)),
            EXPR_STMT => Stmt::Expr(ExprStmt(node)),
            INLINE_VAR_DECL => Stmt::InlineVar(InlineVarDecl(node)),
            INLINE_CONST_DECL => Stmt::InlineConst(InlineConstDecl(node)),
            IF_STMT => Stmt::If(IfStmt(node)),
            WHILE_STMT => Stmt::While(WhileStmt(node)),
            REPEAT_STMT => Stmt::Repeat(RepeatStmt(node)),
            FOR_STMT => Stmt::For(ForStmt(node)),
            RAISE_STMT => Stmt::Raise(RaiseStmt(node)),
            TRY_STMT => Stmt::Try(TryStmt(node)),
            CASE_STMT => Stmt::Case(CaseStmt(node)),
            ERROR_NODE => Stmt::Error(ErrorNode(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Stmt::Compound(n) => &n.0,
            Stmt::Assign(n) => &n.0,
            Stmt::Expr(n) => &n.0,
            Stmt::InlineVar(n) => &n.0,
            Stmt::InlineConst(n) => &n.0,
            Stmt::If(n) => &n.0,
            Stmt::While(n) => &n.0,
            Stmt::Repeat(n) => &n.0,
            Stmt::For(n) => &n.0,
            Stmt::Raise(n) => &n.0,
            Stmt::Try(n) => &n.0,
            Stmt::Case(n) => &n.0,
            Stmt::Error(n) => &n.0,
        }
    }
}

impl CompoundStmt {
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
    pub fn begin_token(&self) -> Option<SyntaxToken> {
        token(&self.0, BEGIN_KW)
    }
    pub fn end_token(&self) -> Option<SyntaxToken> {
        token(&self.0, END_KW)
    }
}

impl AssignStmt {
    pub fn target(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
    pub fn value(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).nth(1)
    }
    pub fn assign_token(&self) -> Option<SyntaxToken> {
        token(&self.0, ASSIGN)
    }
}

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl InlineVarDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn type_ref(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
    pub fn initializer(&self) -> Option<Expr> {
        token(&self.0, ASSIGN)?;
        self.0.children().find_map(Expr::cast)
    }
}

impl InlineConstDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn type_ref(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl IfStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    pub fn then_branch(&self) -> Option<Stmt> {
        self.0.children().find_map(Stmt::cast)
    }
    pub fn else_branch(&self) -> Option<ElseBranch> {
        child(&self.0)
    }
    pub fn then_token(&self) -> Option<SyntaxToken> {
        token(&self.0, THEN_KW)
    }
}

impl CaseStmt {
    pub fn case_token(&self) -> Option<SyntaxToken> {
        token(&self.0, CASE_KW)
    }
    /// The selector (the expression between `case` and `of`).
    pub fn selector(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    pub fn arms(&self) -> impl Iterator<Item = CaseArm> {
        children(&self.0)
    }
    pub fn else_branch(&self) -> Option<CaseElse> {
        child(&self.0)
    }
}

impl CaseArm {
    pub fn labels(&self) -> impl Iterator<Item = CaseLabel> {
        children(&self.0)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, COLON)
    }
    /// The statement after `:` (the last statement child: an `ERROR` node before `:` is not it).
    pub fn body(&self) -> Option<Stmt> {
        let colon = self.colon_token()?.text_range().end();
        self.0
            .children()
            .filter(|n| n.text_range().start() >= colon)
            .find_map(Stmt::cast)
    }
}

impl CaseLabel {
    /// The value, or the lower bound of a range.
    pub fn low(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    /// The upper bound of a range (`None` for a single value).
    pub fn high(&self) -> Option<Expr> {
        let dots = token(&self.0, DOTDOT)?.text_range().end();
        self.0
            .children()
            .filter(|n| n.text_range().start() >= dots)
            .find_map(Expr::cast)
    }
    pub fn is_range(&self) -> bool {
        token(&self.0, DOTDOT).is_some()
    }
}

impl CaseElse {
    pub fn else_token(&self) -> Option<SyntaxToken> {
        token(&self.0, ELSE_KW)
    }
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
}

impl ElseBranch {
    pub fn stmt(&self) -> Option<Stmt> {
        self.0.children().find_map(Stmt::cast)
    }
}

impl WhileStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    pub fn body(&self) -> Option<Stmt> {
        self.0.children().find_map(Stmt::cast)
    }
}

impl RepeatStmt {
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> {
        let until = token(&self.0, UNTIL_KW).map(|t| t.text_range().start());
        self.0
            .children()
            .filter(move |n| until.is_none_or(|u| n.text_range().start() < u))
            .filter_map(Stmt::cast)
    }
    pub fn condition(&self) -> Option<Expr> {
        let until = token(&self.0, UNTIL_KW)?.text_range().start();
        self.0
            .children()
            .filter(|n| n.text_range().start() >= until)
            .find_map(Expr::cast)
    }
    pub fn repeat_token(&self) -> Option<SyntaxToken> {
        token(&self.0, REPEAT_KW)
    }
}

impl ForStmt {
    pub fn var(&self) -> Option<ForVar> {
        child(&self.0)
    }
    pub fn from(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
    pub fn to(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).nth(1)
    }
    pub fn is_downto(&self) -> bool {
        token(&self.0, DOWNTO_KW).is_some()
    }
    /// `for X in C` (`docs/spec/types.md` chapter 5); the collection is `from()`.
    pub fn is_in(&self) -> bool {
        token(&self.0, IN_KW).is_some()
    }
    pub fn body(&self) -> Option<Stmt> {
        self.0.children().find_map(Stmt::cast)
    }
    pub fn for_token(&self) -> Option<SyntaxToken> {
        token(&self.0, FOR_KW)
    }
}

impl ForVar {
    pub fn is_inline(&self) -> bool {
        token(&self.0, VAR_KW).is_some()
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn type_ref(&self) -> Option<TypeSpec> {
        self.0.children().find_map(TypeSpec::cast)
    }
}

// ---- Expressions ----

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal),
    NameRef(NameRef),
    Paren(ParenExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Call(CallExpr),
    Index(IndexExpr),
    Field(FieldExpr),
    Deref(DerefExpr),
    Nil(NilExpr),
    Inherited(InheritedExpr),
    RecordConst(RecordConst),
    ArrayConst(ArrayConst),
    ArrayCtor(ArrayCtor),
    Error(ErrorNode),
}

impl Expr {
    pub fn cast(node: SyntaxNode) -> Option<Expr> {
        Some(match node.kind() {
            LITERAL => Expr::Literal(Literal(node)),
            NAME_REF => Expr::NameRef(NameRef(node)),
            PAREN_EXPR => Expr::Paren(ParenExpr(node)),
            UNARY_EXPR => Expr::Unary(UnaryExpr(node)),
            BINARY_EXPR => Expr::Binary(BinaryExpr(node)),
            CALL_EXPR => Expr::Call(CallExpr(node)),
            INDEX_EXPR => Expr::Index(IndexExpr(node)),
            FIELD_EXPR => Expr::Field(FieldExpr(node)),
            DEREF_EXPR => Expr::Deref(DerefExpr(node)),
            NIL_EXPR => Expr::Nil(NilExpr(node)),
            INHERITED_EXPR => Expr::Inherited(InheritedExpr(node)),
            RECORD_CONST => Expr::RecordConst(RecordConst(node)),
            ARRAY_CONST => Expr::ArrayConst(ArrayConst(node)),
            ARRAY_CTOR => Expr::ArrayCtor(ArrayCtor(node)),
            ERROR_NODE => Expr::Error(ErrorNode(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Literal(n) => &n.0,
            Expr::NameRef(n) => &n.0,
            Expr::Paren(n) => &n.0,
            Expr::Unary(n) => &n.0,
            Expr::Binary(n) => &n.0,
            Expr::Call(n) => &n.0,
            Expr::Index(n) => &n.0,
            Expr::Field(n) => &n.0,
            Expr::Deref(n) => &n.0,
            Expr::Nil(n) => &n.0,
            Expr::Inherited(n) => &n.0,
            Expr::RecordConst(n) => &n.0,
            Expr::ArrayConst(n) => &n.0,
            Expr::ArrayCtor(n) => &n.0,
            Expr::Error(n) => &n.0,
        }
    }
}

impl Literal {
    pub fn token(&self) -> Option<SyntaxToken> {
        self.0.first_token()
    }
}

impl NameRef {
    /// `IDENT` or `string`.
    pub fn token(&self) -> Option<SyntaxToken> {
        self.0.first_token()
    }
    pub fn text(&self) -> String {
        self.token()
            .map(|t| t.text().to_string())
            .unwrap_or_default()
    }
    pub fn is_string_keyword(&self) -> bool {
        self.token().is_some_and(|t| t.kind() == STRING_KW)
    }
}

impl ParenExpr {
    pub fn inner(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl UnaryExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        self.0.first_token()
    }
    pub fn operand(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl BinaryExpr {
    pub fn lhs(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
    pub fn rhs(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).nth(1)
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| !t.kind().is_trivia() && t.kind() != ERROR)
    }
}

impl CallExpr {
    pub fn callee(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        child(&self.0)
    }
    pub fn args(&self) -> Vec<Arg> {
        self.arg_list()
            .map(|l| l.args().collect())
            .unwrap_or_default()
    }
    /// Whether the argument list has an `ERROR` node (`ArgList::has_error`).
    pub fn has_error_arg(&self) -> bool {
        self.arg_list().is_some_and(|l| l.has_error())
    }
}

impl ArgList {
    pub fn args(&self) -> impl Iterator<Item = Arg> {
        children(&self.0)
    }
    /// Whether there is an `ERROR` node skipped by the parser directly under the argument list
    /// (`Chr(0x3042)`; the argument count cannot be determined, so E0705 is not reported).
    pub fn has_error(&self) -> bool {
        self.0.children().any(|n| {
            n.kind() == SyntaxKind::ERROR_NODE
                || (n.kind() == SyntaxKind::ARG
                    && n.children().any(|c| c.kind() == SyntaxKind::ERROR_NODE))
        })
    }
}

impl Arg {
    pub fn exprs(&self) -> Vec<Expr> {
        self.0.children().filter_map(Expr::cast).collect()
    }
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    /// Number of `:` (width and precision).
    pub fn colon_count(&self) -> usize {
        tokens(&self.0, COLON).count()
    }
}

impl IndexExpr {
    pub fn base(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
    /// The items between the brackets: expressions and slices (`docs/spec/types.md` 8.7).
    pub fn indices(&self) -> Vec<IndexItem> {
        self.0
            .children()
            .skip_while(|n| Expr::cast(n.clone()).is_none())
            .skip(1)
            .filter_map(|n| match n.kind() {
                SLICE => Some(IndexItem::Slice(Slice(n))),
                _ => Expr::cast(n).map(IndexItem::Expr),
            })
            .collect()
    }
    pub fn l_brack(&self) -> Option<SyntaxToken> {
        token(&self.0, L_BRACK)
    }
}

/// One item of an index.
#[derive(Debug, Clone)]
pub enum IndexItem {
    Expr(Expr),
    Slice(Slice),
}

impl IndexItem {
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            IndexItem::Expr(e) => e.syntax(),
            IndexItem::Slice(s) => &s.0,
        }
    }
}

impl Slice {
    /// `L..H`: the bounds before and after `..` (either may be left out).
    pub fn bounds(&self) -> (Option<Expr>, Option<Expr>) {
        let Some(dots) = token(&self.0, DOTDOT) else {
            return (None, None);
        };
        let at = dots.text_range().start();
        let mut low = None;
        let mut high = None;
        for e in self.0.children().filter_map(Expr::cast) {
            if e.syntax().text_range().start() < at {
                low = Some(e);
            } else {
                high = Some(e);
            }
        }
        (low, high)
    }
}

impl FieldExpr {
    pub fn base(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    pub fn field(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn dot(&self) -> Option<SyntaxToken> {
        token(&self.0, DOT)
    }
}

impl DerefExpr {
    pub fn base(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

/// Source range of the expression (from the first to the last token, excluding trivia).
pub fn trimmed_range(node: &SyntaxNode) -> rowan::TextRange {
    let first = node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia());
    let last = node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .last();
    match (first, last) {
        (Some(f), Some(l)) => rowan::TextRange::new(f.text_range().start(), l.text_range().end()),
        _ => node.text_range(),
    }
}

/// Source text of the expression with each run of trivia replaced by a single space (for the `Assert` message).
pub fn source_text_normalized(node: &SyntaxNode) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for t in node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if t.kind().is_trivia() {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push_str(t.text());
    }
    out
}
