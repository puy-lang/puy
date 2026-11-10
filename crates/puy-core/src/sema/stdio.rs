//! Standard input and output (`docs/spec/semantics.md` chapter 6): `Write` / `WriteLn` with
//! `Output` / `ErrOutput`, `Read` / `ReadLn` with `Input`, `Flush`, `Eof` / `Eoln`.

use super::Sema;
use super::builtins::{self, StdFile};
use super::expr::{PlaceResult, PlaceUse, err_expr};
use super::hir::*;
use super::types::Ty;
use crate::diagnostics::Code;
use crate::source::TextRange;
use crate::syntax::ast::{self, AstNode};

impl Sema<'_, '_> {
    /// The standard file a plain argument names (`Output`, `System.Output`), unless a declaration
    /// of the program hides the name. No diagnostics.
    fn std_file_of(&self, a: &ast::Arg) -> Option<StdFile> {
        if a.colon_count() > 0 {
            return None;
        }
        let lower = match a.value()? {
            ast::Expr::NameRef(n) => {
                let lower = n.text().to_ascii_lowercase();
                if self.find_visible(&lower).is_some() {
                    return None;
                }
                lower
            }
            ast::Expr::Field(f) => {
                let ast::Expr::NameRef(base) = f.base()? else {
                    return None;
                };
                let unit = base.text().to_ascii_lowercase();
                if unit != "system" || self.find_visible(&unit).is_some() {
                    return None;
                }
                f.field()?.text().to_ascii_lowercase()
            }
            _ => return None,
        };
        match builtins::lookup_system(&lower) {
            Some(builtins::SystemName::StdFile(f)) => Some(f),
            _ => None,
        }
    }

    /// E0740: `Input` / `Output` / `ErrOutput` anywhere but the first argument of the standard
    /// I/O routines.
    pub(super) fn std_file_misuse(&mut self, f: StdFile, range: TextRange) {
        self.error(
            Code::E(740),
            format!(
                "`{}` can only be used as the first argument of `Write`, `WriteLn`, `Read`, `ReadLn`, `Eof`, `Eoln` or `Flush`",
                f.name()
            ),
            range,
        );
    }

    /// The wrong standard file as the first argument (`Write(Input)`, `ReadLn(Output)`; E0702).
    fn wrong_std_file(&mut self, found: StdFile, expected: &str, a: &ast::Arg) {
        let range = ast::trimmed_range(a.syntax());
        self.error(
            Code::E(702),
            format!(
                "type mismatch: expected {expected}, found `{}`",
                found.name()
            ),
            range,
        );
    }

    /// `Write` / `WriteLn` (`docs/spec/semantics.md` 6.1).
    pub(super) fn lower_write(
        &mut self,
        newline: bool,
        args: &[ast::Arg],
        checks: Checks,
    ) -> Option<StmtKind> {
        let mut args = args;
        let mut err = false;
        if let Some(first) = args.first()
            && let Some(f) = self.std_file_of(first)
        {
            match f {
                StdFile::Output => {}
                StdFile::ErrOutput => err = true,
                StdFile::Input => {
                    self.wrong_std_file(f, "`Output` or `ErrOutput`", first);
                    for a in &args[1..] {
                        for x in a.exprs() {
                            self.lower_expr(&x);
                        }
                    }
                    return None;
                }
            }
            args = &args[1..];
        }
        let mut out: Vec<WriteArg> = Vec::new();
        let mut failed = false;
        for a in args {
            match self.write_arg(a, checks) {
                Some(w) => out.push(w),
                None => failed = true,
            }
        }
        if failed {
            return None;
        }
        if out.is_empty() && !newline {
            return Some(StmtKind::Nop);
        }
        Some(StmtKind::Write {
            args: out,
            newline,
            err,
        })
    }

    /// One argument of `Write` (`X`, `X:W`, `X:W:D`).
    fn write_arg(&mut self, a: &ast::Arg, checks: Checks) -> Option<WriteArg> {
        let exprs = a.exprs();
        let v = exprs.first()?;
        let value = self.lower_expr(v);
        if value.ty.is_error() {
            return None;
        }
        // A `UTF8String` is written as its text (`docs/spec/semantics.md` 6.1)
        let value = if value.ty == Ty::Utf8String {
            self.convert_u8(value, Ty::String)
        } else {
            value
        };
        if !(value.ty.is_number()
            || matches!(value.ty, Ty::Boolean | Ty::Char | Ty::String | Ty::Enum(_)))
        {
            let r = value.span;
            self.error(
                Code::E(718),
                format!("cannot write a value of type `{}`", self.tn(value.ty)),
                r,
            );
            return None;
        }
        let mut failed = false;
        let mut width = None;
        if let Some(w) = exprs.get(1) {
            width = self.write_int_spec(w, checks);
            failed |= width.is_none();
        }
        let mut precision = None;
        if a.colon_count() >= 2 {
            if !(value.ty.is_real() || value.ty == Ty::Currency) {
                if let Some(d) = exprs.get(2) {
                    self.lower_expr(d);
                }
                // `X:W:D` is for reals and `Currency` (`docs/spec/semantics.md` 6.1)
                let r = a.syntax().text_range();
                self.error(
                    Code::E(717),
                    "precision specifier requires a floating-point value",
                    r,
                );
                return None;
            }
            match exprs.get(2) {
                Some(d) => {
                    precision = self.write_int_spec(d, checks);
                    failed |= precision.is_none();
                }
                None => failed = true,
            }
        }
        if failed {
            return None;
        }
        Some(WriteArg {
            value,
            width,
            precision,
        })
    }

    /// `W` or `D` of a `Write` argument: an integer expression, widened to `Int64` (E0702).
    fn write_int_spec(&mut self, e: &ast::Expr, checks: Checks) -> Option<Expr> {
        let x = self.lower_expr(e);
        if x.ty.is_error() {
            return None;
        }
        if !x.ty.is_integer() {
            let r = x.span;
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected `Integer`, found `{}`",
                    self.tn(x.ty)
                ),
                r,
            );
            return None;
        }
        Some(self.coerce(x, Ty::Int64, checks.range, None))
    }

    /// `Read` / `ReadLn` (`docs/spec/semantics.md` 6.2).
    pub(super) fn lower_read(
        &mut self,
        newline: bool,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        let mut skip = 0;
        if let Some(first) = args.first()
            && let Some(f) = self.std_file_of(first)
        {
            if f != StdFile::Input {
                self.wrong_std_file(f, "`Input`", first);
                for a in &args[1..] {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                return None;
            }
            skip = 1;
        }
        if !newline && args.len() == skip {
            // `Read` needs a variable (E0705)
            self.error(
                Code::E(705),
                "wrong number of arguments: expected at least 1, found 0",
                span,
            );
            return None;
        }
        let mut targets = Vec::new();
        let mut failed = false;
        for (i, a) in args.iter().enumerate().skip(skip) {
            match self.read_target(a, i + 1) {
                Some(p) => targets.push(p),
                None => failed = true,
            }
        }
        if failed {
            return None;
        }
        Some(StmtKind::Read { targets, newline })
    }

    /// A variable of `Read` / `ReadLn` (argument number `n`): a string, an integer, a real, a
    /// `Currency` or a `Char` (E0718 otherwise).
    fn read_target(&mut self, a: &ast::Arg, n: usize) -> Option<Place> {
        let v = a.value()?;
        let (place, ty) = match self.lower_place(&v, PlaceUse::ReadLn) {
            Some(x) => x,
            None => {
                if let Some(PlaceResult::NotVariable(r)) = self.last_place_result.take() {
                    self.error(
                        Code::E(706),
                        format!("argument {n} must be a variable (parameter `X` is `out`)"),
                        r,
                    );
                }
                return None;
            }
        };
        if ty.is_error() {
            return None;
        }
        if !(ty.is_integer()
            || ty.is_real()
            || matches!(ty, Ty::String | Ty::Utf8String | Ty::Currency | Ty::Char))
        {
            let r = ast::trimmed_range(v.syntax());
            self.error(
                Code::E(718),
                format!("cannot read a value of type `{}`", self.tn(ty)),
                r,
            );
            return None;
        }
        Some(place)
    }

    /// `Flush(Output)` / `Flush(ErrOutput)` (`docs/spec/semantics.md` 6.1).
    pub(super) fn lower_flush(&mut self, args: &[ast::Arg], span: TextRange) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        if args.len() != 1 {
            self.lower_args_but_files(args);
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected 1, found {}",
                    args.len()
                ),
                span,
            );
            return None;
        }
        match self.std_file_of(&args[0]) {
            Some(StdFile::Output) => Some(StmtKind::Flush),
            // Standard error is not buffered
            Some(StdFile::ErrOutput) => Some(StmtKind::Nop),
            Some(f @ StdFile::Input) => {
                self.wrong_std_file(f, "`Output` or `ErrOutput`", &args[0]);
                None
            }
            None => {
                self.not_a_std_file(&args[0], "`Output` or `ErrOutput`");
                None
            }
        }
    }

    /// Checks the arguments of a call with the wrong number of them (no E0740 for the files).
    fn lower_args_but_files(&mut self, args: &[ast::Arg]) {
        for a in args {
            if self.std_file_of(a).is_none()
                && let Some(v) = a.value()
            {
                self.lower_expr(&v);
            }
        }
    }

    /// An argument that should have been a standard file (E0702 with the type found).
    fn not_a_std_file(&mut self, a: &ast::Arg, expected: &str) {
        let v = a.value();
        let e = match &v {
            Some(v) => self.lower_expr(v),
            None => return,
        };
        if e.ty.is_error() {
            return;
        }
        let r = e.span;
        self.error(
            Code::E(702),
            format!(
                "type mismatch: expected {expected}, found `{}`",
                self.tn(e.ty)
            ),
            r,
        );
    }

    /// `Eof` / `Eoln` with parentheses: no argument or `Input` (`docs/spec/semantics.md` 6.2).
    pub(super) fn eof_call(
        &mut self,
        f: Builtin,
        args: &[ast::Arg],
        name_range: TextRange,
        span: TextRange,
    ) -> Expr {
        self.check_no_width_pub(args);
        if args.len() > 1 {
            self.lower_args_but_files(args);
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected 0 to 1, found {}",
                    args.len()
                ),
                span,
            );
            return err_expr(span);
        }
        if let Some(a) = args.first() {
            match self.std_file_of(a) {
                Some(StdFile::Input) => {}
                Some(other) => {
                    self.wrong_std_file(other, "`Input`", a);
                    return err_expr(span);
                }
                None => {
                    self.not_a_std_file(a, "`Input`");
                    return err_expr(span);
                }
            }
        }
        let name = builtins::builtin_name(f);
        self.lower_builtin_any_plain(f, name, name_range, span)
    }
}
