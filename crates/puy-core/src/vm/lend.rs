//! Fields and elements passed to `var` / `out` parameters: `Lend` takes the value out of its
//! location and `Unlend` gives it back (`docs/spec/types.md` chapter 3, `docs/spec/bytecode.md`
//! 2.8, `docs/design/lifetime.md` 3.7).

use super::heap::{Aliased, Loan, LoanContainer};
use super::*;

/// The `EAliasError` of touching a lent location (`docs/spec/semantics.md` 7.1).
fn alias_error(a: Aliased) -> RtError {
    let message = match a {
        Aliased::Elem {
            index: Some(i),
            twice: false,
        } => format!("array element {i} is passed to a var parameter of a call in progress"),
        Aliased::Elem {
            index: None,
            twice: false,
        } => "an array element is passed to a var parameter of a call in progress".into(),
        Aliased::Elem {
            index: Some(i),
            twice: true,
        } => format!("array element {i} is passed to two var parameters"),
        Aliased::Elem {
            index: None,
            twice: true,
        } => "an array element is passed to two var parameters".into(),
        Aliased::Field { twice: false } => {
            "a record field is passed to a var parameter of a call in progress".into()
        }
        Aliased::Field { twice: true } => "a record field is passed to two var parameters".into(),
        Aliased::ObjField { twice: false } => {
            "an object field is passed to a var parameter of a call in progress".into()
        }
        Aliased::ObjField { twice: true } => {
            "an object field is passed to two var parameters".into()
        }
        Aliased::Copy => "a value containing a location passed to a var parameter of a call in \
                          progress cannot be copied"
            .into(),
    };
    rt(exc::ALIAS_ERROR, message)
}

impl From<Aliased> for RefError {
    fn from(a: Aliased) -> Self {
        RefError::Rt(alias_error(a))
    }
}

/// Whether the root of a loan is an instance's handle (or `nil`) rather than a reference slot,
/// whose upper 8 bits hold its kind (`docs/spec/bytecode.md` 1.5).
fn is_handle(root: Slot) -> bool {
    deref(root).0 == 0
}

impl Isolate {
    /// Writes an existing value into a register, counting it (`put_shared`), unless it is a record
    /// or static array containing a lent location (copying it is touching that location).
    pub(super) fn share(&mut self, i: usize, v: Slot) -> Result<(), RefError> {
        self.heap.check_share(v)?;
        Ok(self.put_shared(i, v)?)
    }

    /// `Lend A B C`: takes the value of the location at root `rB` and path `r(A+1)..r(A+C)` into
    /// `rA` and lends the location to the current frame's call.
    pub(super) fn op_lend(&mut self, ra: usize, rb: usize, c: usize) -> Result<(), RefError> {
        // A slot of a record or array in a register, a global variable or a cell
        let root = self.regs[rb];
        if c == 1
            && !is_handle(root)
            && let Some(container) = self.fast_ref_read(root)
        {
            let loan = Loan {
                value: ra,
                root,
                path: ra + 1,
                len: 1,
                depth: self.frames.len(),
                loc: None,
                containers: 0,
            };
            if let Some(value) = self.heap.lend_fast(container, self.regs[ra + 1], loan) {
                // `rA` is 0 (the compiler clears it), so nothing is let go
                self.regs[ra] = value;
                return Ok(());
            }
        }
        let first = self.heap.loan_containers().len();
        let result = self.lend(ra, rb, c, first);
        if result.is_err() {
            self.heap.truncate_loan_containers(first);
        }
        result
    }

    fn lend(&mut self, ra: usize, rb: usize, c: usize, first: usize) -> Result<(), RefError> {
        let depth = self.frames.len();
        let root = self.regs[rb];
        // An instance's handle as the root: the first step is the instance's slot
        // (`docs/spec/objects.md` 14.3). An instance of a reference counted class is counted for
        // the loan (`finish_lend`), so that it lives through the call
        let (mut v, start) = if is_handle(root) {
            let index = self.heap.obj_index(root)?;
            let r = index as heap::StrRef + 1;
            let slot = self.regs[ra + 1];
            let i = usize::try_from(slot).map_err(|_| RefError::Fatal("bad slot number".into()))?;
            self.heap.push_loan_container(LoanContainer {
                block: Some(r),
                ty: self.heap.obj_class(root)?,
                marked: false,
            })?;
            // The slot itself is lent, or a slot on the way lent to this same call is passed twice
            if let Some(l) = self.heap.loan_of(r, i) {
                let twice = l.depth == depth;
                return Err(self.heap.aliased_slot(r, slot, twice).into());
            }
            if c == 1 {
                let value = self.heap.obj_replace_at(index, i, 0)?;
                return self.finish_lend(ra, root, c, first, (r, i), value);
            }
            (self.heap.obj_slot_at(index, i)?, 1)
        } else {
            (self.ref_read(root)?, 0)
        };
        for k in start..c {
            let slot = self.regs[ra + 1 + k];
            // The compiler checks the innermost dynamic array before a record that may be 0, so
            // a default container here is an empty dynamic array
            if v == 0 {
                return Err(RefError::Rt(rt(
                    exc::RANGE_ERROR,
                    dyn_index_message(slot, 0),
                )));
            }
            let ty = self.heap.agg_type(v)?;
            let n = self.heap.slot_count(v)?;
            if !(0..n as i64).contains(&slot) {
                return Err(RefError::Rt(rt(
                    exc::RANGE_ERROR,
                    dyn_index_message(slot, n),
                )));
            }
            self.heap.push_loan_container(LoanContainer {
                block: Some(v),
                ty,
                marked: false,
            })?;
            if k + 1 < c {
                // A slot on the way lent to this same call is passed twice
                if let Some(l) = self.heap.loan_of(v, slot as usize) {
                    let twice = l.depth == depth;
                    return Err(self.heap.aliased_slot(v, slot, twice).into());
                }
                v = self
                    .heap
                    .slot(v, slot)?
                    .ok_or_else(|| RefError::Fatal("lending path out of range".into()))?;
            }
        }
        let i = self.regs[ra + c];
        if let Some(l) = self.heap.loan_of(v, i as usize) {
            let twice = l.depth == depth;
            return Err(self.heap.aliased_slot(v, i, twice).into());
        }
        // The compiler made a record or static array container unique (`slot_mut` checks it)
        let value = std::mem::take(self.heap.slot_mut(v, i)?);
        self.finish_lend(ra, root, c, first, (v, i as usize), value)
    }

    /// The end of `Lend`: records the loan of the slot `loc` whose `value` was taken out, and
    /// moves the value to `rA`. On failure the value goes back to its slot.
    fn finish_lend(
        &mut self,
        ra: usize,
        root: Slot,
        c: usize,
        first: usize,
        loc: (heap::StrRef, usize),
        value: Slot,
    ) -> Result<(), RefError> {
        let loan = Loan {
            value: ra,
            root,
            path: ra + 1,
            len: c,
            depth: self.frames.len(),
            loc: Some(loc),
            containers: first,
        };
        if let Err(e) = self.heap.push_loan(loan) {
            *self.loc_slot_mut(loc)? = value;
            return Err(e.into());
        }
        // `rA` is 0 (the compiler clears it), so nothing is let go
        self.regs[ra] = value;
        if is_handle(root) {
            self.heap.add_ref_obj(root)?;
        }
        Ok(())
    }

    /// The slot `loc` (block, slot number) of a record, array or instance block for writing.
    fn loc_slot_mut(&mut self, loc: (heap::StrRef, usize)) -> Result<&mut Slot, RefError> {
        let (r, i) = loc;
        if self.heap.is_obj_block(r) {
            Ok(self.heap.obj_slot_mut(r, i)?)
        } else {
            Ok(self.heap.slot_mut(r, i as i64)?)
        }
    }

    /// Lets go of the value `old` that slot `slot` of a block of type `ty` held.
    fn release_slot_value(&mut self, ty: u32, slot: Slot, old: Slot) -> Result<(), BadRef> {
        let kind =
            usize::try_from(slot).map_or(bytecode::SLOT_PLAIN, |i| self.heap.slot_kind(ty, i));
        self.heap.release_kind(old, kind)
    }

    /// Gives back the values lent to the current frame's call (`Unlend`), oldest first. With
    /// `raise`, returns the first `ERangeError` (or `EOutOfMemory`) of a location that could not
    /// be found again; otherwise (unwinding) those values are only let go.
    pub(super) fn give_back(&mut self, raise: bool) -> Result<Option<RtError>, BadRef> {
        if !self.heap.lending() || self.give_back_fast() {
            return Ok(None);
        }
        let mut loans = std::mem::take(&mut self.loan_buf);
        let mut containers = std::mem::take(&mut self.loan_container_buf);
        self.heap
            .take_loans(self.frames.len(), &mut loans, &mut containers);
        let mut first = None;
        let mut fatal = None;
        for loan in &loans {
            let value = std::mem::take(&mut self.regs[loan.value]);
            let walked = &containers[loan.containers..loan.containers + loan.len];
            match self.put_back(loan, walked, value) {
                Ok(()) => {}
                Err(RefError::Rt(e)) => {
                    // The value is let go (a managed one holds a reference)
                    let last = walked.last().expect("a lending path is never empty");
                    let slot = self.regs[loan.path + loan.len - 1];
                    if let Err(b) = self.release_slot_value(last.ty, slot, value) {
                        fatal.get_or_insert(b);
                    }
                    first.get_or_insert(e);
                }
                Err(RefError::Fatal(m)) => {
                    fatal.get_or_insert(BadRef { message: m });
                }
            }
            // An instance as the root was counted for the loan (`docs/spec/objects.md` 14.3)
            if is_handle(loan.root)
                && let Err(b) = self.heap.release_obj(loan.root)
            {
                fatal.get_or_insert(b);
            }
        }
        self.loan_buf = loans;
        self.loan_container_buf = containers;
        if let Some(b) = fatal {
            return Err(b);
        }
        Ok(if raise { first } else { None })
    }

    /// `Unlend` when every loan of the current frame's call is one slot that `Lend` reached in
    /// one step from a reference slot to a register, a global variable or a cell, and the path
    /// would lead to the same slot again without a check or copy ([`Heap::give_back_ready`]):
    /// the values go straight back. `false`, with nothing changed, otherwise.
    fn give_back_fast(&mut self) -> bool {
        let from = self.heap.loans_from(self.frames.len());
        let loans = &self.heap.loans()[from..];
        if loans.is_empty() {
            return false;
        }
        for loan in loans {
            let Some((r, i)) = loan.loc else {
                return false;
            };
            if loan.len != 1
                || is_handle(loan.root)
                || self.regs[loan.path] != i as Slot
                || self.fast_ref_read(loan.root) != Some(r)
                || !self.heap.give_back_ready(r, i)
            {
                return false;
            }
        }
        let regs = &mut self.regs;
        self.heap
            .give_back_fast(from, |loan| std::mem::take(&mut regs[loan.value]));
        true
    }

    /// Writes a lent value back, following the path again from the root: records and static
    /// arrays after the innermost dynamic array are made unique (a default one becomes a block of
    /// the type `Lend` saw), dynamic arrays are written shared and their indices checked.
    fn put_back(
        &mut self,
        loan: &Loan,
        walked: &[LoanContainer],
        value: Slot,
    ) -> Result<(), RefError> {
        let last_dyn = walked.iter().rposition(|c| self.heap.is_dyn_type(c.ty));
        // An instance's handle as the root: it is checked again, so a freed instance raises
        // `EAccessViolation` (`docs/spec/objects.md` 14.3)
        let (mut parent, start) = if is_handle(loan.root) {
            let index = self.heap.obj_index(loan.root)?;
            let slot = self.regs[loan.path] as usize;
            (Loc::ObjSlot(index, slot), 1)
        } else {
            (self.ref_target(loan.root)?, 0)
        };
        if start == walked.len() {
            let old = self.loc_replace(parent, value)?;
            let slot = self.regs[loan.path];
            self.release_slot_value(walked[0].ty, slot, old)?;
            return Ok(());
        }
        let mut v = self.loc_get(parent)?;
        for (k, c) in walked.iter().enumerate().skip(start) {
            let slot = self.regs[loan.path + k];
            if last_dyn.is_none_or(|d| k > d) {
                let n = self.heap.unique(v, c.ty)??;
                if n != v {
                    // `unique` let go of the old value when it copied it
                    self.loc_replace(parent, n)?;
                    v = n;
                }
            } else if v == 0 {
                if self.heap.is_dyn_type(c.ty) {
                    return Err(RefError::Rt(rt(
                        exc::RANGE_ERROR,
                        dyn_index_message(slot, 0),
                    )));
                }
                // A default record before a dynamic array: everything in it is default too
                continue;
            }
            let n = self.heap.slot_count(v)?;
            if !(0..n as i64).contains(&slot) {
                return Err(RefError::Rt(rt(
                    exc::RANGE_ERROR,
                    dyn_index_message(slot, n),
                )));
            }
            if k + 1 < walked.len() {
                parent = Loc::Slot(v, slot);
                v = self.loc_get(parent)?;
            } else {
                // The slot is 0 unless the container was replaced or grown back meanwhile
                let old = std::mem::replace(self.heap.slot_mut(v, slot)?, value);
                self.release_slot_value(c.ty, slot, old)?;
            }
        }
        Ok(())
    }
}
