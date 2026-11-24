//! The built-ins of `TValue` (`docs/spec/rtti.md` chapter 3, `docs/spec/bytecode.md` 2.24): the
//! value made a `TValue` by its type (`ValueWrap`), taken back by the type asked for
//! (`ValueIs`, `ValueCast`), and the intrinsics the part of `System` written in Pascal calls.

use super::*;

/// The slots of a `TValue` (`docs/spec/rtti.md` 3.1): the type information number, the value of a
/// type not managed, the reference of a managed one.
const FIELD_TYPE: u32 = 0;
const FIELD_BITS: u32 = 1;
const FIELD_REF: u32 = 2;

/// The modes of `ValueRead` (`I123`) the reads of `Double` and `Currency` use.
const READ_DOUBLE: i64 = 4;
const READ_CURRENCY: i64 = 5;

/// The modes of `ValueArray` (`I128`).
const ARRAY_LENGTH: i64 = 0;
const ARRAY_TYPE: i64 = 1;
const ARRAY_BITS: i64 = 2;
const ARRAY_REF: i64 = 3;
const ARRAY_ELEM_TYPE: i64 = 4;
const ARRAY_SET: i64 = 5;
const ARRAY_SET_VALUE: i64 = 6;

impl FuncLower<'_, '_> {
    /// The `TValue` built-ins into `dst`.
    pub(super) fn value_builtin(&mut self, func: Builtin, args: &[Expr], ty: Ty, dst: Reg) {
        match func {
            Builtin::ValueWrap => self.value_wrap(args, ty, dst),
            Builtin::ValueIs => {
                let ExprKind::TypeInfo(t) = args[0].kind else {
                    unreachable!("sema gives `ValueIs` the type information of its type")
                };
                if self.program.types.is_tvalue(t) {
                    // `AsType<TValue>` always holds (`docs/spec/rtti.md` 3.5)
                    self.expr_any(&args[2]);
                    self.load_int(dst, 1);
                    return;
                }
                let v = self.borrowed_value(&args[1]);
                let regs = self.value_handle_args(v, &args[0]);
                self.expr(&args[2], regs[3]);
                self.intrinsic(
                    op::intr::VALUE_MATCH,
                    &[dst, regs[0], regs[1], regs[2], regs[3]],
                );
            }
            Builtin::ValueMatch => {
                // The value's own bits or reference (one of them is 0) as the handle
                let regs: Vec<Reg> = (0..4)
                    .map(|i| self.alloc_arg(Some(if i == 0 { Ty::Integer } else { Ty::Int64 })))
                    .collect();
                self.borrowed_into(&args[0], regs[0]);
                let bits = self.alloc_borrow(Ty::Int64);
                let r = self.alloc_borrow(Ty::Int64);
                self.borrowed_into(&args[1], bits);
                self.borrowed_into(&args[2], r);
                self.op3(op::OR_INT, regs[1], bits, r);
                self.borrowed_into(&args[3], regs[2]);
                self.borrowed_into(&args[4], regs[3]);
                self.intrinsic(
                    op::intr::VALUE_MATCH,
                    &[dst, regs[0], regs[1], regs[2], regs[3]],
                );
            }
            Builtin::ValueCast => self.value_cast(args, ty, dst),
            Builtin::ValueToString | Builtin::ValueReadString => {
                let regs = self.value_args(args, None);
                let id = if func == Builtin::ValueToString {
                    op::intr::VALUE_TO_STRING
                } else {
                    op::intr::VALUE_READ_STRING
                };
                self.intrinsic(id, &[dst, regs[0], regs[1], regs[2]]);
            }
            Builtin::ValueRead | Builtin::ValueReadDouble | Builtin::ValueReadCurrency => {
                let mode = match func {
                    Builtin::ValueReadDouble => Some(READ_DOUBLE),
                    Builtin::ValueReadCurrency => Some(READ_CURRENCY),
                    _ => None,
                };
                let regs = self.value_args(args, mode);
                self.intrinsic(
                    op::intr::VALUE_READ,
                    &[dst, regs[0], regs[1], regs[2], regs[3]],
                );
            }
            Builtin::TypeKindOf => {
                let x = self.expr_any(&args[0]);
                self.intrinsic(op::intr::TYPE_KIND_OF, &[dst, x]);
            }
            Builtin::ValueArrayLength
            | Builtin::ValueArrayType
            | Builtin::ValueArrayBits
            | Builtin::ValueArrayRef
            | Builtin::ValueArrayElemType
            | Builtin::ValueArraySet
            | Builtin::ValueArrayValue
            | Builtin::ValueArraySetValue => self.value_array(func, args, dst),
            Builtin::FormatValues => {
                // `docs/spec/rtti.md` 4.2: the format and the array, read without counting
                let regs = [
                    self.alloc_arg(Some(Ty::String)),
                    self.alloc_arg(Some(args[1].ty)),
                ];
                self.borrowed_into(&args[0], regs[0]);
                self.borrowed_into(&args[1], regs[1]);
                self.intrinsic(op::intr::FORMAT_VALUES, &[dst, regs[0], regs[1]]);
            }
            _ => unreachable!("not a built-in of `TValue`"),
        }
    }

    /// The arguments `AType, ABits, ARef` (and the mode, written or given) in consecutive
    /// registers, read without counting.
    fn value_args(&mut self, args: &[Expr], mode: Option<i64>) -> Vec<Reg> {
        let n = args.len() + usize::from(mode.is_some());
        let regs: Vec<Reg> = (0..n)
            .map(|i| self.alloc_arg(args.get(i).map(|a| a.ty).or(Some(Ty::Integer))))
            .collect();
        for (a, r) in args.iter().zip(&regs) {
            self.borrowed_into(a, *r);
        }
        if let Some(m) = mode {
            self.load_int(regs[n - 1], m);
        }
        regs
    }

    /// The registers of `ValueMatch` for the `TValue` block `v` and the type information `info`
    /// of the target: the type, the handle (the bits or the reference, one of them 0), the target,
    /// and the register of the mode (left for the caller).
    fn value_handle_args(&mut self, v: Reg, info: &Expr) -> [Reg; 4] {
        let regs = [
            self.alloc_arg(Some(Ty::Integer)),
            self.alloc_arg(Some(Ty::Int64)),
            self.alloc_arg(Some(Ty::Integer)),
            self.alloc_arg(Some(Ty::Boolean)),
        ];
        let bits = self.alloc_borrow(Ty::Int64);
        let r = self.alloc_borrow(Ty::Int64);
        self.get_slot(regs[0], v, &At::Field(FIELD_TYPE), Ty::Integer, false);
        self.get_slot(bits, v, &At::Field(FIELD_BITS), Ty::Int64, false);
        self.get_slot(r, v, &At::Field(FIELD_REF), Ty::ValueRef, false);
        self.op3(op::OR_INT, regs[1], bits, r);
        self.expr(info, regs[2]);
        regs
    }

    /// `ValueWrap(X, TypeInfo(T))` (`docs/spec/rtti.md` 3.1, 3.3): a new `TValue` block whose
    /// `FType` is the type information number of `T` (of the instance's class for an instance),
    /// and whose `FBits` or `FRef` holds `X` by the representation of its type.
    fn value_wrap(&mut self, args: &[Expr], tvalue: Ty, dst: Reg) {
        let x_ty = args[0].ty;
        if self.program.types.is_tvalue(x_ty) {
            // Not nested (3.3)
            self.expr(&args[0], dst);
            return;
        }
        let types = &self.program.types;
        let kind = types.slot_kind(x_ty);
        let class = matches!(x_ty, Ty::Class(_));
        let x = self.expr_any(&args[0]);
        let ft = self.alloc_borrow(Ty::Integer);
        if class {
            // The class of the instance (`nil`: the type's own; `ValueOfObject`)
            let regs = [
                self.alloc_arg(Some(Ty::Int64)),
                self.alloc_arg(Some(Ty::Integer)),
            ];
            self.op2(op::MOVE, regs[0], x);
            self.expr(&args[1], regs[1]);
            self.intrinsic(op::intr::VALUE_OF_OBJECT, &[ft, regs[0], regs[1]]);
        } else {
            self.expr(&args[1], ft);
        }
        let t = self.alloc(tvalue);
        self.clear_value(tvalue, t);
        self.op_table(op::UNIQUE, t, Operand::Type(tvalue));
        self.set_slot(t, &At::Field(FIELD_TYPE), ft, Ty::Integer, false);
        match kind {
            bytecode::SLOT_REF => self.set_slot(t, &At::Field(FIELD_REF), x, Ty::ValueRef, true),
            bytecode::SLOT_OBJ => {
                // `TObject` and the interfaces look at the instance (`AssignObj`'s test;
                // `docs/spec/objects.md` 4.2): with `c` 1 for an instance of a reference counted
                // class and 0 otherwise, the bits are `X and (c - 1)` (`X` or 0) and the reference
                // `X and -c` (0 or `X`)
                let c = self.alloc_borrow(Ty::Int64);
                self.intrinsic(op::intr::IS_REF_OBJ, &[c, x]);
                let mask = self.alloc_borrow(Ty::Int64);
                let part = self.alloc_borrow(Ty::Int64);
                self.op_imm(op::ADD_INT_I, mask, c, -1);
                self.op3(op::AND_INT, part, x, mask);
                self.set_slot(t, &At::Field(FIELD_BITS), part, Ty::Int64, false);
                self.op2(op::NEG_INT, mask, c);
                self.op3(op::AND_INT, part, x, mask);
                self.set_slot(t, &At::Field(FIELD_REF), part, Ty::ValueRef, true);
            }
            _ => self.set_slot(t, &At::Field(FIELD_BITS), x, Ty::Int64, false),
        }
        self.move_value(tvalue, dst, t);
    }

    /// `ValueCast(T, V)` (`docs/spec/rtti.md` 3.5): the value of the `TValue` `V` as a `T`, by
    /// the representation of `T` (after `ValueIs`; the empty `TValue` gives the default value).
    fn value_cast(&mut self, args: &[Expr], ty: Ty, dst: Reg) {
        if self.program.types.is_tvalue(ty) {
            self.expr(&args[1], dst);
            return;
        }
        let v = self.borrowed_value(&args[1]);
        match self.program.types.slot_kind(ty) {
            bytecode::SLOT_REF => self.get_slot(dst, v, &At::Field(FIELD_REF), ty, true),
            bytecode::SLOT_OBJ => {
                // One of the two is 0
                let bits = self.alloc_borrow(Ty::Int64);
                let r = self.alloc_borrow(Ty::Int64);
                self.get_slot(bits, v, &At::Field(FIELD_BITS), Ty::Int64, false);
                self.get_slot(r, v, &At::Field(FIELD_REF), Ty::ValueRef, false);
                self.op3(op::OR_INT, bits, bits, r);
                self.copy_value(ty, dst, bits);
            }
            _ => self.get_slot(dst, v, &At::Field(FIELD_BITS), ty, false),
        }
    }

    /// The array members of `TValue` (`ValueArray`, `I128`): `AType`, `ARef`, the index, the mode
    /// and the value to write (`VBits or VRef`) in consecutive registers.
    fn value_array(&mut self, func: Builtin, args: &[Expr], dst: Reg) {
        let regs = [
            self.alloc_arg(Some(Ty::Integer)),
            self.alloc_arg(Some(Ty::ValueRef)),
            self.alloc_arg(Some(Ty::Integer)),
            self.alloc_arg(Some(Ty::Integer)),
            self.alloc_arg(Some(Ty::Int64)),
        ];
        self.borrowed_into(&args[0], regs[0]);
        self.borrowed_into(&args[1], regs[1]);
        match args.get(2) {
            Some(i) => self.borrowed_into(i, regs[2]),
            None => self.op1(op::LOAD_ZERO, regs[2]),
        }
        let mode = match func {
            Builtin::ValueArrayLength => ARRAY_LENGTH,
            Builtin::ValueArrayType => ARRAY_TYPE,
            Builtin::ValueArrayBits => ARRAY_BITS,
            Builtin::ValueArrayRef => ARRAY_REF,
            Builtin::ValueArrayElemType => ARRAY_ELEM_TYPE,
            // The element's box (a `TValue` laid out flat)
            Builtin::ValueArrayValue => ARRAY_REF,
            Builtin::ValueArraySetValue => ARRAY_SET_VALUE,
            _ => ARRAY_SET,
        };
        self.load_int(regs[3], mode);
        if func == Builtin::ValueArraySet {
            let bits = self.alloc_borrow(Ty::Int64);
            let r = self.alloc_borrow(Ty::Int64);
            self.borrowed_into(&args[3], bits);
            self.borrowed_into(&args[4], r);
            self.op3(op::OR_INT, regs[4], bits, r);
        } else if func == Builtin::ValueArraySetValue {
            // The `TValue` block, read without counting
            self.borrowed_into(&args[3], regs[4]);
        } else {
            self.op1(op::LOAD_ZERO, regs[4]);
        }
        self.intrinsic(
            op::intr::VALUE_ARRAY,
            &[dst, regs[0], regs[1], regs[2], regs[3], regs[4]],
        );
    }
}
