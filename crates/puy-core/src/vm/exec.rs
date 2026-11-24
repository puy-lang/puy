//! Bodies of the instructions that both the fast path (`Isolate::run_loop`) and `Isolate::step`
//! run (`docs/design/vm.md` 3.14).
//!
//! Each body reads its operands as register indices and immediates (already decoded, with the
//! frame's base added), does the whole work of the instruction, and reports a failure as a
//! `RefError` instead of ending the step itself: `RefError::Rt` is a Pascal exception (raised at the
//! instruction, whose `pc` the caller writes back), `RefError::Fatal` an internal error. A body
//! never changes the frames or `pc`, so the fast path keeps going in the same frame after it.

use super::*;

/// How a hash table lookup of the instance compares the keys (`HashLookupInt` /
/// `HashLookupStr`; `docs/spec/bytecode.md` 2.23): the 64 bits of the slots (as `EqInt`), or as
/// strings (as `EqStr`). It also picks the hash of the key (`HashInt` / `HashStr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyCmp {
    Int,
    Str,
}

/// `DivInt` / `DivIntChk` / `ModInt` / `DivUInt` / `ModUInt` on `n` and `d`
/// (`docs/spec/bytecode.md` 2.2): a zero divisor is always `EDivByZero`.
pub(super) fn int_div(opc: u8, n: Slot, d: Slot) -> Result<Slot, RtError> {
    if d == 0 {
        return Err(rt(exc::DIV_BY_ZERO, "division by zero"));
    }
    Ok(match opc {
        op::word::DIV_INT_CHK if n == i64::MIN && d == -1 => return Err(int_overflow("Int64")),
        op::word::DIV_INT | op::word::DIV_INT_CHK => n.wrapping_div(d),
        op::word::MOD_INT => n.wrapping_rem(d),
        op::word::DIV_UINT => ((n as u64) / (d as u64)) as Slot,
        op::word::MOD_UINT => ((n as u64) % (d as u64)) as Slot,
        _ => unreachable!("not a division"),
    })
}

/// The integer operations with an immediate (`docs/spec/bytecode.md` 2.2) other than `AddIntI`,
/// `DivIntI` and `ModIntI`: `x` is `rB`, `imm` is `sC`. The same results as the register forms
/// with `imm` in `rC`.
#[inline(always)]
pub(super) fn int_imm(opc: u8, x: Slot, imm: i64) -> Slot {
    match opc {
        op::word::MUL_INT_I => x.wrapping_mul(imm),
        op::word::AND_INT_I => x & imm,
        op::word::OR_INT_I => x | imm,
        op::word::XOR_INT_I => x ^ imm,
        op::word::SHL_INT_I => x.wrapping_shl((imm & 63) as u32),
        op::word::SHR_INT_I => ((x as u64).wrapping_shr((imm & 63) as u32)) as i64,
        op::word::SHL_INT32_I => ((x as i32).wrapping_shl((imm & 31) as u32)) as i64,
        op::word::SHR_INT32_I => (((x as u32).wrapping_shr((imm & 31) as u32)) as i32) as i64,
        op::word::ADD_INT32_I => int32(op::word::ADD_INT32, x, imm),
        op::word::MUL_INT32_I => int32(op::word::MUL_INT32, x, imm),
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
        op::word::ADD_INT32 => x.wrapping_add(y),
        op::word::SUB_INT32 => x.wrapping_sub(y),
        op::word::MUL_INT32 => x.wrapping_mul(y),
        _ => unreachable!("not a 32-bit integer operation"),
    })
}

/// Whether a compare-and-branch with an immediate (`docs/spec/bytecode.md` 2.3) jumps: `x` is
/// `rA`, `imm` is `sB` (signed).
#[inline(always)]
pub(super) fn cmp_imm(opc: u8, x: Slot, imm: i64) -> bool {
    match opc {
        op::word::JMP_IF_EQ_INT_I => x == imm,
        op::word::JMP_IF_NE_INT_I => x != imm,
        op::word::JMP_IF_LT_INT_I => x < imm,
        op::word::JMP_IF_LE_INT_I => x <= imm,
        op::word::JMP_IF_GT_INT_I => x > imm,
        op::word::JMP_IF_GE_INT_I => x >= imm,
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
    Some(if opc == op::word::FOR_LOOP_UP {
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
            op::word::LOAD_CONST => match self.op_load_cached_const(ra, bx) {
                Ok(true) => Ok(()),
                Ok(false) => return None,
                Err(e) => Err(e),
            },
            op::word::ASSIGN_REF => self.op_assign_ref(ra, rb),
            op::word::MOVE_REF => self.op_move_ref(ra, rb),
            op::word::ADD_REF => self.op_add_ref(ra),
            op::word::RELEASE => self.op_release(ra),
            op::word::ASSIGN_OBJ => self.op_assign_obj(ra, rb),
            op::word::LOAD_WEAK => self.op_load_weak(ra, rb),
            op::word::ADD_REF_OBJ => self
                .heap
                .add_ref_obj(self.task.regs[ra])
                .map_err(RefError::from),
            op::word::RELEASE_OBJ => {
                let old = std::mem::take(&mut self.task.regs[ra]);
                self.heap.release_obj(old).map_err(RefError::from)
            }
            op::word::LOAD_GLOBAL_REF => self.op_load_global_ref(ra, bx),
            op::word::STORE_GLOBAL_REF => self.op_store_global_ref(ra, bx),
            op::word::STR_LEN => self.op_str_len(ra, rb),
            op::word::CONCAT_STR => self.op_concat_str(ra, rb, rc),
            op::word::CONCAT_STR_N => self.op_concat_str_n(ra, rb, c),
            op::word::INT_TO_STR | op::word::UINT_TO_STR | op::word::BOOL_TO_STR => {
                self.op_int_to_str(opc, ra, rb)
            }
            op::word::STR_COPY => self.op_str_copy(ra, rb),
            op::word::STR_POS => self.op_str_pos(ra, rb),
            op::word::STR_UPPER_ASCII | op::word::STR_LOWER_ASCII => {
                self.op_str_case(ra, rb, opc == op::word::STR_UPPER_ASCII)
            }
            op::word::APPEND_STR => self.op_append_str(ra, rb, rc),
            op::word::APPEND_U8 => self.op_append_u8(ra, rb, rc),
            op::word::STR_INDEX => self.op_str_index(ra, rb, rc),
            op::word::STR_SET_INDEX => self.op_str_set_index(ra, rb, rc),
            op::word::CMP_STR => self
                .str_compare(self.task.regs[rb], self.task.regs[rc])
                .map(|o| self.task.regs[ra] = o),
            op::word::EQ_STR => self
                .str_equal(self.task.regs[rb], self.task.regs[rc])
                .map(|e| self.task.regs[ra] = e as Slot),
            // `docs/spec/equality.md` 8.1
            op::word::HASH_INT => {
                self.task.regs[ra] = self.hash_keys.int(self.task.regs[rb] as u64);
                Ok(())
            }
            op::word::HASH_F64 => {
                self.task.regs[ra] = self.hash_keys.f64(slot_f64(self.task.regs[rb]));
                Ok(())
            }
            op::word::HASH_STR => self.op_hash_str(ra, rb),
            // `docs/spec/bytecode.md` 2.23
            op::word::HASH_FIND => self.op_hash_find(ra, rb),
            op::word::HASH_LOOKUP_INT => self.op_hash_lookup(ra, rb, c, KeyCmp::Int),
            op::word::HASH_LOOKUP_STR => self.op_hash_lookup(ra, rb, c, KeyCmp::Str),
            op::word::HASH_MIX => {
                self.task.regs[ra] = self
                    .hash_keys
                    .mix(self.task.regs[rb] as u64, self.task.regs[rc] as u64);
                Ok(())
            }
            op::word::KEY_EQ_F64 => {
                let eq =
                    hash::key_eq_f64(slot_f64(self.task.regs[rb]), slot_f64(self.task.regs[rc]));
                self.task.regs[ra] = eq as Slot;
                Ok(())
            }
            op::word::ARR_LEN => self.op_arr_len(ra, rb),
            // `BytesGet` / `BytesSet` of an array that is not of bytes (an open array parameter
            // given a static array of `Byte`), and every case the fast path leaves, read and write
            // as `DynGet` / `DynSet` (one call site each, so that they stay inlined here)
            op::word::DYN_GET | op::word::DYN_GET_REF | op::word::BYTES_GET => {
                self.op_dyn_get(ra, rb, rc, opc == op::word::DYN_GET_REF)
            }
            op::word::DYN_SET | op::word::DYN_SET_REF | op::word::BYTES_SET => {
                self.op_dyn_set(ra, rb, rc, opc == op::word::DYN_SET_REF)
            }
            op::word::FIELD_GET | op::word::FIELD_GET_REF => {
                self.op_slot_get(ra, rb, c as i64, opc == op::word::FIELD_GET_REF)
            }
            op::word::VAL_GET | op::word::VAL_GET_REF => {
                self.op_slot_get(ra, rb, self.task.regs[rc], opc == op::word::VAL_GET_REF)
            }
            op::word::FIELD_SET | op::word::FIELD_SET_REF => {
                self.op_slot_set(ra, b as i64, rc, opc == op::word::FIELD_SET_REF)
            }
            op::word::REF_FIELD_GET | op::word::REF_FIELD_GET_REF => {
                self.op_ref_field_get(ra, rb, c as i64, opc == op::word::REF_FIELD_GET_REF)
            }
            // `bx` is the type word (`docs/spec/bytecode.md` 1.2)
            op::word::REF_FIELD_SET | op::word::REF_FIELD_SET_REF => self.op_ref_field_set(
                ra,
                b as i64,
                rc,
                bx as u32,
                opc == op::word::REF_FIELD_SET_REF,
            ),
            op::word::VAL_SET | op::word::VAL_SET_REF => {
                self.op_slot_set(ra, self.task.regs[rb], rc, opc == op::word::VAL_SET_REF)
            }
            // Flat places (`docs/spec/bytecode.md` 2.8). `AggLoad`, `AggStore` and `LendPlace`,
            // whose type word `run_loop` does not read, are in step
            op::word::ELEM_AT => self.op_elem_at(ra, rb, rc),
            op::word::SLOT_GET | op::word::SLOT_GET_REF => {
                self.op_cursor_get(ra, rb, c, opc == op::word::SLOT_GET_REF)
            }
            op::word::SLOT_SET | op::word::SLOT_SET_REF => {
                self.op_cursor_set(ra, b, rc, opc == op::word::SLOT_SET_REF)
            }
            op::word::UNIQUE => self.op_unique(ra, bx as u32),
            op::word::LOAD_REF => self
                .ref_read(self.task.regs[rb])
                .map(|v| self.task.regs[ra] = v),
            op::word::LOAD_REF_REF => self.op_load_ref_ref(ra, rb),
            op::word::STORE_REF => self.op_store_ref(ra, rb),
            op::word::STORE_REF_REF | op::word::CLEAR_REF => {
                let v = if opc == op::word::CLEAR_REF {
                    0
                } else {
                    self.task.regs[rb]
                };
                self.op_store_ref_ref(ra, v)
            }
            op::word::LEND => self.op_lend(ra, rb, c),
            op::word::UNLEND => self.op_unlend(),
            op::word::NEW => self.op_new(ra, bx as u32),
            op::word::IS_CLASS => self.op_is_class(ra, bx as u32),
            op::word::AS_CLASS => self.op_as_class(ra, bx as u32),
            op::word::OBJ_GET | op::word::OBJ_GET_REF => {
                self.op_obj_get(ra, rb, c, opc == op::word::OBJ_GET_REF)
            }
            op::word::OBJ_SET | op::word::OBJ_SET_REF => {
                self.op_obj_set(ra, b, rc, opc == op::word::OBJ_SET_REF)
            }
            // The end of a `finally` body reached normally with no `Jmp` after it (the creation of
            // an instance; `docs/spec/objects.md` 14.3): nothing to do. The other states change
            // `pc`, which step does
            op::word::END_FINALLY if b == 0 && self.task.regs[ra] == FINALLY_NORMAL => Ok(()),
            op::word::CAP_GET => self.op_cap_get(ra, rb, c),
            op::word::CELL_GET => self
                .heap
                .cell_get(self.task.regs[rb])
                .map(|v| self.task.regs[ra] = v)
                .map_err(RefError::from),
            op::word::CELL_GET_REF => match self.heap.cell_get(self.task.regs[rb]) {
                Ok(v) => self.share(ra, v),
                Err(e) => Err(e.into()),
            },
            op::word::CELL_SET => {
                let v = self.task.regs[rb];
                self.heap
                    .cell_mut(self.task.regs[ra])
                    .map(|slot| *slot = v)
                    .map_err(RefError::from)
            }
            op::word::CELL_SET_REF => self.op_cell_set_ref(ra, rb),
            op::word::INTRINSIC => match u8::try_from(c) {
                Ok(op::intr::CELL_REF) => self.op_cell_ref(ra, rb),
                Ok(id @ (op::intr::NEW_CELL | op::intr::NEW_CELL_REF | op::intr::NEW_CELL_OBJ)) => {
                    self.op_new_cell(ra, id)
                }
                Ok(op::intr::MAKE_CLOSURE) => self.op_make_closure(ra, rb),
                // The check of `Self` before an inlined method (`docs/design/inlining.md` 7): a
                // `nil` or freed instance is left to step, which raises. Here rather than in its
                // own arm of the run loop, which slowed the loop down by moving its code
                // (`docs/notes/benchmark-m4c/M4-111.md`)
                Ok(op::intr::CLASS_TYPE) => {
                    let ty = self.heap.obj_class_fast(self.task.regs[rb])?;
                    self.task.regs[ra] = Slot::from(ty) + 1;
                    Ok(())
                }
                _ => return None,
            },
            _ => return None,
        };
        Some(r)
    }

    /// `CapGet A B C`: `rA :=` captured cell `C` of the closure `rB`, uncounted
    /// (`docs/spec/objects.md` 14.9).
    fn op_cap_get(&mut self, ra: usize, rb: usize, i: usize) -> Result<(), RefError> {
        let r = self.task.regs[rb];
        let Some(v) = self.heap.closure_cap(r, i) else {
            // Why not: not a live closure, or past the end of its list
            self.heap.closure_caps(r)?;
            return Err(RefError::Fatal(format!("capture {i} out of range")));
        };
        self.task.regs[ra] = v;
        Ok(())
    }

    /// `CellSetRef A B`: the value of the cell `rA` := `rB`, counting `rB` before letting go of
    /// the old value.
    fn op_cell_set_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.task.regs[rb];
        self.heap.check_share(v)?;
        self.heap.add_ref(v)?;
        let old = std::mem::replace(self.heap.cell_mut(self.task.regs[ra])?, v);
        Ok(self.heap.release(old)?)
    }

    /// The `CellRef` built-in (`docs/spec/objects.md` 14.9): `rA :=` a reference slot of kind 3
    /// to the cell `rB`.
    fn op_cell_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let r = self.task.regs[rb];
        if !heap::is_block_ref(r) {
            return Err(RefError::Fatal(format!("bad cell {r}")));
        }
        self.task.regs[ra] = make_ref(3, r as u64);
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
        let frame = *self
            .task
            .frames
            .last()
            .expect("an instruction runs in a frame");
        let reg = (ra - frame.base as usize) as u16;
        let r = self.heap.alloc_cell(kind, frame.func, reg)?;
        Ok(self.put(ra, r)?)
    }

    /// The `MakeClosure` built-in (`docs/spec/objects.md` 14.9): `rA :=` a new closure of the
    /// function `rB` (`-1`: none, a group block) capturing the `r(B+1)` cells from `r(B+2)`,
    /// which it counts.
    fn op_make_closure(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let f = self.task.regs[rb];
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
        let n = self.task.regs[rb + 1];
        let frame = *self
            .task
            .frames
            .last()
            .expect("an instruction runs in a frame");
        let end = frame.base as usize + self.module.funcs[frame.func as usize].regs as usize;
        // Compared without overflow (`usize` is 32 bits on wasm32)
        let Some(n) = usize::try_from(n)
            .ok()
            .filter(|n| *n <= end.saturating_sub(rb + 2))
        else {
            return Err(RefError::Fatal(format!("bad capture count {n}")));
        };
        let list = heap::Caps::new(&self.task.regs[rb + 2..rb + 2 + n])?;
        for &cell in list.iter() {
            self.heap.add_ref(cell)?;
        }
        let r = match self.heap.alloc_closure(func, list) {
            Ok(r) => r,
            Err(OutOfMemory) => {
                for i in rb + 2..rb + 2 + n {
                    let cell = self.task.regs[i];
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
        let old = std::mem::replace(&mut self.task.regs[ra], h);
        Ok(self.heap.release_obj(old)?)
    }

    /// `IsClass A Bx`: `rA :=` whether the instance `rA` is of the class `Bx` or a descendant
    /// (`nil` is 0; `docs/spec/objects.md` 14.3). `rA` is an uncounted copy of the handle.
    fn op_is_class(&mut self, ra: usize, ty: u32) -> Result<(), RefError> {
        let h = self.task.regs[ra];
        let yes = match self.heap.is_class_fast(h, ty) {
            Some(yes) => yes,
            // The handle does not check: `obj_class` reports why
            None => self.heap.class_inherits(self.heap.obj_class(h)?, ty),
        };
        self.task.regs[ra] = Slot::from(yes);
        Ok(())
    }

    /// `AsClass A Bx`: `EInvalidCast` unless the instance `rA` is `nil` or of the class `Bx` or a
    /// descendant (`docs/spec/objects.md` 2.9, 14.3). `rA` is left as it is.
    fn op_as_class(&mut self, ra: usize, ty: u32) -> Result<(), RefError> {
        let h = self.task.regs[ra];
        // `nil` passes (also in the `AsClass` arm of `run_loop`)
        if h == 0 || self.heap.is_class_fast(h, ty) == Some(true) {
            return Ok(());
        }
        // Another class, or a handle that does not check (`obj_class` reports why)
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
        let index = self.heap.obj_index(self.task.regs[rb])?;
        let v = self.obj_slot(index, i)?;
        if managed {
            self.share(ra, v)?;
        } else {
            self.task.regs[ra] = v;
        }
        Ok(())
    }

    /// Slot `i` of the live instance at slab index `index` (from [`heap::Heap::obj_index`], so
    /// the handle is checked once per instruction) for a read: out of its slots is `Fatal`, a
    /// lent slot `EAliasError` (`docs/spec/objects.md` 14.4). The lending checks take the block's
    /// reference (index + 1).
    fn obj_slot(&self, index: usize, i: usize) -> Result<Slot, RefError> {
        let v = self.heap.obj_slot_at(index, i)?;
        self.heap.check_slot(index as heap::StrRef + 1, i as i64)?;
        Ok(v)
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
        let v = self.task.regs[rc];
        let index = self.heap.obj_index(self.task.regs[ra])?;
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

    /// `LoadRefRef A B`: `rA := (the location rB points to)` for a managed value. A flat place
    /// (kind 4) is read into a record or static array value, as `AggLoad` does.
    fn op_load_ref_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let place = self.task.regs[rb];
        if deref(place).0 == lend::PLACE_REF {
            let (j, loan) = self.place_loan(place)?;
            let (r, i) = loan.loc.expect("a lent flat place always has slots");
            let ty = loan.place.expect("a lent flat place has a type");
            return self.agg_load(ra, Some(r), i, ty, Some(j));
        }
        let v = self.ref_read(place)?;
        self.share(ra, v)
    }

    /// `StoreRef A B`: `(the location rA points to) := rB` for an unmanaged value.
    fn op_store_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let loc = self.ref_target(self.task.regs[ra])?;
        self.loc_replace(loc, self.task.regs[rb])?;
        Ok(())
    }

    /// `StoreRefRef A B` (`v` is `rB`) and `ClearRef A` (`v` is 0): the location `rA` points to
    /// takes a reference to the managed value `v` and lets go of its old value. A flat place
    /// (kind 4) takes the slots of the record or static array value `v`, as `AggStore` does.
    fn op_store_ref_ref(&mut self, ra: usize, v: Slot) -> Result<(), RefError> {
        let place = self.task.regs[ra];
        if deref(place).0 == lend::PLACE_REF {
            let (j, loan) = self.place_loan(place)?;
            let (r, i) = loan.loc.expect("a lent flat place always has slots");
            let ty = loan.place.expect("a lent flat place has a type");
            return self.agg_store(r, i, ty, v, Some(j));
        }
        let loc = self.ref_target(place)?;
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
        let v = self.task.regs[rb];
        if self.heap.assign_ref_fast(v, self.task.regs[ra]) {
            self.task.regs[ra] = v;
            return Ok(());
        }
        self.share(ra, v)
    }

    /// `AssignObj A B` (`docs/spec/objects.md` 14.3): `AssignRef` of a value counted by its
    /// contents.
    fn op_assign_obj(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.task.regs[rb];
        self.heap.add_ref_obj(v)?;
        let old = std::mem::replace(&mut self.task.regs[ra], v);
        Ok(self.heap.release_obj(old)?)
    }

    /// `LoadWeak A B` (`docs/spec/objects.md` 4.4, 14.3): the instance the weak handle `rB`
    /// refers to, counted by its contents into `rA` like `AssignObj`, or `nil` once the instance
    /// is freed or its destruction has started.
    fn op_load_weak(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = self.heap.weak_target(self.task.regs[rb])?;
        self.heap.add_ref_obj(v)?;
        let old = std::mem::replace(&mut self.task.regs[ra], v);
        Ok(self.heap.release_obj(old)?)
    }

    /// `AddRef A`. When it fails, `rA` is cleared: the register did not get its reference, and
    /// the managed registers would let go of it (a value parameter counting its argument when its
    /// routine is entered; `docs/design/lifetime.md` 3.3). `AddRefObj`, which a `TObject` value
    /// parameter uses there, fails only with `Fatal` (a bad handle), so it needs no such clearing;
    /// one that could raise would have to clear `rA` the same way.
    fn op_add_ref(&mut self, ra: usize) -> Result<(), RefError> {
        let v = self.task.regs[ra];
        let counted = self
            .heap
            .check_share(v)
            .map_err(RefError::from)
            .and_then(|()| self.heap.add_ref(v).map_err(RefError::from));
        if counted.is_err() {
            self.task.regs[ra] = 0;
        }
        counted
    }

    /// `MoveRef A B`: the reference of `rB` moves to `rA`; `rB` becomes 0.
    fn op_move_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let v = std::mem::take(&mut self.task.regs[rb]);
        Ok(self.put(ra, v)?)
    }

    /// `Release A`: lets go of `rA` and clears it.
    fn op_release(&mut self, ra: usize) -> Result<(), RefError> {
        let old = std::mem::take(&mut self.task.regs[ra]);
        Ok(self.heap.release(old)?)
    }

    /// `LoadGlobalRef A Bx`.
    fn op_load_global_ref(&mut self, ra: usize, g: usize) -> Result<(), RefError> {
        let v = self.globals[g];
        self.share(ra, v)
    }

    /// `StoreGlobalRef A Bx`.
    fn op_store_global_ref(&mut self, ra: usize, g: usize) -> Result<(), RefError> {
        let v = self.task.regs[ra];
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
            if self.heap.assign_ref_fast(v, self.task.regs[ra]) {
                self.task.regs[ra] = v;
            } else {
                self.put_shared(ra, v)?;
            }
        } else {
            self.task.regs[ra] = v;
        }
        Ok(true)
    }

    /// `StrLen A B`.
    fn op_str_len(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        self.task.regs[ra] = self.heap.get(self.task.regs[rb])?.len() as i64;
        Ok(())
    }

    /// `ConcatStr A B C`: an empty operand shares the other one; otherwise a new string. When
    /// `rA` is `rB` and holds the string's only reference (an intermediate value of `X + Y + Z`),
    /// `rC` is appended to it in place with no capacity to spare, which gives the same result
    /// without allocating another block (`docs/spec/bytecode.md` 2.4).
    fn op_concat_str(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let (sb, sc) = (self.task.regs[rb], self.task.regs[rc]);
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
        let joined = self.heap.join_strs(&self.task.regs[rb..rb + n])?;
        self.put_joined(ra, joined)
    }

    /// `StrFromChar A B`: the one-character string of the code unit `rB` (an ASCII one is shared
    /// from the table of short strings; `docs/design/vm.md` 3.11).
    pub(super) fn op_str_from_char(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let u = self.task.regs[rb] as u16;
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
        let v = self.task.regs[rb];
        if opc == op::word::INT_TO_STR && (0..SMALL_INTS).contains(&v) {
            let x = self.small_str(SMALL_CHARS + v as usize)?;
            return Ok(self.put(ra, x)?);
        }
        let mut buf = [0u8; 21];
        let text: &[u8] = match opc {
            op::word::INT_TO_STR => strconv::decimal(v < 0, v.unsigned_abs(), &mut buf),
            op::word::UINT_TO_STR => strconv::decimal(false, v as u64, &mut buf),
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
        let whole = self.task.regs[rb];
        let s = self.heap.get(whole)?;
        match copy_range(self.task.regs[rb + 1], self.task.regs[rb + 2], s.len()) {
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
        let sub = self.heap.get(self.task.regs[rb])?;
        let s = self.heap.get(self.task.regs[rb + 1])?;
        self.task.regs[ra] = s.find(sub).map_or(0, |p| p as i64 + 1);
        Ok(())
    }

    /// `StrUpperAscii A B` / `StrLowerAscii A B`: a new string with the ASCII letters changed.
    fn op_str_case(&mut self, ra: usize, rb: usize, upper: bool) -> Result<(), RefError> {
        let s = self.heap.get(self.task.regs[rb])?;
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
        let (sa, sb, sc) = (self.task.regs[ra], self.task.regs[rb], self.task.regs[rc]);
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
        let (sb, sc) = (self.task.regs[rb], self.task.regs[rc]);
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
        let (sa, sb, sc) = (self.task.regs[ra], self.task.regs[rb], self.task.regs[rc]);
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
        let s = self.heap.get(self.task.regs[rb])?;
        let i = self.task.regs[rc];
        if i < 1 || i > s.len() as i64 {
            return Err(str_index_error(i, s.len()));
        }
        self.task.regs[ra] = s.unit(i as usize - 1) as i64;
        Ok(())
    }

    /// `StrSetIndex A B C`: `rA[rB] := rC`, in place when `rA` is the only reference.
    fn op_str_set_index(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let len = self.heap.get(self.task.regs[ra])?.len();
        let i = self.task.regs[rb];
        if i < 1 || i > len as i64 {
            return Err(str_index_error(i, len));
        }
        let (index, unit) = (i as usize - 1, self.task.regs[rc] as u16);
        // The only reference writes in place; a shared string (the constant cache holds one of
        // every literal) is copied (`docs/spec/bytecode.md` 2.4)
        let in_place = match self.heap.unique_str_mut(self.task.regs[ra])? {
            Some(s) => s.set_unit_in_place(index, unit),
            None => false,
        };
        if !in_place {
            let n = self.heap.get(self.task.regs[ra])?.with_unit(index, unit)?;
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
    /// (`docs/spec/equality.md` 2.3), remembered by its block (`docs/spec/bytecode.md` 2.21).
    fn op_hash_str(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let h = self.heap.str_hash(self.task.regs[rb], &self.hash_keys)?;
        self.task.regs[ra] = Slot::from(h);
        Ok(())
    }

    /// `HashFind A B` (`docs/spec/bytecode.md` 2.23): the first entry of the chain of the hash
    /// `rB` (after the entry `r(B+4)` when that is 0 or more) whose remembered hash is `rB`; -1
    /// if there is none.
    fn op_hash_find(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let r = &self.task.regs;
        let arrays = [r[rb + 1], r[rb + 2], r[rb + 3], 0];
        self.task.regs[ra] = self.hash_chain(r[rb], arrays, r[rb + 4], None)?;
        Ok(())
    }

    /// `HashLookupInt A B C` / `HashLookupStr A B C` (`docs/spec/bytecode.md` 2.23): the entry
    /// whose key equals `r(B+1)` in the table of the instance `rB`, whose buckets, hashes, next
    /// entries and keys are its slots `C` to `C + 3` (read as `ObjGet` reads them), looked for
    /// with the hash `HashInt` / `HashStr` gives the key; -1 if there is none.
    fn op_hash_lookup(
        &mut self,
        ra: usize,
        rb: usize,
        slot: usize,
        cmp: KeyCmp,
    ) -> Result<(), RefError> {
        // The four fields at once; a handle that does not check or a lent slot is told apart
        // as `ObjGet` tells it
        let arrays = match self.heap.obj_fields_fast::<4>(self.task.regs[rb], slot) {
            Some(a) => a,
            None => {
                let index = self.heap.obj_index(self.task.regs[rb])?;
                let mut arrays = [0; 4];
                for (k, a) in arrays.iter_mut().enumerate() {
                    *a = self.obj_slot(index, slot + k)?;
                }
                arrays
            }
        };
        let key = self.task.regs[rb + 1];
        let h = match cmp {
            KeyCmp::Int => self.hash_keys.int(key as u64),
            // A key that is no string hashes as the empty string here, and `hash_chain` refuses
            // it (`Fatal`) before it follows the chain
            KeyCmp::Str => Slot::from(self.heap.str_hash(key, &self.hash_keys)?),
        };
        self.task.regs[ra] = self.hash_chain(h, arrays, -1, Some((cmp, key)))?;
        Ok(())
    }

    /// The chain of a hash table (`docs/spec/bytecode.md` 2.23) followed: from the bucket of
    /// `h` (or, when `start` is 0 or more, from the entry after `start`), the first entry whose
    /// remembered hash is `h` and, with a `key`, whose key compares equal to it; -1 if there is
    /// none. `arrays` are the buckets, the hashes, the next entries and the keys (read only with a
    /// `key`); anything the standard library does not make of them is `Fatal`.
    fn hash_chain(
        &self,
        h: Slot,
        arrays: [Slot; 4],
        start: Slot,
        key: Option<(KeyCmp, Slot)>,
    ) -> Result<Slot, RefError> {
        let fatal = |m: String| Err(RefError::Fatal(m));
        let heap = &self.heap;
        let whats = ["the buckets", "the hashes", "the next entries"];
        let ([buckets, hashes, next], keys) = match key {
            // The four arrays at once; keys of bytes (`TDictionary<Byte, V>`) on a path of
            // their own
            Some(_) => match heap.hash_arrays(arrays, [whats[0], whats[1], whats[2], "the keys"]) {
                Ok([b, h, n, k]) => ([b, h, n], heap::Elems::Slots(k)),
                Err(e) => match Self::hash_byte_keys(heap, arrays, whats) {
                    Some(found) => found?,
                    None => return Err(e.into()),
                },
            },
            None => (
                heap.hash_arrays([arrays[0], arrays[1], arrays[2]], whats)?,
                heap::Elems::Slots(&[]),
            ),
        };
        if !buckets.is_empty() && !buckets.len().is_power_of_two() {
            return fatal(format!(
                "the buckets of a hash table: length {} is not a power of 2",
                buckets.len()
            ));
        }
        // An entry of the arrays, or the error of an index outside them
        let at = |a: &[i64], i: i64| -> Result<i64, RefError> {
            match usize::try_from(i).ok().and_then(|i| a.get(i)) {
                Some(v) => Ok(*v),
                None => Err(RefError::Fatal(format!(
                    "a hash table entry {i} out of range 0..{}",
                    a.len()
                ))),
            }
        };
        let mut i = if start >= 0 {
            at(next, start)?
        } else if buckets.is_empty() {
            -1
        } else {
            buckets[(h as u64 & (buckets.len() as u64 - 1)) as usize]
        };
        // The key looked for, as it is compared
        let wanted_str = match key {
            Some((KeyCmp::Str, w)) => Some(heap.hash_key_str(w)?),
            _ => None,
        };
        let mut steps = 0usize;
        while i >= 0 {
            steps += 1;
            if steps > next.len() {
                return fatal("a chain of a hash table that goes round".into());
            }
            if at(hashes, i)? == h {
                let found = match key {
                    None => true,
                    Some((_, wanted)) => {
                        let k = keys.get(i).ok_or_else(|| {
                            RefError::Fatal(format!(
                                "a hash table entry {i} out of range 0..{}",
                                keys.len()
                            ))
                        })?;
                        match wanted_str {
                            None => k == wanted,
                            // The same block (or both empty) is equal without reading it
                            Some(w) => k == wanted || heap.hash_key_str(k)?.equals(w),
                        }
                    }
                };
                if found {
                    break;
                }
            }
            i = at(next, i)?;
        }
        #[cfg(test)]
        executed::add(steps as u64);
        Ok(i.max(-1))
    }

    /// The arrays of a hash table whose keys are a dynamic array of bytes ([`Heap::hash_keys`]):
    /// `None` when the keys are not one (the caller reports the error it has), otherwise the
    /// other three arrays as [`Heap::hash_arrays`] reads them.
    #[cold]
    #[allow(clippy::type_complexity)]
    fn hash_byte_keys<'h>(
        heap: &'h heap::Heap,
        arrays: [Slot; 4],
        whats: [&str; 3],
    ) -> Option<Result<([&'h [i64]; 3], heap::Elems<'h>), RefError>> {
        let keys = heap.hash_keys(arrays[3]).ok()?;
        if !matches!(keys, heap::Elems::Bytes(_)) {
            return None;
        }
        Some(
            heap.hash_arrays([arrays[0], arrays[1], arrays[2]], whats)
                .map(|a| (a, keys))
                .map_err(RefError::from),
        )
    }

    /// `HashRelink A B` (`docs/spec/bytecode.md` 2.23): the chains of the hash table made again.
    pub(super) fn op_hash_relink(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let (buckets, hashes, next, n) = (
            self.task.regs[ra],
            self.task.regs[rb],
            self.task.regs[rb + 1],
            self.task.regs[rb + 2],
        );
        let _linked = self.heap.hash_relink(buckets, hashes, next, n)?;
        #[cfg(test)]
        executed::add(_linked as u64);
        Ok(())
    }

    /// `ArrLen A B`.
    fn op_arr_len(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        self.task.regs[ra] = self.heap.len(self.task.regs[rb])? as i64;
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
        let (container, i) = (self.task.regs[rb], self.task.regs[rc]);
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
            self.task.regs[ra] = v;
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
        let (container, i) = (self.task.regs[ra], self.task.regs[rb]);
        if self.set_fast(container, i, self.task.regs[rc], managed) {
            return Ok(());
        }
        self.dyn_slot(container, i)?;
        self.heap.check_slot(container, i)?;
        self.slot_store(container, i, self.task.regs[rc], managed)
    }

    /// `FieldGet` / `ValGet` and their `Ref` forms: `rA := slot i of the block rB`. A default
    /// record or array (0) has default slots.
    fn op_slot_get(&mut self, ra: usize, rb: usize, i: i64, managed: bool) -> Result<(), RefError> {
        self.slot_get(ra, self.task.regs[rb], i, managed)
    }

    /// `RefFieldGet` / `RefFieldGetRef`: `rA :=` slot `i` of the record the reference slot `rB`
    /// points to (`docs/spec/bytecode.md` 2.8). A default record (0) has default slots.
    fn op_ref_field_get(
        &mut self,
        ra: usize,
        rb: usize,
        i: i64,
        managed: bool,
    ) -> Result<(), RefError> {
        let place = self.task.regs[rb];
        if deref(place).0 == lend::PLACE_REF {
            // The slot of a flat place (`docs/design/records.md` 3.7 B)
            let (j, loan) = self.place_loan(place)?;
            let (r, k) = self.place_slot(&loan, i)?;
            return self.place_get(ra, r, k, Some(j), managed);
        }
        let container = self.ref_read(place)?;
        self.slot_get(ra, container, i, managed)
    }

    /// `RefFieldSet` / `RefFieldSetRef`: slot `i` of the record the reference slot `rA` points to
    /// := `rC` (`docs/spec/bytecode.md` 2.8). The record is made unique first, as `Unique` with
    /// the type `ty` does, and the location takes the new block (uncounted, as `StoreRef`).
    fn op_ref_field_set(
        &mut self,
        ra: usize,
        i: i64,
        rc: usize,
        ty: u32,
        managed: bool,
    ) -> Result<(), RefError> {
        let place = self.task.regs[ra];
        if deref(place).0 == lend::PLACE_REF {
            // The slot of a flat place, written in place (its container was made unique before it
            // was lent, or is a dynamic array; the type word is not needed)
            let (j, loan) = self.place_loan(place)?;
            let (r, k) = self.place_slot(&loan, i)?;
            return self.place_set(r, k, self.task.regs[rc], Some(j), managed);
        }
        let loc = self.ref_target(place)?;
        let mut container = self.loc_get(loc)?;
        if !self.heap.unique_fast(container) {
            self.check_copy_shared(container)?;
            container = self.heap.unique(container, ty)??;
            self.loc_replace(loc, container)?;
        }
        if self.set_fast(container, i, self.task.regs[rc], managed) {
            return Ok(());
        }
        self.heap.check_slot(container, i)?;
        self.slot_store(container, i, self.task.regs[rc], managed)
    }

    /// `rA :=` slot `i` of the block `container` (counted when `managed`); 0 has default slots.
    fn slot_get(
        &mut self,
        ra: usize,
        container: Slot,
        i: i64,
        managed: bool,
    ) -> Result<(), RefError> {
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
            self.task.regs[ra] = v;
        }
        Ok(())
    }

    /// `FieldSet` / `ValSet` and their `Ref` forms: slot `i` of the block `rA` := `rC`.
    fn op_slot_set(&mut self, ra: usize, i: i64, rc: usize, managed: bool) -> Result<(), RefError> {
        if self.set_fast(self.task.regs[ra], i, self.task.regs[rc], managed) {
            return Ok(());
        }
        self.heap.check_slot(self.task.regs[ra], i)?;
        self.slot_store(self.task.regs[ra], i, self.task.regs[rc], managed)
    }

    /// The fast path of a slot read (`docs/design/vm.md` 3.14 (a′)): puts the slot's value `v`
    /// into `ra`, counting it when `managed`. `false`, with nothing changed, when counting needs
    /// the body ([`Heap::assign_ref_fast`]).
    #[inline(always)]
    fn get_fast(&mut self, ra: usize, v: Slot, managed: bool) -> bool {
        if managed && !self.heap.assign_ref_fast(v, self.task.regs[ra]) {
            return false;
        }
        self.task.regs[ra] = v;
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
            let old = self.heap.slot_replace(container, i, v)?;
            self.heap.release(old)?;
        } else {
            self.heap.slot_replace(container, i, v)?;
        }
        Ok(())
    }

    /// `ElemAt A B C`: the cursor `rA`, `r(A+1)` to element `rC` of the array `rB` (a dynamic
    /// or static array block, or 0): the element's first slot and the array, uncounted
    /// (`docs/spec/bytecode.md` 2.8). The index is checked against the number of elements.
    fn op_elem_at(&mut self, ra: usize, rb: usize, rc: usize) -> Result<(), RefError> {
        let (arr, i) = (self.task.regs[rb], self.task.regs[rc]);
        let (n, w) = if arr == 0 {
            (0, 1)
        } else {
            let ty = self.heap.agg_type(arr)?;
            match self.heap.type_entry(ty).map(|t| t.kind) {
                Some(bytecode::TYPE_ARRAY | bytecode::TYPE_DYNARRAY) => {}
                _ => {
                    return Err(RefError::Fatal(format!(
                        "`ElemAt` of the heap block {arr}, which is not an array"
                    )));
                }
            }
            (self.heap.len(arr)?, self.heap.width_of(ty))
        };
        if !(0..n as i64).contains(&i) {
            return Err(RefError::Rt(rt(exc::RANGE_ERROR, dyn_index_message(i, n))));
        }
        self.task.regs[ra] = i * w as i64;
        self.task.regs[ra + 1] = arr;
        Ok(())
    }

    /// The block and slot number a cursor `r(at)`, `r(at+1)` names with `off` added: `None` for
    /// the container 0 (every slot at its default value). A negative or overflowing position is
    /// `Fatal`; one past the block's slots is reported by the access.
    fn cursor_slot(
        &self,
        at: usize,
        off: usize,
    ) -> Result<Option<(heap::StrRef, usize)>, RefError> {
        let Some(r) = self.heap.cursor_block(self.task.regs[at + 1])? else {
            return Ok(None);
        };
        let pos = self.task.regs[at];
        match usize::try_from(pos).ok().and_then(|p| p.checked_add(off)) {
            Some(i) => Ok(Some((r, i))),
            None => Err(RefError::Fatal(format!("bad cursor position {pos}"))),
        }
    }

    /// `SlotGet` / `SlotGetRef A B C`: `rA :=` slot `C` of the flat place at the cursor `rB`,
    /// `r(B+1)` (0 for the container 0).
    fn op_cursor_get(
        &mut self,
        ra: usize,
        rb: usize,
        off: usize,
        managed: bool,
    ) -> Result<(), RefError> {
        match self.cursor_slot(rb, off)? {
            Some((r, i)) => self.place_get(ra, r, i, None, managed),
            None if managed => Ok(self.put(ra, 0)?),
            None => {
                self.task.regs[ra] = 0;
                Ok(())
            }
        }
    }

    /// `SlotSet` / `SlotSetRef A B C`: slot `B` of the flat place at the cursor `rA`, `r(A+1)`
    /// := `rC`. The container is written in place: a dynamic array or an instance (shared), or a
    /// record or static array made unique before (the container 0 or a shared one is `Fatal`).
    fn op_cursor_set(
        &mut self,
        ra: usize,
        off: usize,
        rc: usize,
        managed: bool,
    ) -> Result<(), RefError> {
        let Some((r, i)) = self.cursor_slot(ra, off)? else {
            return Err(RefError::Fatal(
                "write to a flat place of a default value".into(),
            ));
        };
        self.place_set(r, i, self.task.regs[rc], None, managed)
    }

    /// `rA :=` slot `i` of the record, array or instance block `r` (counted when `managed`),
    /// refused while a loan after `after` holds the slot ([`heap::Heap::loan_after`]).
    pub(super) fn place_get(
        &mut self,
        ra: usize,
        r: heap::StrRef,
        i: usize,
        after: Option<usize>,
        managed: bool,
    ) -> Result<(), RefError> {
        let v = self.heap.any_slot(r, i)?;
        self.check_place_slot(r, i, after)?;
        if managed {
            self.share(ra, v)
        } else {
            self.task.regs[ra] = v;
            Ok(())
        }
    }

    /// Slot `i` of the record, array or instance block `r` := `v` (counting `v` and letting go
    /// of the old value when `managed`), refused while a loan after `after` holds the slot.
    pub(super) fn place_set(
        &mut self,
        r: heap::StrRef,
        i: usize,
        v: Slot,
        after: Option<usize>,
        managed: bool,
    ) -> Result<(), RefError> {
        self.heap.any_slot(r, i)?;
        self.check_place_slot(r, i, after)?;
        if managed {
            self.heap.check_share(v)?;
            self.heap.add_ref(v)?;
            match self.heap.any_slot_mut(r, i) {
                Ok(slot) => {
                    let old = std::mem::replace(slot, v);
                    self.heap.release(old)?;
                }
                Err(e) => {
                    self.heap.release(v)?;
                    return Err(e.into());
                }
            }
        } else {
            *self.heap.any_slot_mut(r, i)? = v;
        }
        Ok(())
    }

    /// Refuses touching slot `i` of the block `r` while a loan after `after` (any loan with
    /// `None`) holds it (`EAliasError`).
    fn check_place_slot(
        &self,
        r: heap::StrRef,
        i: usize,
        after: Option<usize>,
    ) -> Result<(), RefError> {
        if self.heap.lending() && self.heap.loan_after(r, i, after).is_some() {
            return Err(self.heap.aliased_slot(r, i as i64, false).into());
        }
        Ok(())
    }

    /// `AggLoad A B` with the type `ty` (the type word): `rA :=` the record or static array
    /// value of type `ty` in the flat place at the cursor `rB`, `r(B+1)`.
    pub(super) fn op_agg_load(&mut self, ra: usize, rb: usize, ty: u32) -> Result<(), RefError> {
        match self.cursor_slot(rb, 0)? {
            Some((r, i)) => self.agg_load(ra, Some(r), i, ty, None),
            None => self.agg_load(ra, None, 0, ty, None),
        }
    }

    /// `AggStore A B` with the type `ty` (the type word): the flat place at the cursor `rA`,
    /// `r(A+1)` := the record or static array value `rB` of type `ty` (0 is the default).
    pub(super) fn op_agg_store(&mut self, ra: usize, rb: usize, ty: u32) -> Result<(), RefError> {
        let Some((r, i)) = self.cursor_slot(ra, 0)? else {
            return Err(RefError::Fatal(
                "write to a flat place of a default value".into(),
            ));
        };
        self.agg_store(r, i, ty, self.task.regs[rb], None)
    }

    /// `rA :=` the value of type `ty` whose slots are those of the flat place from slot `i` of
    /// the block `r` (all 0 for `None`), counting the managed ones; the old `rA` is let go.
    /// When every slot is 0 the value is 0 (the default). Otherwise the value goes into the
    /// block `rA` holds when only `rA` holds it, it is of type `ty` and it contains no lent
    /// location (no block is made); else into a new block (`docs/design/records.md` 3.5 B).
    /// Refused while a loan after `after` holds one of the slots, or one of them holds a value
    /// containing a lent location (`EAliasError`).
    pub(super) fn agg_load(
        &mut self,
        ra: usize,
        r: Option<heap::StrRef>,
        i: usize,
        ty: u32,
        after: Option<usize>,
    ) -> Result<(), RefError> {
        let w = self.place_width(ty)?;
        let mut values = std::mem::take(&mut self.place_buf);
        values.clear();
        let gathered = (|| -> Result<(), RefError> {
            let Some(r) = r else {
                values.resize(w, 0);
                return Ok(());
            };
            match after {
                // Through the reference to the flat place, only the loans after it refuse
                Some(_) => {
                    for k in i..i + w {
                        self.check_place_slot(r, k, after)?;
                    }
                    let slots = self.heap.any_slots(r)?.1;
                    for (k, v) in slots.iter().skip(i).take(w).enumerate() {
                        if self.heap.slot_kind(ty, k) == bytecode::SLOT_REF {
                            self.heap.check_share(*v)?;
                        }
                    }
                }
                // Copying a lent slot or a value containing a lent location
                None => self.heap.check_copy_slots(r, i, i + w)?,
            }
            let slots = self.heap.any_slots(r)?.1;
            let range = slots.get(i..i + w).ok_or_else(|| {
                RefError::Fatal(format!(
                    "flat place {i}..{} out of range in heap block {r}",
                    i + w
                ))
            })?;
            values.extend_from_slice(range);
            Ok(())
        })();
        let result = gathered.and_then(|()| self.agg_put(ra, &values, ty));
        self.place_buf = values;
        result
    }

    /// The end of [`Isolate::agg_load`]: `rA :=` the value of type `ty` with the slots `values`.
    fn agg_put(&mut self, ra: usize, values: &[Slot], ty: u32) -> Result<(), RefError> {
        if values.iter().all(|v| *v == 0) {
            return Ok(self.put(ra, 0)?);
        }
        for (k, v) in values.iter().enumerate() {
            self.heap.add_ref_kind(*v, self.heap.slot_kind(ty, k))?;
        }
        let old = self.task.regs[ra];
        if self.heap.reusable_box(old, ty) {
            for (k, v) in values.iter().enumerate() {
                let slot = self.heap.any_slot_mut(old, k)?;
                let before = std::mem::replace(slot, *v);
                self.heap.release_kind(before, self.heap.slot_kind(ty, k))?;
            }
            return Ok(());
        }
        let mut slots = Vec::new();
        let made =
            heap::try_extend(&mut slots, values).and_then(|()| self.heap.alloc_agg(ty, slots));
        match made {
            Ok(b) => Ok(self.put(ra, b)?),
            Err(OutOfMemory) => {
                for (k, v) in values.iter().enumerate() {
                    self.heap.release_kind(*v, self.heap.slot_kind(ty, k))?;
                }
                Err(OutOfMemory.into())
            }
        }
    }

    /// The flat place of type `ty` from slot `i` of the block `r` := the record or static array
    /// value `v` (0 is the default): its slots are counted, then the old ones let go. Refused
    /// while a loan after `after` holds one of the slots, or when `v` contains a lent location
    /// (`EAliasError`).
    pub(super) fn agg_store(
        &mut self,
        r: heap::StrRef,
        i: usize,
        ty: u32,
        v: Slot,
        after: Option<usize>,
    ) -> Result<(), RefError> {
        let w = self.place_width(ty)?;
        let n = self.heap.any_slots(r)?.1.len();
        if i + w > n {
            return Err(RefError::Fatal(format!(
                "flat place {i}..{} out of range in heap block {r}",
                i + w
            )));
        }
        for k in i..i + w {
            self.check_place_slot(r, k, after)?;
        }
        self.heap.check_share(v)?;
        let mut values = std::mem::take(&mut self.place_buf);
        values.clear();
        let result = (|| -> Result<(), RefError> {
            if v == 0 {
                values.resize(w, 0);
            } else {
                let source = self.heap.slot_count(v)?;
                if source != w {
                    return Err(RefError::Fatal(format!(
                        "heap block {v} has {source} slots, not the {w} of a flat place"
                    )));
                }
                values.extend_from_slice(self.heap.elems(v)?);
            }
            for (k, x) in values.iter().enumerate() {
                self.heap.add_ref_kind(*x, self.heap.slot_kind(ty, k))?;
            }
            // Writable checked before anything is let go
            self.heap.any_slots_mut(r)?;
            for (k, x) in values.iter().enumerate() {
                let old = std::mem::replace(self.heap.any_slot_mut(r, i + k)?, *x);
                self.heap.release_kind(old, self.heap.slot_kind(ty, k))?;
            }
            Ok(())
        })();
        self.place_buf = values;
        result
    }

    /// `Unique A Bx`: makes the block `rA` (of type `Bx`) the only reference, copying it if shared.
    fn op_unique(&mut self, ra: usize, ty: u32) -> Result<(), RefError> {
        if self.heap.unique_fast(self.task.regs[ra]) {
            return Ok(());
        }
        self.check_copy_shared(self.task.regs[ra])?;
        self.task.regs[ra] = self.heap.unique(self.task.regs[ra], ty)??;
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
        let frame = *self.task.frames.last().expect("`Ret` runs in a frame");
        let v = value.map_or(0, |r| std::mem::take(&mut self.task.regs[r]));
        self.release_frame(&frame)?;
        let fr = self.task.frames.pop().expect("the frame was just read");
        let Some(caller) = self.task.frames.last() else {
            if self.heap.job_open() {
                self.heap.stamp_job(heap::JobOrigin::RetLast, 0);
            }
            return Ok(true);
        };
        if value.is_some() {
            self.task.regs[fr.ret() as usize] = v;
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
            self.heap.stamp_job(origin, self.task.frames.len() as u32);
        }
        Ok(false)
    }
}
