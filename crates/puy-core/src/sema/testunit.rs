//! The tests of a test unit (`docs/spec/attributes.md` 5.2): the checks of the attributes of the
//! testing framework, the test table, and the routine without parameters made for each test.
//!
//! The attributes are read where they are written (`attrs.rs`) and checked here, once every
//! routine of the unit is declared and analyzed. The routine made for a test is analyzed as a
//! routine of the unit, with the calls built by the analysis of calls; its parts all have the
//! position of the test's name, and the arguments of a `[TestCase]` their own.

use super::Sema;
use super::attrs::{TestAttr, TestAttrKind};
use super::hir::{
    Builtin, CallArg, Checks, Dispatch, Expr, ExprKind, Handler, Place, RoutineId, Stmt, StmtKind,
    TestInfo, TryHandler, VarId, VarKind,
};
use super::types::{Const, EXCEPTION, MethodKind, SelfKind, Ty, TypeId, TypeKind};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::TextRange;
use crate::syntax::ast::{self, AstNode};
use crate::syntax::{SyntaxKind, SyntaxNode};

/// The attributes of the testing framework written on one declaration.
struct DeclTests {
    decl: SyntaxNode,
    /// The name of the declaration (the routine's name in its heading).
    name: Option<TextRange>,
    classes: Vec<TypeId>,
    attrs: Vec<(TestAttrKind, ast::Attribute)>,
}

/// A routine with tests (`[Test]` / `[TestCase]`), checked.
struct TestRoutine {
    routine: RoutineId,
    /// The name of the routine in its declaration.
    name: TextRange,
    /// The `[TestCase]`s in the order they are written (none: one test without arguments).
    cases: Vec<ast::Attribute>,
    skip: Option<String>,
    classes: Vec<TypeId>,
}

/// What a test of a class calls around the test (`docs/spec/attributes.md` 5.2).
#[derive(Clone, Copy)]
struct TestClass {
    class: TypeId,
    setup: Option<RoutineId>,
    teardown: Option<RoutineId>,
}

/// The `[Setup]` / `[TearDown]` methods and the tests of a class.
#[derive(Default)]
struct ClassMarks {
    setup: Vec<(RoutineId, TextRange)>,
    teardown: Vec<(RoutineId, TextRange)>,
    tests: usize,
    /// The methods with `[Test]` / `[TestCase]` written, checked or not: a class with one has
    /// tests for the checks of `[Setup]` / `[TearDown]` (no E0787 after an error of the test).
    written: usize,
}

/// The marks of a class, added when it has none yet.
fn class_marks(classes: &mut Vec<(TypeId, ClassMarks)>, c: TypeId) -> &mut ClassMarks {
    let k = match classes.iter().position(|(x, _)| *x == c) {
        Some(k) => k,
        None => {
            classes.push((c, ClassMarks::default()));
            classes.len() - 1
        }
    };
    &mut classes[k].1
}

/// The written spelling of an attribute's name, and its range.
fn attr_name(a: &ast::Attribute) -> (String, TextRange) {
    match a.name() {
        Some(q) => (q.text(), q.syntax().text_range()),
        None => (String::new(), a.syntax().text_range()),
    }
}

/// The arguments written in an attribute.
fn attr_args(a: &ast::Attribute) -> Vec<ast::Arg> {
    a.args().map(|l| l.args().collect()).unwrap_or_default()
}

/// The range of an attribute's arguments with their parentheses, or of its name when it has none.
fn args_range(a: &ast::Attribute) -> TextRange {
    match a.args() {
        Some(l) => l.syntax().text_range(),
        None => attr_name(a).1,
    }
}

/// `(4, 2)`: the arguments of a `[TestCase]` as written, each run of trivia one space
/// (`docs/spec/attributes.md` 5.2, the name of a test).
fn case_text(a: &ast::Attribute) -> String {
    let args: Vec<String> = attr_args(a)
        .iter()
        .map(|x| ast::source_text_normalized(x.syntax()))
        .collect();
    format!("({})", args.join(", "))
}

impl Sema<'_, '_> {
    /// Checks the attributes of the testing framework of the test unit and makes its tests
    /// (`docs/spec/attributes.md` 5.2), in the order of their declarations.
    pub(super) fn collect_tests(&mut self) {
        self.flush_attribute_checks();
        let mut decls: Vec<DeclTests> = Vec::new();
        for TestAttr {
            kind,
            attr,
            names,
            decl,
            classes,
        } in std::mem::take(&mut self.test_attrs)
        {
            match decls.iter_mut().find(|d| d.decl == decl) {
                Some(d) => d.attrs.push((kind, attr)),
                None => decls.push(DeclTests {
                    decl,
                    name: names.first().copied(),
                    classes,
                    attrs: vec![(kind, attr)],
                }),
            }
        }
        decls.sort_by_key(|d| d.decl.text_range().start());
        let mut routines: Vec<TestRoutine> = Vec::new();
        let mut classes: Vec<(TypeId, ClassMarks)> = Vec::new();
        for d in decls {
            self.check_test_decl(d, &mut routines, &mut classes);
        }
        // The classes of the tests: one `[Setup]` and one `[TearDown]` at most, only with tests,
        // and an instance made with `Create` (`docs/spec/attributes.md` 5.2)
        let mut usable: Vec<TestClass> = Vec::new();
        for (class, marks) in &classes {
            let class_name = self.types.name(Ty::Class(*class));
            let mut ok = true;
            for (what, list) in [("Setup", &marks.setup), ("TearDown", &marks.teardown)] {
                if let [(_, first), (_, second), ..] = list.as_slice() {
                    let d = Diagnostic::error(
                        Code::E(787),
                        format!("class `{class_name}` has more than one `[{what}]` method"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(*second), "")
                    .with_secondary(self.span(*first), "the first one");
                    self.push(d);
                    ok = false;
                }
                if marks.written == 0
                    && let Some((_, at)) = list.first()
                {
                    let d = Diagnostic::error(
                        Code::E(787),
                        format!(
                            "`[{what}]` is written in class `{class_name}`, which has no tests"
                        ),
                        Stage::Sema,
                    )
                    .with_primary(self.span(*at), "")
                    .with_note("`[Setup]` and `[TearDown]` run around each test of their class");
                    self.push(d);
                    ok = false;
                }
            }
            if marks.tests > 0 && !self.test_class_creatable(*class) {
                ok = false;
            }
            if ok {
                usable.push(TestClass {
                    class: *class,
                    setup: marks.setup.first().map(|(r, _)| *r),
                    teardown: marks.teardown.first().map(|(r, _)| *r),
                });
            }
        }
        let errored: Vec<TypeId> = classes
            .iter()
            .map(|(c, _)| *c)
            .filter(|c| !usable.iter().any(|u| u.class == *c))
            .collect();
        // The written cases by name, for the ones written twice (W0027)
        let mut names: Vec<(String, TextRange)> = Vec::new();
        for t in routines {
            let class = self.routines[t.routine.0 as usize].method_of;
            if class.is_some_and(|c| errored.contains(&c)) {
                continue;
            }
            let class = class.and_then(|c| usable.iter().find(|u| u.class == c).copied());
            let base = format!(
                "{}.{}",
                self.program_name, self.routines[t.routine.0 as usize].name
            );
            let cases: Vec<Option<ast::Attribute>> = if t.cases.is_empty() {
                vec![None]
            } else {
                t.cases.iter().cloned().map(Some).collect()
            };
            for (k, case) in cases.iter().enumerate() {
                let mut name = match case {
                    Some(a) => format!("{base}{}", case_text(a)),
                    None => base.clone(),
                };
                let at = case.as_ref().map_or(t.name, |a| attr_name(a).1);
                let same: Vec<TextRange> = names
                    .iter()
                    .filter(|(n, _)| *n == name)
                    .map(|(_, r)| *r)
                    .collect();
                names.push((name.clone(), at));
                if let Some(first) = same.first() {
                    let d = Diagnostic::warning(
                        Code::W(27),
                        format!("duplicate test case `{name}`"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(at), "")
                    .with_secondary(self.span(*first), "first written here")
                    .with_note(format!(
                        "the test runs again as `{name} #{}`",
                        same.len() + 1
                    ));
                    self.push(d);
                    name = format!("{name} #{}", same.len() + 1);
                }
                let Some(thunk) = self.test_thunk(&name, &t, class, case.as_ref()) else {
                    continue;
                };
                let unit = self.unit_index;
                self.tests.push(TestInfo {
                    name,
                    unit,
                    span: t.name,
                    thunk,
                    case: case.as_ref().map(|_| k as u32),
                    skip: t.skip.clone(),
                });
            }
        }
    }

    /// The checks of the attributes of one declaration (`docs/spec/attributes.md` 5.2): where
    /// each can be written, their arguments, and the parameters of the routine. A routine with
    /// tests goes in `routines`, and the methods of a class in `classes`.
    fn check_test_decl(
        &mut self,
        d: DeclTests,
        routines: &mut Vec<TestRoutine>,
        classes: &mut Vec<(TypeId, ClassMarks)>,
    ) {
        let routine = match d.decl.kind() {
            SyntaxKind::ROUTINE_DECL
            | SyntaxKind::ROUTINE_HEADER_DECL
            | SyntaxKind::METHOD_DECL => d.name.and_then(|n| {
                self.routines
                    .iter()
                    .position(|r| r.file == self.input.file && r.name_span == n)
                    .map(|i| RoutineId(i as u32))
            }),
            _ => None,
        };
        let has = |k: TestAttrKind| d.attrs.iter().any(|(x, _)| *x == k);
        let is_test = has(TestAttrKind::Test) || has(TestAttrKind::TestCase);
        let mut ok = true;
        // Each attribute once: a second one of the same kind (other than `[TestCase]`) is the
        // same as one
        let mut seen: Vec<TestAttrKind> = Vec::new();
        for (kind, a) in &d.attrs {
            if *kind != TestAttrKind::TestCase {
                if seen.contains(kind) {
                    continue;
                }
                seen.push(*kind);
            }
            if let Some(msg) = self.misplaced_test_attr(*kind, routine, &d.decl, is_test) {
                let (text, range) = attr_name(a);
                let msg = msg.replace("{}", &text);
                let d = self.error(Code::E(785), msg, range);
                if *kind == TestAttrKind::Skip && !is_test {
                    d.notes
                        .push("`[Skip]` goes with `[Test]` or `[TestCase]`".into());
                }
                ok = false;
            }
        }
        // The arguments of the attributes other than `[TestCase]`, checked with the routine's
        // made (`test_thunk`)
        let mut skip: Option<String> = None;
        for (kind, a) in &d.attrs {
            match kind {
                TestAttrKind::Test | TestAttrKind::Setup | TestAttrKind::TearDown => {
                    if !self.language_attribute_args(a) {
                        ok = false;
                    }
                }
                TestAttrKind::Skip => match self.skip_reason(a, &d.classes) {
                    Some(r) => {
                        skip.get_or_insert(r);
                    }
                    None => ok = false,
                },
                TestAttrKind::TestCase => {}
            }
        }
        // A method with a test attribute makes its class a class with tests, even when the test
        // has an error
        if is_test && let Some(c) = routine.and_then(|r| self.routines[r.0 as usize].method_of) {
            class_marks(classes, c).written += 1;
        }
        let Some(routine) = routine.filter(|_| ok) else {
            return;
        };
        let info = &self.routines[routine.0 as usize];
        let has_params = !info.params.is_empty();
        let needs_args = info.min_args() > 0;
        let name = info.name_span;
        let class = info.method_of;
        // The parameters: a test without `[TestCase]` is called without arguments, as are
        // `[Setup]` and `[TearDown]`
        if is_test && !has(TestAttrKind::TestCase) && needs_args {
            self.error(
                Code::E(786),
                "a test with parameters needs `[TestCase]`",
                name,
            )
            .notes
            .push("each `[TestCase(…)]` runs the test with its arguments".into());
            return;
        }
        for (kind, a) in &d.attrs {
            if matches!(kind, TestAttrKind::Setup | TestAttrKind::TearDown) && has_params {
                let (text, _) = attr_name(a);
                self.error(
                    Code::E(786),
                    format!("a `[{text}]` method cannot have parameters"),
                    name,
                );
                return;
            }
        }
        if let Some(c) = class {
            let marks = class_marks(classes, c);
            for (kind, a) in &d.attrs {
                let at = attr_name(a).1;
                match kind {
                    TestAttrKind::Setup if !marks.setup.iter().any(|(r, _)| *r == routine) => {
                        marks.setup.push((routine, at));
                    }
                    TestAttrKind::TearDown
                        if !marks.teardown.iter().any(|(r, _)| *r == routine) =>
                    {
                        marks.teardown.push((routine, at));
                    }
                    _ => {}
                }
            }
            if is_test {
                marks.tests += 1;
            }
        }
        if is_test {
            routines.push(TestRoutine {
                routine,
                name,
                cases: d
                    .attrs
                    .iter()
                    .filter(|(k, _)| *k == TestAttrKind::TestCase)
                    .map(|(_, a)| a.clone())
                    .collect(),
                skip,
                classes: d.classes,
            });
        }
    }

    /// Why an attribute of the testing framework cannot be written on the declaration (the
    /// message of E0785, `{}` standing for the attribute's name), or `None` when it can.
    fn misplaced_test_attr(
        &self,
        kind: TestAttrKind,
        routine: Option<RoutineId>,
        decl: &SyntaxNode,
        is_test: bool,
    ) -> Option<&'static str> {
        let method_attr = matches!(kind, TestAttrKind::Setup | TestAttrKind::TearDown);
        let Some(r) = routine else {
            return Some(if decl.kind() == SyntaxKind::METHOD_DECL {
                "`[{}]` can only be written on an instance method of a class"
            } else if method_attr {
                "`[{}]` can only be written on a method of a class"
            } else {
                "`[{}]` can only be written on a procedure or a method"
            });
        };
        let info = &self.routines[r.0 as usize];
        match info.method_of {
            None if method_attr => {
                return Some("`[{}]` can only be written on a method of a class");
            }
            None => {}
            Some(c) => {
                let m = self.method_of_routine(r).map(|(_, m)| m);
                let is_class = matches!(self.types.get(c).kind, TypeKind::Class { .. })
                    && !self.types.is_record(c)
                    && !self.types.is_helper(c);
                let instance = m.as_ref().is_some_and(|m| {
                    matches!(m.kind, MethodKind::Procedure | MethodKind::Function)
                        && m.self_kind == SelfKind::Instance
                });
                if !is_class || !instance {
                    return Some("`[{}]` can only be written on an instance method of a class");
                }
            }
        }
        if info.ret.is_some() {
            return Some("`[{}]` cannot be written on a function");
        }
        if !info.type_params.is_empty() || info.method_of.is_some_and(|c| self.types.is_generic(c))
        {
            return Some(
                "`[{}]` cannot be written on a generic routine or a method of a generic type",
            );
        }
        if info.is_abstract {
            return Some("`[{}]` cannot be written on an abstract method");
        }
        if info.scope.is_some() {
            return Some("`[{}]` cannot be written on a method of a local type");
        }
        match kind {
            TestAttrKind::Skip if !is_test => Some("`[{}]` can only be written on a test"),
            TestAttrKind::Setup | TestAttrKind::TearDown if is_test => {
                Some("`[{}]` cannot be written on a test")
            }
            _ => None,
        }
    }

    /// The reason of a `[Skip]` (empty without one; `docs/spec/attributes.md` 5.2): a string
    /// constant. `None` after an error.
    fn skip_reason(&mut self, a: &ast::Attribute, classes: &[TypeId]) -> Option<String> {
        if a.args().is_some_and(|l| l.has_error()) {
            return None;
        }
        let args = attr_args(a);
        match args.as_slice() {
            [] => Some(String::new()),
            [x] => {
                let v = x.value()?;
                let e = self.with_class_scopes(classes, |s| s.lower_expected(&v, Ty::String));
                let range = ast::trimmed_range(v.syntax());
                if e.ty.is_error() {
                    return None;
                }
                let e = self.coerce(e, Ty::String, false, None);
                match e.value {
                    Some(Const::Str(units)) => Some(String::from_utf16_lossy(&units)),
                    Some(Const::Char(c)) => Some(String::from_utf16_lossy(&[c])),
                    _ if e.ty.is_error() => None,
                    _ => {
                        self.error(Code::E(713), "constant expression required", range);
                        None
                    }
                }
            }
            _ => {
                self.error(
                    Code::E(705),
                    format!(
                        "wrong number of arguments: expected 0 to 1, found {}",
                        args.len()
                    ),
                    args_range(a),
                );
                None
            }
        }
    }

    /// Runs `f` with the classes whose bodies enclose an attribute's declaration in scope (their
    /// nested types and constants are seen by the attribute's arguments).
    fn with_class_scopes<T>(&mut self, classes: &[TypeId], f: impl FnOnce(&mut Self) -> T) -> T {
        for c in classes {
            self.class_scopes.push(super::statics::ClassScope {
                class: *c,
                vis: super::types::Visibility::Public,
                names: Vec::new(),
            });
        }
        let r = f(self);
        for _ in classes {
            self.class_scopes.pop();
        }
        r
    }

    /// Whether an instance of a test class can be made with `Create` and no arguments, with
    /// E0788 when it cannot (`docs/spec/attributes.md` 5.2).
    fn test_class_creatable(&mut self, class: TypeId) -> bool {
        let span = self.types.get(class).span;
        let name = self.types.name(Ty::Class(class));
        let decl = self.type_decl_span(Ty::Class(class), span);
        let msg = if self.types.class_is_abstract(class) {
            format!("test class `{name}` is abstract")
        } else {
            self.begin_routine(None, None, "<test>".into(), span, false, false);
            let ok = self.dry_run(|s| {
                let n = s.sink.len();
                let e = s.new_instance(class, "Create", &[], span, span, None);
                !e.ty.is_error() && !s.sink.iter().skip(n).any(|d| d.is_error())
            });
            self.cur = None;
            if ok {
                return true;
            }
            format!("test class `{name}` cannot be created with `Create` and no arguments")
        };
        let d = Diagnostic::error(Code::E(788), msg, Stage::Sema)
            .with_primary(decl, "")
            .with_note("an instance of the test class is created with `Create` for each test");
        self.push(d);
        false
    }

    /// The routine without parameters that runs a test (`docs/spec/attributes.md` 5.2), named
    /// `<test:…>` after the test's name without the unit's; `None` when the arguments of its
    /// `[TestCase]` have an error.
    fn test_thunk(
        &mut self,
        name: &str,
        t: &TestRoutine,
        class: Option<TestClass>,
        case: Option<&ast::Attribute>,
    ) -> Option<RoutineId> {
        let at = t.name;
        let short = name
            .strip_prefix(&self.program_name)
            .and_then(|n| n.strip_prefix('.'))
            .unwrap_or(name);
        let display = format!("<test:{short}>");
        let id = RoutineId(self.routines.len() as u32);
        self.routines.push(super::RoutineInfo {
            name: display.clone(),
            params: Vec::new(),
            ret: None,
            has_body: true,
            forward_span: None,
            implicit_forward: true,
            name_span: at,
            hir: None,
            overload: false,
            overload_fix_at: None,
            unit: self.unit_index,
            file: self.input.file,
            interface: false,
            parent: None,
            scope: None,
            calls: Vec::new(),
            method_of: None,
            self_kind: SelfKind::Instance,
            is_abstract: false,
            native: false,
            body_span: None,
            closure: false,
            type_params: Vec::new(),
            closure_info: Default::default(),
        });
        self.begin_routine(Some(id), None, display, at, false, false);
        let stmts = self.test_body(t, class, case);
        let calls = std::mem::take(&mut self.cur_mut().calls);
        let failed = stmts.is_none();
        let routine = self.end_routine(stmts.unwrap_or_default(), at);
        let info = &mut self.routines[id.0 as usize];
        info.hir = Some(Box::new(routine));
        info.calls = calls;
        (!failed).then_some(id)
    }

    /// The statements of a test's routine (`docs/spec/attributes.md` 5.2). For a test of a
    /// class:
    ///
    /// ```text
    /// Obj := TFoo.Create;
    /// try                      // only for a class managed by hand
    ///   Obj.Setup;
    ///   try
    ///     Obj.Test(…);
    ///   except
    ///     on E: Exception do
    ///     begin
    ///       try
    ///         Obj.TearDown;
    ///       except
    ///         on E2: Exception do AddSuppressed(E, E2);
    ///       end;
    ///       raise;
    ///     end;
    ///   end;
    ///   Obj.TearDown;
    /// finally
    ///   Obj.Free;
    /// end;
    /// ```
    fn test_body(
        &mut self,
        t: &TestRoutine,
        class: Option<TestClass>,
        case: Option<&ast::Attribute>,
    ) -> Option<Vec<Stmt>> {
        let at = t.name;
        let stmt = |kind: StmtKind| Stmt { kind, span: at };
        let Some(tc) = class else {
            let call = self.test_call(t.routine, None, case, &t.classes)?;
            return Some(vec![stmt(StmtKind::Expr(call))]);
        };
        let ty = Ty::Class(tc.class);
        let obj = self.add_var("Obj".into(), ty, VarKind::Local, at);
        let new = self.new_instance(tc.class, "Create", &[], at, at, None);
        if new.ty.is_error() {
            return None;
        }
        let var = |v: VarId, ty: Ty| Expr {
            kind: ExprKind::Var(v),
            ty,
            value: None,
            span: at,
        };
        let mut body = vec![stmt(StmtKind::Assign {
            target: Place::Var(obj, at),
            value: new,
            check: Checks {
                overflow: false,
                range: false,
            },
            is_init: false,
        })];
        let mut inner: Vec<Stmt> = Vec::new();
        if let Some(s) = tc.setup {
            let call = self.test_call(s, Some(var(obj, ty)), None, &[])?;
            inner.push(stmt(StmtKind::Expr(call)));
        }
        let test = self.test_call(t.routine, Some(var(obj, ty)), case, &t.classes)?;
        match tc.teardown {
            None => inner.push(stmt(StmtKind::Expr(test))),
            Some(td) => {
                let exc = Ty::Class(EXCEPTION);
                let e = self.add_var("E".into(), exc, VarKind::Exception, at);
                let e2 = self.add_var("E2".into(), exc, VarKind::Exception, at);
                let teardown = |s: &mut Self| s.test_call(td, Some(var(obj, ty)), None, &[]);
                let first = teardown(self)?;
                let second = teardown(self)?;
                let add = Expr {
                    kind: ExprKind::Builtin {
                        func: Builtin::AddSuppressed,
                        args: vec![var(e, exc), var(e2, exc)],
                    },
                    ty: Ty::Boolean,
                    value: None,
                    span: at,
                };
                let quiet = stmt(StmtKind::Try {
                    body: vec![stmt(StmtKind::Expr(first))],
                    handler: TryHandler::Except {
                        handlers: vec![Handler {
                            var: Some(e2),
                            class: exc,
                            body: Some(Box::new(stmt(StmtKind::Expr(add)))),
                            span: at,
                        }],
                        else_body: None,
                    },
                });
                let on_error = stmt(StmtKind::Block(vec![quiet, stmt(StmtKind::Raise(None))]));
                inner.push(stmt(StmtKind::Try {
                    body: vec![stmt(StmtKind::Expr(test))],
                    handler: TryHandler::Except {
                        handlers: vec![Handler {
                            var: Some(e),
                            class: exc,
                            body: Some(Box::new(on_error)),
                            span: at,
                        }],
                        else_body: None,
                    },
                }));
                inner.push(stmt(StmtKind::Expr(second)));
            }
        }
        let manual = matches!(
            self.types.get(tc.class).kind,
            TypeKind::Class {
                ref_counted: false,
                ..
            }
        );
        if manual {
            body.push(stmt(StmtKind::Try {
                body: inner,
                handler: TryHandler::Finally(vec![stmt(StmtKind::Free {
                    target: var(obj, ty),
                    destroy: false,
                })]),
            }));
        } else {
            // The variable lets go of the instance when the routine ends
            body.extend(inner);
        }
        Some(body)
    }

    /// The call of a test, `[Setup]` or `[TearDown]` routine: of a method on `recv`, with the
    /// arguments of the `[TestCase]` checked as those of a call (`docs/spec/attributes.md` 5.2)
    /// and required to be constants (E0713). `None` after an error.
    fn test_call(
        &mut self,
        routine: RoutineId,
        recv: Option<Expr>,
        case: Option<&ast::Attribute>,
        classes: &[TypeId],
    ) -> Option<Expr> {
        let at = self.routines[routine.0 as usize].name_span;
        let (args, range) = match case {
            Some(a) => {
                if a.args().is_some_and(|l| l.has_error()) {
                    return None;
                }
                (attr_args(a), args_range(a))
            }
            None => (Vec::new(), at),
        };
        let call = self.with_class_scopes(classes, |s| s.lower_routine_call(routine, &args, range));
        let ExprKind::Call {
            args: mut values, ..
        } = call.kind
        else {
            return None;
        };
        let mut ok = true;
        for (a, v) in args.iter().zip(&values) {
            let constant = matches!(v, CallArg::Value(e) if e.value.is_some());
            if !constant {
                let r = a.value().map_or_else(
                    || a.syntax().text_range(),
                    |v| ast::trimmed_range(v.syntax()),
                );
                self.error(Code::E(713), "constant expression required", r);
                ok = false;
            }
        }
        if !ok {
            return None;
        }
        let dispatch = match recv {
            Some(r) => {
                values.insert(0, CallArg::Value(r));
                Dispatch::Method
            }
            None => Dispatch::Routine,
        };
        Some(Expr {
            kind: ExprKind::Call {
                routine,
                args: values,
                dispatch,
            },
            ty: Ty::Error,
            value: None,
            span: call.span,
        })
    }
}
