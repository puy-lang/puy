//! Bodies of the instructions that both the fast path (`Isolate::run_loop`) and `Isolate::step`
//! run (`docs/design/vm.md` 3.14).
//!
//! Each body reads its operands as register indices and immediates (already decoded, with the
//! frame's base added), does the whole work of the instruction, and reports a failure as a
//! `RefError` instead of ending the step itself: `RefError::Rt` is a Pascal exception (raised at the
//! instruction, whose `pc` the caller writes back), `RefError::Fatal` an internal error. A body
//! never changes the frames or `pc`, so the fast path keeps going in the same frame after it.

use super::*;

/// `DivInt` / `DivIntChk` / `ModInt` / `DivUInt` / `ModUInt` on `n` and `d`
/// (`docs/spec/bytecode.md` 2.2): a zero divisor is always `EDivByZero`.
pub(super) fn int_div(opc: u8, n: Slot, d: Slot) -> Result<Slot, RtError> {
    if d == 0 {
        return Err(rt(exc::DIV_BY_ZERO, "division by zero"));
    }
    Ok(match opc {
        op::DIV_INT_CHK if n == i64::MIN && d == -1 => return Err(int_overflow("Int64")),
        op::DIV_INT | op::DIV_INT_CHK => n.wrapping_div(d),
        op::MOD_INT => n.wrapping_rem(d),
        op::DIV_UINT => ((n as u64) / (d as u64)) as Slot,
        op::MOD_UINT => ((n as u64) % (d as u64)) as Slot,
        _ => unreachable!("not a division"),
    })
}

/// The integer operations with an immediate (`docs/spec/bytecode.md` 2.2) other than `AddIntI`,
/// `DivIntI` and `ModIntI`: `x` is `rB`, `imm` is `sC`. The same results as the register forms
/// with `imm` in `rC`.
#[inline(always)]
pub(super) fn int_imm(opc: u8, x: Slot, imm: i64) -> Slot {
    match opc {
        op::MUL_INT_I => x.wrapping_mul(imm),
        op::AND_INT_I => x & imm,
        op::OR_INT_I => x | imm,
        op::XOR_INT_I => x ^ imm,
        op::SHL_INT_I => x.wrapping_shl((imm & 63) as u32),
        op::SHR_INT_I => ((x as u64).wrapping_shr((imm & 63) as u32)) as i64,
        op::SHL_INT32_I => ((x as i32).wrapping_shl((imm & 31) as u32)) as i64,
        op::SHR_INT32_I => (((x as u32).wrapping_shr((imm & 31) as u32)) as i32) as i64,
        op::ADD_INT32_I => int32(op::ADD_INT32, x, imm),
        op::MUL_INT32_I => int32(op::MUL_INT32, x, imm),
        _ => unreachable!("not an integer operation with an immediate"),
    }
}

/// `AddInt32` / `SubInt32` / `MulInt32` (`docs/spec/bytecode.md` 2.2): the operation on the low 32
/// bits of `x` and `y`, wrapping, sign-extended (an `Integer`). The same result as the 64-bit
/// operation followed by `Narrow A A 4`.
#[inline(always)]
pub(super) fn int32(opc: u8, x: Slot, y: Slot) -> Slot {
    let (x, y) = (x as i32, y as i32);
    i64::from(match opc {
        op::ADD_INT32 => x.wrapping_add(y),
        op::SUB_INT32 => x.wrapping_sub(y),
        op::MUL_INT32 => x.wrapping_mul(y),
        _ => unreachable!("not a 32-bit integer operation"),
    })
}

/// Whether a compare-and-branch with an immediate (`docs/spec/bytecode.md` 2.3) jumps: `x` is
/// `rA`, `imm` is `sB` (signed).
#[inline(always)]
pub(super) fn cmp_imm(opc: u8, x: Slot, imm: i64) -> bool {
    match opc {
        op::JMP_IF_EQ_INT_I => x == imm,
        op::JMP_IF_NE_INT_I => x != imm,
        op::JMP_IF_LT_INT_I => x < imm,
        op::JMP_IF_LE_INT_I => x <= imm,
        op::JMP_IF_GT_INT_I => x > imm,
        op::JMP_IF_GE_INT_I => x >= imm,
        _ => unreachable!("not a compare-and-branch with an immediate"),
    }
}

/// `ForLoopUp` / `ForLoopDown` (`docs/spec/bytecode.md` 2.3) on the counter `x` and the limit
/// `limit`: the next counter when the loop goes on (the counter is not the limit), `None` when it
/// ends.
#[inline(always)]
pub(super) fn for_step(opc: u8, x: Slot, limit: Slot) -> Option<Slot> {
    if x == limit {
        return None;
    }
    Some(if opc == op::FOR_LOOP_UP {
        x.wrapping_add(1)
    } else {
        x.wrapping_sub(1)
    })
}

/// The `EIntOverflow` of an arithmetic instruction on the type `t`.
pub(super) fn int_overflow(t: &str) -> RtError {
    rt(
        exc::INT_OVERFLOW,
        format!("integer overflow in {t} arithmetic"),
    )
}

/// The `ERangeError` of a string index outside `1..len`.
fn str_index_error(i: i64, len: usize) -> RefError {
    RefError::Rt(rt(
        exc::RANGE_ERROR,
        format!("string index {i} is out of range 1..{len}"),
    ))
}

impl From<OutOfMemory> for RefError {
    fn from(_: OutOfMemory) -> Self {
        RefError::Rt(oom())
    }
}

impl From<heap::BadHandle> for RefError {
    fn from(e: heap::BadHandle) -> Self {
        match e {
            heap::BadHandle::Nil => RefError::Rt(rt(exc::ACCESS_VIOLATION, NIL_ACCESS)),
            heap::BadHandle::Freed => RefError::Rt(rt(exc::ACCESS_VIOLATION, FREED_ACCESS)),
            heap::BadHandle::Fatal(m) => RefError::Fatal(m),
        }
    }
}

impl From<RtError> for RefError {
    fn from(e: RtError) -> Self {
        RefError::Rt(e)
    }
}

impl Isolate {
    /// Runs one of the instructions below, or returns `None` for any other instruction (and for
    /// `LoadConst` of a constant not made yet). The operands are decoded: `ra` / `rb` / `rc` are
    /// register indices with the frame's base added, `b` / `c` / `bx` the raw fields (the fast
    /// path passes the narrow forms, step the `Wide` ones too).
    ///
    /// It is not inlined, so the fast path's own `match` stays small (the register-only
    /// instructions there slowed down when these bodies were inlined into it; the record in
    /// `docs/notes/benchmark-m2.md`).
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn exec_op(
        &mut self,
        opc: u8,
        ra: usize,
        rb: usize,
        rc: usize,
        b: usize,
        c: usize,
        bx: usize,
    ) -> Option<Result<(), RefError>> {
        let r = match opc {
            op::LOAD_CONST => match self.op_load_cached_const(ra, bx) {
                Ok(true) => Ok(()),
                Ok(false) => return None,
                Err(e) => Err(e),
            },
            op::ASSIGN_REF => self.op_assign_ref(ra, rb),
            op::MOVE_REF => self.op_move_ref(ra, rb),
            op::ADD_REF => self.op_add_ref(ra),
            op::RELEASE => self.op_release(ra),
            op::ASSIGN_OBJ => self.op_assign_obj(ra, rb),
            op::LOAD_WEAK => self.op_load_weak(ra, rb),
            op::ADD_REF_OBJ => self.heap.add_ref_obj(self.regs[ra]).map_err(RefError::from),
            op::RELEASE_OBJ => {
                let old = std::mem::take(&mut self.regs[ra]);
                self.heap.release_obj(old).map_err(RefError::from)
            }
            op::LOAD_GLOBAL_REF => self.op_load_global_ref(ra, bx),
            op::STORE_GLOBAL_REF => self.op_store_global_ref(ra, bx),
            op::STR_LEN => self.op_str_len(ra, rb),
            op::CONCAT_STR => self.op_concat_str(ra, rb, rc),
            op::CONCAT_STR_N => self.op_concat_str_n(ra, rb, c),
            op::INT_TO_STR | op::UINT_TO_STR | op::BOOL_TO_STR => self.op_int_to_str(opc, ra, rb),
            op::STR_COPY => self.op_str_copy(ra, rb),
            op::STR_POS => self.op_str_pos(ra, rb),
            op::STR_UPPER_ASCII | op::STR_LOWER_ASCII => {
                self.op_str_case(ra, rb, opc == op::STR_UPPER_ASCII)
            }
            op::APPEND_STR => self.op_append_str(ra, rb, rc),
            op::APPEND_U8 => self.op_append_u8(ra, rb, rc),
            op::STR_INDEX => self.op_str_index(ra, rb, rc),
            op::STR_SET_INDEX => self.op_str_set_index(ra, rb, rc),
            op::CMP_STR => self
                .str_compare(self.regs[rb], self.regs[rc])
                .map(|o| self.regs[ra] = o),
            op::EQ_STR => self
                .str_equal(self.regs[rb], self.regs[rc])
                .map(|e| self.regs[ra] = e as Slot),
            // `docs/spec/equality.md` 8.1
            op::HASH_INT => {
                self.regs[ra] = self.hash_keys.int(self.regs[rb] as u64);
                Ok(())
            }
            op::HASH_F64 => {
                self.regs[ra] = self.hash_keys.f64(slot_f64(self.regs[rb]));
                Ok(())
            }
            op::HASH_STR => self.op_hash_str(ra, rb),
            op::HASH_MIX => {
                self.regs[ra] = self
                    .hash_keys
                    .mix(self.regs[rb] as u64, self.regs[rc] as u64);
                Ok(())
            }
            op::KEY_EQ_F64 => {
                let eq = hash::key_eq_f64(slot_f64(self.regs[rb]), slot_f64(self.regs[rc]));
                self.regs[ra] = eq as Slot;
                Ok(())
            }
            op::ARR_LEN => self.op_arr_len(ra, rb),
            op::DYN_GET | op::DYN_GET_REF => self.op_dyn_get(ra, rb, rc, opc == op::DYN_GET_REF),
            op::DYN_SET | op::DYN_SET_REF => self.op_dyn_set(ra, rb, rc, opc == op::DYN_SET_REF),
            op::FIELD_GET | op::FIELD_GET_REF => {
                self.op_slot_get(ra, rb, c as i64, opc == op::FIELD_GET_REF)
            }
            op::VAL_GET | op::VAL_GET_REF => {
                self.op_slot_get(ra, rb, self.regs[rc], opc == op::VAL_GET_REF)
            }
            op::FIELD_SET | op::FIELD_SET_REF => {
                self.op_slot_set(ra, b as i64, rc, opc == op::FIELD_SET_REF)
            }
            op::VAL_SET | op::VAL_SET_REF => {
                self.op_slot_set(ra, self.regs[rb], rc, opc == op::VAL_SET_REF)
            }
            op::UNIQUE => self.op_unique(ra, bx as u32),
            op::LOAD_REF => self.ref_read(self.regs[rb]).map(|v| self.regs[ra] = v),
            op::LOAD_REF_REF => self.op_load_ref_ref(ra, rb),
            op::STORE_REF => self.op_store_ref(ra, rb),
            op::STORE_REF_REF | op::CLEAR_REF => {
                let v = if opc == op::CLEAR_REF {
                    0
                } else {
                    self.regs[rb]
                };
                self.op_store_ref_ref(ra, v)
            }
            op::LEND => self.op_lend(ra, rb, c),
            op::UNLEND => self.op_unlend(),
            op::NEW => self.op_new(ra, bx as u32),
            op::IS_CLASS => self.op_is_class(ra, bx as u32),
            op::AS_CLASS => self.op_as_class(ra, bx as u32),
            op::OBJ_GET | op::OBJ_GET_REF => self.op_obj_get(ra, rb, c, opc == op::OBJ_GET_REF),
            op::OBJ_SET | op::OBJ_SET_REF => self.op_obj_set(ra, b, rc, opc == op::OBJ_SET_REF),
            // The end of a `finally` body reached normally with no `Jmp` after it (the creation of
            // an instance; `docs/spec/objects.md` 14.3): nothing to do. The other states change
            // `pc`, which step does
            op::END_FINALLY if b == 0 && self.regs[ra] == FINALLY_NORMAL => Ok(()),
            op::CAP_GET => self.op_cap_get(ra, rb, c),
            op::CELL_GET => self
                .heap
                .cell_get(self.regs[rb])
                .map(|v| self.regs[ra] = v)
                .map_err(RefError::from),
            op::CELL_GET_REF => match self.heap.cell_get(self.regs[rb]) {
                Ok(v) => self.share(ra, v),
                Err(e) => Err(e.into()),
            },
            op::CELL_SET => {
                let v = self.regs[rb];
                self.heap
                    .cell_mut(self.regs[ra])
                    .map(|slot| *slot = v)
                    .map_err(RefError::from)
            }
            op::CELL_SET_REF => self.op_cell_set_ref(ra, rb),
            op::INTRINSIC => match u8::try_from(c) {
                Ok(op::intr::CELL_REF) => self.op_cell_ref(ra, rb),
                Ok(id @ (op::intr::NEW_CELL | op::intr::NEW_CELL_REF | op::intr::NEW_CELL_OBJ)) => {
                    self.op_new_cell(ra, id)
                }
                Ok(op::intr::MAKE_CLOSURE) => self.op_make_closure(ra, rb),
                _ => return None,
            },
            _ => return None,
        };
        Some(r)
    }

    /// `CapGet A B C`: `rA :=` captured cell `C` of the closure `rB`, uncounted
    /// (`docs/spec/objects.md` 14.9).
    fn op_cap_get(&mut self, ra: usize, rb: usize, i: usize) -> Result<(), RefError> {
        let r = self.regs[rb];
        let Some(v) = self.heap.closure_cap(r, i) else {
            // Why not: not a live closure, or past the end of its list
            self.heap.closure_caps(r)?;
            return Err(RefError::Fatal(format!("capture {i} out of range")));
        };
        self.regs[ra] = v;
        Ok(())
    }

    /// `CellSetRef A B`: the value of the cell `rA` := `rB`, counting `rB` before letting go of
    /// the old value.
    fn op_cell_set_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.regs[rb];
        self.heap.check_share(v)?;
        self.heap.add_ref(v)?;
        let old = std::mem::replace(self.heap.cell_mut(self.regs[ra])?, v);
        Ok(self.heap.release(old)?)
    }

    /// The `CellRef` built-in (`docs/spec/objects.md` 14.9): `rA :=` a reference slot of kind 3
    /// to the cell `rB`.
    fn op_cell_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let r = self.regs[rb];
        if !heap::is_block_ref(r) {
            return Err(RefError::Fatal(format!("bad cell {r}")));
        }
        self.regs[ra] = make_ref(3, r as u64);
        Ok(())
    }

    /// The `NewCell` / `NewCellRef` / `NewCellObj` built-ins: `rA :=` a new cell whose value is of
    /// the kind the built-in names, letting go of the old `rA` (`docs/spec/objects.md` 14.9). The
    /// cell remembers the register for the leak report.
    fn op_new_cell(&mut self, ra: usize, id: u8) -> Result<(), RefError> {
        let kind = match id {
            op::intr::NEW_CELL => bytecode::SLOT_PLAIN,
            op::intr::NEW_CELL_REF => bytecode::SLOT_REF,
            _ => bytecode::SLOT_OBJ,
        };
        let frame = *self.frames.last().expect("an instruction runs in a frame");
        let reg = (ra - frame.base as usize) as u16;
        let r = self.heap.alloc_cell(kind, frame.func, reg)?;
        Ok(self.put(ra, r)?)
    }

    /// The `MakeClosure` built-in (`docs/spec/objects.md` 14.9): `rA :=` a new closure of the
    /// function `rB` (`-1`: none, a group block) capturing the `r(B+1)` cells from `r(B+2)`,
    /// which it counts.
    fn op_make_closure(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let f = self.regs[rb];
        let func = if f == -1 {
            heap::NO_FUNC
        } else {
            match u32::try_from(f)
                .ok()
                .filter(|f| (*f as usize) < self.module.funcs.len())
            {
                Some(f) => f,
                None => return Err(RefError::Fatal(format!("bad function index {f}"))),
            }
        };
        let n = self.regs[rb + 1];
        let frame = *self.frames.last().expect("an instruction runs in a frame");
        let end = frame.base as usize + self.module.funcs[frame.func as usize].regs as usize;
        // Compared without overflow (`usize` is 32 bits on wasm32)
        let Some(n) = usize::try_from(n)
            .ok()
            .filter(|n| *n <= end.saturating_sub(rb + 2))
        else {
            return Err(RefError::Fatal(format!("bad capture count {n}")));
        };
        let list = heap::Caps::new(&self.regs[rb + 2..rb + 2 + n])?;
        for &cell in list.iter() {
            self.heap.add_ref(cell)?;
        }
        let r = match self.heap.alloc_closure(func, list) {
            Ok(r) => r,
            Err(OutOfMemory) => {
                for i in rb + 2..rb + 2 + n {
                    let cell = self.regs[i];
                    self.heap.release(cell)?;
                }
                return Err(OutOfMemory.into());
            }
        };
        Ok(self.put(ra, r)?)
    }

    /// `New A Bx`: `rA :=` a new instance of the class `Bx` (`docs/spec/objects.md` 14.3). A
    /// making instruction: the old value of `rA` is let go of by its contents (a handle of a
    /// class managed by hand, or 0, holds nothing).
    fn op_new(&mut self, ra: usize, ty: u32) -> Result<(), RefError> {
        let h = self.heap.alloc_obj(ty)??;
        let old = std::mem::replace(&mut self.regs[ra], h);
        Ok(self.heap.release_obj(old)?)
    }

    /// `IsClass A Bx`: `rA :=` whether the instance `rA` is of the class `Bx` or a descendant
    /// (`nil` is 0; `docs/spec/objects.md` 14.3). `rA` is an uncounted copy of the handle.
    fn op_is_class(&mut self, ra: usize, ty: u32) -> Result<(), RefError> {
        let h = self.regs[ra];
        let yes = h != 0 && self.heap.class_inherits(self.heap.obj_class(h)?, ty);
        self.regs[ra] = Slot::from(yes);
        Ok(())
    }

    /// `AsClass A Bx`: `EInvalidCast` unless the instance `rA` is `nil` or of the class `Bx` or a
    /// descendant (`docs/spec/objects.md` 2.9, 14.3). `rA` is left as it is.
    fn op_as_class(&mut self, ra: usize, ty: u32) -> Result<(), RefError> {
        let h = self.regs[ra];
        if h == 0 {
            return Ok(());
        }
        let class = self.heap.obj_class(h)?;
        if self.heap.class_inherits(class, ty) {
            return Ok(());
        }
        Err(RefError::Rt(self.invalid_cast(class, ty)))
    }

    /// The `EInvalidCast` of a class `class` that does not implement the interface `intf`
    /// (`docs/spec/objects.md` 7.6: the declared names of the class and the interface).
    pub(super) fn invalid_intf_cast(&self, class: u32, intf: u32) -> RtError {
        let name = |t: u32| class_decl_name(self.heap.type_name(t)).to_string();
        rt(
            exc::INVALID_CAST,
            format!(
                "invalid interface cast: `{}` does not implement `{}`",
                name(class),
                name(intf)
            ),
        )
    }

    /// The `EInvalidCast` of a class `class` that is not `target` or a descendant
    /// (`docs/spec/objects.md` 2.8, 2.9: the declared names of the two classes).
    pub(super) fn invalid_cast(&self, class: u32, target: u32) -> RtError {
        let name = |t: u32| class_decl_name(self.heap.type_name(t)).to_string();
        rt(
            exc::INVALID_CAST,
            format!(
                "invalid class typecast: `{}` is not `{}`",
                name(class),
                name(target)
            ),
        )
    }

    /// `ObjGet` / `ObjGetRef A B C`: `rA :=` slot `C` of the instance `rB`, checked against its
    /// generation (`docs/spec/objects.md` 14.4).
    fn op_obj_get(
        &mut self,
        ra: usize,
        rb: usize,
        i: usize,
        managed: bool,
    ) -> Result<(), RefError> {
        // The handle is checked once; the lending checks take the block's reference (index + 1)
        let index = self.heap.obj_index(self.regs[rb])?;
        let v = self.heap.obj_slot_at(index, i)?;
        self.heap.check_slot(index as heap::StrRef + 1, i as i64)?;
        if managed {
            self.share(ra, v)?;
        } else {
            self.regs[ra] = v;
        }
        Ok(())
    }

    /// `ObjSet` / `ObjSetRef A B C`: slot `B` of the instance `rA` := `rC`, written in place (the
    /// instance is shared, never made unique); a managed slot counts the new value, then lets go
    /// of the old one.
    fn op_obj_set(
        &mut self,
        ra: usize,
        i: usize,
        rc: usize,
        managed: bool,
    ) -> Result<(), RefError> {
        let v = self.regs[rc];
        let index = self.heap.obj_index(self.regs[ra])?;
        self.heap.check_slot(index as heap::StrRef + 1, i as i64)?;
        if managed {
            self.heap.check_share(v)?;
            self.heap.add_ref(v)?;
            match self.heap.obj_replace_at(index, i, v) {
                Ok(old) => self.heap.release(old)?,
                Err(e) => {
                    self.heap.release(v)?;
                    return Err(e.into());
                }
            }
        } else {
            self.heap.obj_replace_at(index, i, v)?;
        }
        Ok(())
    }

    /// `LoadRefRef A B`: `rA := (the location rB points to)` for a managed value.
    fn op_load_ref_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.ref_read(self.regs[rb])?;
        self.share(ra, v)
    }

    /// `StoreRef A B`: `(the location rA points to) := rB` for an unmanaged value.
    fn op_store_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let loc = self.ref_target(self.regs[ra])?;
        self.loc_replace(loc, self.regs[rb])?;
        Ok(())
    }

    /// `StoreRefRef A B` (`v` is `rB`) and `ClearRef A` (`v` is 0): the location `rA` points to
    /// takes a reference to the managed value `v` and lets go of its old value.
    fn op_store_ref_ref(&mut self, ra: usize, v: Slot) -> Result<(), RefError> {
        let loc = self.ref_target(self.regs[ra])?;
        self.heap.check_share(v)?;
        self.heap.add_ref(v)?;
        let old = self.loc_replace(loc, v)?;
        Ok(self.heap.release(old)?)
    }

    /// `Unlend`: gives back the values lent to the current frame's call.
    fn op_unlend(&mut self) -> Result<(), RefError> {
        match self.give_back(true)? {
            Some(e) => Err(RefError::Rt(e)),
            None => Ok(()),
        }
    }

    /// `AssignRef A B`: `rA := rB` for a managed value.
    fn op_assign_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.regs[rb];
        if self.heap.assign_ref_fast(v, self.regs[ra]) {
            self.regs[ra] = v;
            return Ok(());
        }
        self.share(ra, v)
    }

    /// `AssignObj A B` (`docs/spec/objects.md` 14.3): `AssignRef` of a value counted by its
    /// contents.
    fn op_assign_obj(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.regs[rb];
        self.heap.add_ref_obj(v)?;
        let old = std::mem::replace(&mut self.regs[ra], v);
        Ok(self.heap.release_obj(old)?)
    }

    /// `LoadWeak A B` (`docs/spec/objects.md` 4.4, 14.3): the instance the weak handle `rB`
    /// refers to, counted by its contents into `rA` like `AssignObj`, or `nil` once the instance
    /// is freed or its destruction has started.
    fn op_load_weak(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.heap.weak_target(self.regs[rb])?;
        self.heap.add_ref_obj(v)?;
        let old = std::mem::replace(&mut self.regs[ra], v);
        Ok(self.heap.release_obj(old)?)
    }

    /// `AddRef A`. When it fails, `rA` is cleared: the register did not get its reference, and
    /// the managed registers would let go of it (a value parameter counting its argument when its
    /// routine is entered; `docs/design/lifetime.md` 3.3). `AddRefObj`, which a `TObject` value
    /// parameter uses there, fails only with `Fatal` (a bad handle), so it needs no such clearing;
    /// one that could raise would have to clear `rA` the same way.
    fn op_add_ref(&mut self, ra: usize) -> Result<(), RefError> {
        let v = self.regs[ra];
        let counted = self
            .heap
            .check_share(v)
            .map_err(RefError::from)
            .and_then(|()| self.heap.add_ref(v).map_err(RefError::from));
        if counted.is_err() {
            self.regs[ra] = 0;
        }
        counted
    }

    /// `MoveRef A B`: the reference of `rB` moves to `rA`; `rB` becomes 0.
    fn op_move_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = std::mem::take(&mut self.regs[rb]);
        Ok(self.put(ra, v)?)
    }

    /// `Release A`: lets go of `rA` and clears it.
    fn op_release(&mut self, ra: usize) -> Result<(), RefError> {
        let old = std::mem::take(&mut self.regs[ra]);
        Ok(self.heap.release(old)?)
    }

    /// `LoadGlobalRef A Bx`.
    fn op_load_global_ref(&mut self, ra: usize, g: usize) -> Result<(), RefError> {
        let v = self.globals[g];
        self.share(ra, v)
    }

    /// `StoreGlobalRef A Bx`.
    fn op_store_global_ref(&mut self, ra: usize, g: usize) -> Result<(), RefError> {
        let v = self.regs[ra];
        self.heap.check_share(v)?;
        self.heap.add_ref(v)?;
        let old = std::mem::replace(&mut self.globals[g], v);
        Ok(self.heap.release(old)?)
    }

    /// `LoadConst A Bx` of a constant already in the cache: a managed constant gets a reference
    /// of its own (the cache keeps its one). `false` when the constant is not made yet.
    pub(super) fn op_load_cached_const(&mut self, ra: usize, k: usize) -> Result<bool, RefError> {
        let entry = self.const_cache[k];
        let Some(v) = entry.get() else {
            return Ok(false);
        };
        if entry.managed() {
            // Counted like `AssignRef` without a `Result` when that frees nothing
            if self.heap.assign_ref_fast(v, self.regs[ra]) {
                self.regs[ra] = v;
            } else {
                self.put_shared(ra, v)?;
            }
        } else {
            self.regs[ra] = v;
        }
        Ok(true)
    }

    /// `StrLen A B`.
    fn op_str_len(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        self.regs[ra] = self.heap.get(self.regs[rb])?.len() as i64;
        Ok(())
    }

    /// `ConcatStr A B C`: an empty operand shares the other one; otherwise a new string. When
    /// `rA` is `rB` and holds the string's only reference (an intermediate value of `X + Y + Z`),
    /// `rC` is appended to it in place with no capacity to spare, which gives the same result
    /// without allocating another block (`docs/spec/bytecode.md` 2.4).
    fn op_concat_str(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let (sb, sc) = (self.regs[rb], self.regs[rc]);
        if ra == rb && sb != sc && sb != 0 && self.heap.ref_count(sb)? == 1 {
            let y = self.heap.get(sc)?.len();
            if y > 0 {
                if self.heap.get(sb)?.len() + y > MAX_STRING_LEN {
                    return Err(RefError::Rt(oom()));
                }
                return self
                    .heap
                    .concat_in_place(sb, sc)?
                    .map_err(|_| RefError::Rt(oom()));
            }
        }
        let joined = self.heap.join_strs(&[sb, sc])?;
        self.put_joined(ra, joined)
    }

    /// Writes the result of [`heap::Heap::join_strs`] to `rA` (`ConcatStr`, `ConcatStrN`).
    fn put_joined(
        &mut self,
        ra: usize,
        joined: Result<heap::Joined, OutOfMemory>,
    ) -> Result<(), RefError> {
        match joined {
            Ok(heap::Joined::Shared(s)) => Ok(self.put_shared(ra, s)?),
            Ok(heap::Joined::New(s)) => {
                let v = self.alloc_string(s)?;
                Ok(self.put(ra, v)?)
            }
            Err(OutOfMemory) => Err(RefError::Rt(oom())),
        }
    }

    /// `ConcatStrN A B C`: `rA := rB + … + r(B+C-1)`, made at once (`docs/spec/bytecode.md` 2.4).
    fn op_concat_str_n(&mut self, ra: usize, rb: usize, n: usize) -> Result<(), RefError> {
        let joined = self.heap.join_strs(&self.regs[rb..rb + n])?;
        self.put_joined(ra, joined)
    }

    /// `StrFromChar A B`: the one-character string of the code unit `rB` (an ASCII one is shared
    /// from the table of short strings; `docs/design/vm.md` 3.11).
    pub(super) fn op_str_from_char(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let u = self.regs[rb] as u16;
        let x = if usize::from(u) < SMALL_CHARS {
            self.small_str(usize::from(u))?
        } else {
            let s = PStr::from_units(&[u]).map_err(|_| RefError::Rt(oom()))?;
            self.alloc_string(s)?
        };
        Ok(self.put(ra, x)?)
    }

    /// `IntToStr A B` / `UIntToStr A B` / `BoolToStr A B`: the text of `rB` as a new string (the
    /// integers from 0 below `SMALL_INTS` are shared from the table of short strings;
    /// `docs/design/vm.md` 3.11).
    fn op_int_to_str(&mut self, opc: u8, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.regs[rb];
        if opc == op::INT_TO_STR && (0..SMALL_INTS).contains(&v) {
            let x = self.small_str(SMALL_CHARS + v as usize)?;
            return Ok(self.put(ra, x)?);
        }
        let mut buf = [0u8; 21];
        let text: &[u8] = match opc {
            op::INT_TO_STR => strconv::decimal(v < 0, v.unsigned_abs(), &mut buf),
            op::UINT_TO_STR => strconv::decimal(false, v as u64, &mut buf),
            _ if v != 0 => b"TRUE",
            _ => b"FALSE",
        };
        let s = PStr::ascii(text).map_err(|_| RefError::Rt(oom()))?;
        let x = self.alloc_string(s)?;
        Ok(self.put(ra, x)?)
    }

    /// `StrCopy A B`: `rA := Copy(rB, r(B+1), r(B+2))`. The whole string is shared; any other
    /// part is a new string.
    fn op_str_copy(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let whole = self.regs[rb];
        let s = self.heap.get(whole)?;
        match copy_range(self.regs[rb + 1], self.regs[rb + 2], s.len()) {
            None => Ok(self.put(ra, 0)?),
            Some((start, end)) if start == 0 && end == s.len() => Ok(self.put_shared(ra, whole)?),
            Some((start, end)) => {
                let part = s.slice(start, end).map_err(|_| RefError::Rt(oom()))?;
                let x = self.alloc_string(part)?;
                Ok(self.put(ra, x)?)
            }
        }
    }

    /// `StrPos A B`: `rA := Pos(rB, r(B+1))`, searched without copying either string.
    fn op_str_pos(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let sub = self.heap.get(self.regs[rb])?;
        let s = self.heap.get(self.regs[rb + 1])?;
        self.regs[ra] = s.find(sub).map_or(0, |p| p as i64 + 1);
        Ok(())
    }

    /// `StrUpperAscii A B` / `StrLowerAscii A B`: a new string with the ASCII letters changed.
    fn op_str_case(&mut self, ra: usize, rb: usize, upper: bool) -> Result<(), RefError> {
        let s = self.heap.get(self.regs[rb])?;
        let mapped = s.map_ascii_case(upper).map_err(|_| RefError::Rt(oom()))?;
        let x = self.alloc_string(mapped)?;
        Ok(self.put(ra, x)?)
    }

    /// `AppendStr A B C`: `rA := rB + rC`, where `rA` holds the value of the place being assigned
    /// read without counting (the place's own reference) and `rB` the counted copy of it taken
    /// before the operands were evaluated (`docs/spec/bytecode.md` 2.4). When `rA` and `rB` are
    /// still the same string and nothing else refers to it, `rC` is appended in place; otherwise
    /// this is `ConcatStr` (the place's old value is let go).
    fn op_append_str(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let (sa, sb, sc) = (self.regs[ra], self.regs[rb], self.regs[rc]);
        if sa == sb && sa != 0 && self.heap.ref_count(sa)? == 2 {
            let n = self.heap.get(sa)?.len() + self.heap.get(sc)?.len();
            if n > MAX_STRING_LEN {
                return Err(RefError::Rt(oom()));
            }
            return self
                .heap
                .append_str(sa, sc)?
                .map_err(|_| RefError::Rt(oom()));
        }
        self.op_concat_str(ra, rb, rc)
    }

    /// `rA := rB + rC` of two `UTF8String`s (`ConcatU8`, whose arm is in `step`; `AppendU8` falls
    /// back to this). Always inlined: called out of line from `step`, the hot loop's layout moved
    /// and `floats` / `records` ran about 1.1 times slower (`docs/notes/benchmark-m2.md`).
    #[inline(always)]
    pub(super) fn op_concat_u8(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let (sb, sc) = (self.regs[rb], self.regs[rc]);
        let x = self.heap.get_u8(sb)?;
        let y = self.heap.get_u8(sc)?;
        if x.len() + y.len() > MAX_STRING_LEN {
            return Err(RefError::Rt(oom()));
        }
        if x.is_empty() || y.is_empty() {
            let s = if x.is_empty() { sc } else { sb };
            return Ok(self.put_shared(ra, s)?);
        }
        let n = try_join(&[x, y])?;
        let v = self.alloc_u8(n)?;
        Ok(self.put(ra, v)?)
    }

    /// `AppendU8 A B C`: `AppendStr` of `UTF8String`s.
    fn op_append_u8(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let (sa, sb, sc) = (self.regs[ra], self.regs[rb], self.regs[rc]);
        if sa == sb && sa != 0 && self.heap.ref_count(sa)? == 2 {
            let n = self.heap.get_u8(sa)?.len() + self.heap.get_u8(sc)?.len();
            if n > MAX_STRING_LEN {
                return Err(RefError::Rt(oom()));
            }
            return self
                .heap
                .append_u8(sa, sc)?
                .map_err(|_| RefError::Rt(oom()));
        }
        self.op_concat_u8(ra, rb, rc)
    }

    /// `StrIndex A B C`: the code unit `rB[rC]` (always range-checked).
    fn op_str_index(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let s = self.heap.get(self.regs[rb])?;
        let i = self.regs[rc];
        if i < 1 || i > s.len() as i64 {
            return Err(str_index_error(i, s.len()));
        }
        self.regs[ra] = s.unit(i as usize - 1) as i64;
        Ok(())
    }

    /// `StrSetIndex A B C`: `rA[rB] := rC`, in place when `rA` is the only reference.
    fn op_str_set_index(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let len = self.heap.get(self.regs[ra])?.len();
        let i = self.regs[rb];
        if i < 1 || i > len as i64 {
            return Err(str_index_error(i, len));
        }
        let (index, unit) = (i as usize - 1, self.regs[rc] as u16);
        // The only reference writes in place; a shared string (the constant cache holds one of
        // every literal) is copied (`docs/spec/bytecode.md` 2.4)
        let in_place = match self.heap.unique_str_mut(self.regs[ra])? {
            Some(s) => s.set_unit_in_place(index, unit),
            None => false,
        };
        if !in_place {
            let n = self.heap.get(self.regs[ra])?.with_unit(index, unit)?;
            let v = self.alloc_string(n)?;
            self.put(ra, v)?;
        }
        Ok(())
    }

    /// The order of two strings (`CmpStr`): negative, 0 or positive.
    fn str_compare(&self, x: Slot, y: Slot) -> Result<Slot, RefError> {
        Ok(self.heap.get(x)?.compare(self.heap.get(y)?) as Slot)
    }

    /// Whether two strings are equal (`EqStr`, `JmpIfEqStr`, `JmpIfNeStr`).
    pub(super) fn str_equal(&self, x: Slot, y: Slot) -> Result<bool, RefError> {
        Ok(self.heap.get(x)?.equals(self.heap.get(y)?))
    }

    /// `HashStr A B`: the hash of the string's code units, whatever its representation
    /// (`docs/spec/equality.md` 2.3).
    fn op_hash_str(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let h = self
            .hash_keys
            .units(self.heap.get(self.regs[rb])?.iter_units());
        self.regs[ra] = h;
        Ok(())
    }

    /// `ArrLen A B`.
    fn op_arr_len(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        self.regs[ra] = self.heap.len(self.regs[rb])? as i64;
        Ok(())
    }

    /// `DynGet A B C` / `DynGetRef A B C`: `rA := rB[rC]` (always range-checked).
    fn op_dyn_get(
        &mut self,
        ra: usize,
        rb: usize,
        rc: usize,
        managed: bool,
    ) -> Result<(), RefError> {
        let (container, i) = (self.regs[rb], self.regs[rc]);
        if let Some(v) = self.heap.dyn_get_fast(container, i)
            && self.get_fast(ra, v, managed)
        {
            return Ok(());
        }
        let v = self.dyn_slot(container, i)?;
        self.heap.check_slot(container, i)?;
        if managed {
            self.share(ra, v)?;
        } else {
            self.regs[ra] = v;
        }
        Ok(())
    }

    /// `DynSet A B C` / `DynSetRef A B C`: `rA[rB] := rC` (always range-checked).
    fn op_dyn_set(
        &mut self,
        ra: usize,
        rb: usize,
        rc: usize,
        managed: bool,
    ) -> Result<(), RefError> {
        let (container, i) = (self.regs[ra], self.regs[rb]);
        if self.set_fast(container, i, self.regs[rc], managed) {
            return Ok(());
        }
        self.dyn_slot(container, i)?;
        self.heap.check_slot(container, i)?;
        self.slot_store(container, i, self.regs[rc], managed)
    }

    /// `FieldGet` / `ValGet` and their `Ref` forms: `rA := slot i of the block rB`. A default
    /// record or array (0) has default slots.
    fn op_slot_get(&mut self, ra: usize, rb: usize, i: i64, managed: bool) -> Result<(), RefError> {
        let container = self.regs[rb];
        if let Some(v) = self.heap.dyn_get_fast(container, i)
            && self.get_fast(ra, v, managed)
        {
            return Ok(());
        }
        let v = if container == 0 {
            0
        } else {
            self.heap
                .slot(container, i)?
                .ok_or_else(|| RefError::Fatal(format!("slot {i} out of range in a read")))?
        };
        self.heap.check_slot(container, i)?;
        if managed {
            self.share(ra, v)?;
        } else {
            self.regs[ra] = v;
        }
        Ok(())
    }

    /// `FieldSet` / `ValSet` and their `Ref` forms: slot `i` of the block `rA` := `rC`.
    fn op_slot_set(&mut self, ra: usize, i: i64, rc: usize, managed: bool) -> Result<(), RefError> {
        if self.set_fast(self.regs[ra], i, self.regs[rc], managed) {
            return Ok(());
        }
        self.heap.check_slot(self.regs[ra], i)?;
        self.slot_store(self.regs[ra], i, self.regs[rc], managed)
    }

    /// The fast path of a slot read (`docs/design/vm.md` 3.14 (a′)): puts the slot's value `v`
    /// into `ra`, counting it when `managed`. `false`, with nothing changed, when counting needs
    /// the body ([`Heap::assign_ref_fast`]).
    #[inline(always)]
    fn get_fast(&mut self, ra: usize, v: Slot, managed: bool) -> bool {
        if managed && !self.heap.assign_ref_fast(v, self.regs[ra]) {
            return false;
        }
        self.regs[ra] = v;
        true
    }

    /// The fast path of a slot write: `container[i] := v` when the heap's fast paths allow it
    /// ([`Heap::dyn_set_fast`], [`Heap::slot_set_ref_fast`]). `false`, with nothing changed,
    /// otherwise.
    #[inline(always)]
    fn set_fast(&mut self, container: Slot, i: i64, v: Slot, managed: bool) -> bool {
        if managed {
            self.heap.slot_set_ref_fast(container, i, v).is_some()
        } else {
            self.heap.dyn_set_fast(container, i, v).is_some()
        }
    }

    /// Writes `v` into slot `i` of `container`; a managed slot counts the new value and lets go
    /// of the old one.
    fn slot_store(
        &mut self,
        container: Slot,
        i: i64,
        v: Slot,
        managed: bool,
    ) -> Result<(), RefError> {
        if managed {
            self.heap.check_share(v)?;
            self.heap.add_ref(v)?;
            let old = std::mem::replace(self.heap.slot_mut(container, i)?, v);
            self.heap.release(old)?;
        } else {
            *self.heap.slot_mut(container, i)? = v;
        }
        Ok(())
    }

    /// `Unique A Bx`: makes the block `rA` (of type `Bx`) the only reference, copying it if shared.
    fn op_unique(&mut self, ra: usize, ty: u32) -> Result<(), RefError> {
        if self.heap.unique_fast(self.regs[ra]) {
            return Ok(());
        }
        self.check_copy_shared(self.regs[ra])?;
        self.regs[ra] = self.heap.unique(self.regs[ra], ty)??;
        Ok(())
    }

    /// Refuses making a shared block unique (copying it) while it contains a lent location
    /// (`Unique`, `UniqueVal`).
    pub(super) fn check_copy_shared(&self, v: Slot) -> Result<(), RefError> {
        if self.heap.lending() && self.heap.ref_count(v)? > 1 {
            self.heap.check_copy_block(v)?;
        }
        Ok(())
    }

    /// `Ret A` / `RetVoid`: leaves the top frame. The result (`value`, a register of the frame) is
    /// set aside so the implicit `finally` does not release it, and written to the caller after
    /// the frame's managed registers are released (the destination is the callee's base, the
    /// first argument). Returns whether that was the last frame; then the result is dropped.
    /// When the releases opened a job (a destructor to run), it is stamped to raise its exception
    /// at the caller's `Call` (`docs/spec/objects.md` 8.5); the caller runs it next.
    pub(super) fn op_ret(&mut self, value: Option<usize>) -> Result<bool, RefError> {
        let frame = *self.frames.last().expect("`Ret` runs in a frame");
        let v = value.map_or(0, |r| std::mem::take(&mut self.regs[r]));
        self.release_frame(&frame)?;
        let fr = self.frames.pop().expect("the frame was just read");
        let Some(caller) = self.frames.last() else {
            if self.heap.job_open() {
                self.heap.stamp_job(heap::JobOrigin::RetLast, 0);
            }
            return Ok(true);
        };
        if value.is_some() {
            self.regs[fr.ret() as usize] = v;
        }
        if self.heap.job_open() {
            // The kind of the result: that of its register in the callee's list
            let kind = value.map_or(0, |r| {
                let reg = (r - frame.base as usize) as u16;
                self.module.funcs[frame.func as usize]
                    .managed
                    .iter()
                    .find(|m| m.reg == reg)
                    .map_or(0, |m| m.kind)
            });
            let origin = heap::JobOrigin::Ret {
                pc: caller.pc.saturating_sub(1),
                result: fr.ret(),
                kind,
            };
            self.heap.stamp_job(origin, self.frames.len() as u32);
        }
        Ok(false)
    }
}
