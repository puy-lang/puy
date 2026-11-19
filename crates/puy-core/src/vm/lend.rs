//! Fields and elements passed to `var` / `out` parameters: `Lend` takes the value out of its
//! location and `Unlend` gives it back (`docs/spec/types.md` chapter 3, `docs/spec/bytecode.md`
//! 2.8, `docs/design/lifetime.md` 3.7).

use super::heap::{Aliased, Loan, LoanContainer};
use super::*;
use crate::bytecode::TypeEntry;

/// The kind of a reference slot to a lent flat place (`docs/spec/bytecode.md` 1.5).
pub(super) const PLACE_REF: u8 = 4;

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
        let root = self.task.regs[rb];
        if c == 1
            && !is_handle(root)
            && let Some(container) = self.fast_ref_read(root)
        {
            let loan = Loan {
                value: ra,
                root,
                path: ra + 1,
                len: 1,
                depth: self.task.frames.len(),
                loc: None,
                width: 1,
                place: None,
                kind: bytecode::SLOT_PLAIN,
                evacuated: false,
                boxed: false,
                containers: 0,
                ncontainers: 0,
            };
            if let Some(value) = self.heap.lend_fast(container, self.task.regs[ra + 1], loan) {
                // `rA` is 0 (the compiler clears it), so nothing is let go
                self.task.regs[ra] = value;
                return Ok(());
            }
        }
        self.lend_with(ra, rb, c, None)
    }

    /// `LendPlace A B C` with the type `ty` (the type word): lends the flat place of type `ty`
    /// at root `rB` and path `r(A+1)..r(A+C)` to the current frame's call, and puts a reference
    /// slot of kind 4 to it in `rA` (`docs/spec/bytecode.md` 2.8).
    pub(super) fn op_lend_place(
        &mut self,
        ra: usize,
        rb: usize,
        c: usize,
        ty: u32,
    ) -> Result<(), RefError> {
        if self.lend_place_fast(ra, rb, c, ty) {
            return Ok(());
        }
        self.lend_with(ra, rb, c, Some(ty))
    }

    /// `LendPlace` on the fast path: an element of a dynamic array in a register, a
    /// global variable or a cell, or a place in one, with nothing in the way
    /// ([`Heap::lend_place_fast`]). `false` with nothing changed otherwise.
    fn lend_place_fast(&mut self, ra: usize, rb: usize, c: usize, ty: u32) -> bool {
        let root = self.task.regs[rb];
        if (c == 1 || c == 2)
            && !is_handle(root)
            && let Some(container) = self.fast_ref_read(root)
        {
            let Some(width) = self
                .heap
                .type_entry(ty)
                .and_then(TypeEntry::flat_width)
                .map(|w| w as usize)
            else {
                return false;
            };
            let inner = (c == 2).then(|| self.task.regs[ra + 2]);
            let loan = Loan {
                value: ra,
                root,
                path: ra + 1,
                len: c,
                depth: self.task.frames.len(),
                loc: None,
                width,
                place: Some(ty),
                kind: bytecode::SLOT_PLAIN,
                evacuated: false,
                boxed: false,
                containers: 0,
                ncontainers: 0,
            };
            if let Some(index) =
                self.heap
                    .lend_place_fast(container, self.task.regs[ra + 1], inner, width, ty, loan)
            {
                self.task.regs[ra] = make_ref(PLACE_REF, index as u64);
                return true;
            }
        }
        false
    }

    /// `Lend` (`place` is `None`) or `LendPlace` (`place` is the type of the flat place) by
    /// walking the path.
    fn lend_with(
        &mut self,
        ra: usize,
        rb: usize,
        c: usize,
        place: Option<u32>,
    ) -> Result<(), RefError> {
        let first = self.heap.loan_containers().len();
        let result = self.lend(ra, rb, c, first, place);
        if result.is_err() {
            self.heap.truncate_loan_containers(first);
        }
        result
    }

    fn lend(
        &mut self,
        ra: usize,
        rb: usize,
        c: usize,
        first: usize,
        place: Option<u32>,
    ) -> Result<(), RefError> {
        let depth = self.task.frames.len();
        let root = self.task.regs[rb];
        let ((r, i), after) = self.lend_walk(root, ra + 1, c, depth)?;
        let ncontainers = self.heap.loan_containers().len() - first;
        let (ty, n) = {
            let (t, s) = self.heap.any_slots(r)?;
            (t, s.len())
        };
        let mut loan = Loan {
            value: ra,
            root,
            path: ra + 1,
            len: c,
            depth,
            loc: Some((r, i)),
            width: 1,
            place: None,
            kind: self.heap.slot_kind(ty, i),
            evacuated: false,
            boxed: false,
            containers: first,
            ncontainers,
        };
        let Some(pty) = place else {
            if let Some((_, l)) = self.heap.loan_after(r, i, after) {
                let twice = l.depth == depth;
                return Err(self.heap.aliased_slot(r, i as i64, twice).into());
            }
            // The compiler made a record or static array container unique (`any_slot_mut`
            // checks it)
            let value = std::mem::take(self.heap.any_slot_mut(r, i)?);
            return self.finish_lend(ra, loan, value);
        };
        let width = self.place_width(pty)?;
        if i + width > n {
            return Err(RefError::Fatal(format!(
                "flat place {i}..{} out of range in heap block {r}",
                i + width
            )));
        }
        for k in i..i + width {
            if let Some((_, l)) = self.heap.loan_after(r, k, after) {
                let twice = l.depth == depth;
                return Err(self.heap.aliased_slot(r, k as i64, twice).into());
            }
        }
        // The callee writes the slots in place: a record or static array container must be
        // unique already, as for `Lend`
        self.heap.any_slots_mut(r)?;
        loan.width = width;
        loan.place = Some(pty);
        loan.kind = bytecode::SLOT_PLAIN;
        let index = self.heap.loans().len();
        self.finish_lend(ra, loan, make_ref(4, index as u64))
    }

    /// The width of a flat place of the record or static array type `ty` (the verifier checked
    /// the type word).
    pub(super) fn place_width(&self, ty: u32) -> Result<usize, RefError> {
        self.heap
            .type_entry(ty)
            .and_then(TypeEntry::flat_width)
            .map(|w| w as usize)
            .ok_or_else(|| RefError::Fatal(format!("type {ty} is not a record or static array")))
    }

    /// Walks the path `r(path)..r(path+len-1)` of `Lend` / `LendPlace` from `root`, pushing the
    /// containers it goes through, and returns the slot it reaches with the loan whose flat
    /// place the root is (a reference slot of kind 4) when the slot is in that place: only the
    /// loans after it refuse that slot ([`Heap::loan_after`]).
    ///
    /// A step selects a slot of its container: a slot number in a record or an instance, an
    /// element index in an array, followed in an array whose elements take more than one slot by
    /// the slot number in the element (`docs/spec/bytecode.md` 2.8). With steps left, the walk
    /// goes on into the value of that slot.
    fn lend_walk(
        &mut self,
        root: Slot,
        path: usize,
        len: usize,
        depth: usize,
    ) -> Result<((heap::StrRef, usize), Option<usize>), RefError> {
        let (mut at, mut k, mut after, mut v) = if is_handle(root) {
            // An instance's handle as the root: the first step is the instance's slot
            // (`docs/spec/objects.md` 14.3)
            let r = self.heap.obj_index(root)? as heap::StrRef + 1;
            let (ty, n) = {
                let (t, s) = self.heap.any_slots(r)?;
                (t, s.len())
            };
            self.push_container(r, ty)?;
            let (i, used) = self.path_slot(ty, n, path, 0, len)?;
            (Some((r, i)), used, None, 0)
        } else if deref(root).0 == PLACE_REF {
            let (j, loan) = self.place_loan(root)?;
            let ((r, i), used) = self.place_path_slot(&loan, path, len)?;
            let ty = self.heap.any_slots(r)?.0;
            self.push_container(r, ty)?;
            (Some((r, i)), used, Some(j), 0)
        } else {
            (None, 0, None, self.ref_read(root)?)
        };
        loop {
            if let Some((r, i)) = at {
                if k == len {
                    return Ok(((r, i), after));
                }
                // A slot on the way lent to this same call is passed twice
                if let Some((_, l)) = self.heap.loan_after(r, i, after) {
                    let twice = l.depth == depth;
                    return Err(self.heap.aliased_slot(r, i as i64, twice).into());
                }
                v = self.heap.any_slot(r, i)?;
                // Out of the flat place of the root: every loan holding a slot refuses it
                after = None;
            }
            // The compiler checks the innermost dynamic array before a record that may be 0, so
            // a default container here is an empty dynamic array
            if v == 0 {
                return Err(RefError::Rt(rt(
                    exc::RANGE_ERROR,
                    dyn_index_message(self.task.regs[path + k], 0),
                )));
            }
            let ty = self.heap.agg_type(v)?;
            let n = self.heap.slot_count(v)?;
            self.push_container(v, ty)?;
            let (i, used) = self.path_slot(ty, n, path, k, len)?;
            at = Some((v, i));
            k += used;
        }
    }

    /// Adds a container walked by the loan being made.
    fn push_container(&mut self, block: heap::StrRef, ty: u32) -> Result<(), RefError> {
        self.heap.push_loan_container(LoanContainer {
            block: Some(block),
            ty,
            marked: false,
        })?;
        Ok(())
    }

    /// The slot of a container of type `ty` holding `n` slots that the steps from `k` of the
    /// path `r(path)..r(path+len-1)` select, and the number of steps taken: an element index of
    /// an array (range checked), followed in an array whose elements take more than one slot by
    /// the slot number in the element when a step is left; the slot number of a record or an
    /// instance. The element index is checked against the number of elements, as the slot
    /// number of a record is against the slots (`ERangeError`); a slot number in an element out
    /// of it is `Fatal` (only forged bytecode has one).
    fn path_slot(
        &self,
        ty: u32,
        n: usize,
        path: usize,
        k: usize,
        len: usize,
    ) -> Result<(usize, usize), RefError> {
        let step = self.task.regs[path + k];
        let entry = self.heap.type_entry(ty);
        let array =
            entry.is_some_and(|t| matches!(t.kind, bytecode::TYPE_ARRAY | bytecode::TYPE_DYNARRAY));
        let w = if array {
            entry.map_or(1, TypeEntry::elem_width)
        } else {
            1
        };
        let count = n / w;
        if !(0..count as i64).contains(&step) {
            return Err(RefError::Rt(rt(
                exc::RANGE_ERROR,
                dyn_index_message(step, count),
            )));
        }
        let base = step as usize * w;
        if w > 1 && k + 1 < len {
            let inner = self.task.regs[path + k + 1];
            if !(0..w as i64).contains(&inner) {
                return Err(RefError::Fatal(format!(
                    "slot {inner} of a lending path out of an element of {w} slots"
                )));
            }
            return Ok((base + inner as usize, 2));
        }
        Ok((base, 1))
    }

    /// The loan a reference slot of kind 4 refers to (by its position in the lending table),
    /// which lends a flat place. Anything else is `Fatal` (only forged bytecode makes one).
    pub(super) fn place_loan(&self, v: Slot) -> Result<(usize, Loan), RefError> {
        let (kind, j) = deref(v);
        match self.heap.loans().get(j) {
            Some(l) if kind == PLACE_REF && l.place.is_some() && !l.boxed => Ok((j, *l)),
            _ => Err(RefError::Fatal("bad reference slot".into())),
        }
    }

    /// The slot of the flat place `loan` lends that the first steps of the path
    /// `r(path)..r(path+len-1)` select, read as the steps into a block of the place's type are
    /// (a record's slot number; an element index of a static array, followed by the slot number
    /// in the element when its elements take more than one slot), so that a path reads the
    /// same whichever kind of reference slot its root is; and the number of steps taken.
    fn place_path_slot(
        &self,
        loan: &Loan,
        path: usize,
        len: usize,
    ) -> Result<((heap::StrRef, usize), usize), RefError> {
        let ty = loan.place.expect("a lent flat place has a type");
        let (i, used) = self.path_slot(ty, loan.width, path, 0, len)?;
        Ok((self.place_slot(loan, i as i64)?, used))
    }

    /// Slot `i` of the flat place `loan` lends: its block and slot number. Outside the place it is
    /// `Fatal`.
    pub(super) fn place_slot(
        &self,
        loan: &Loan,
        i: i64,
    ) -> Result<(heap::StrRef, usize), RefError> {
        let (r, start) = loan.loc.expect("a lent flat place always has slots");
        match usize::try_from(i) {
            Ok(i) if i < loan.width => Ok((r, start + i)),
            _ => Err(RefError::Fatal(format!(
                "slot {i} out of a flat place of {} slots",
                loan.width
            ))),
        }
    }

    /// The end of `Lend` / `LendPlace`: records `loan` (its containers pushed since
    /// `loan.containers`) and puts `value` in `rA`: the value taken out of the lent slot, or the
    /// reference to the lent flat place. On failure a value goes back to its slot.
    fn finish_lend(&mut self, ra: usize, loan: Loan, value: Slot) -> Result<(), RefError> {
        if let Err(e) = self.heap.push_loan(loan) {
            if loan.place.is_none() {
                let (r, i) = loan.loc.expect("a value is lent from a slot");
                *self.heap.any_slot_mut(r, i)? = value;
            }
            return Err(e.into());
        }
        // `rA` is 0 (the compiler clears it), so nothing is let go
        self.task.regs[ra] = value;
        if is_handle(loan.root) {
            self.heap.add_ref_obj(loan.root)?;
        }
        Ok(())
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
            .take_loans(self.task.frames.len(), &mut loans, &mut containers);
        let mut first = None;
        let mut fatal = None;
        for loan in &loans {
            let walked = &containers[loan.containers..loan.containers + loan.ncontainers];
            let result = if loan.boxed {
                let value = std::mem::take(&mut self.task.regs[loan.value]);
                self.put_box_back(loan, walked, value)
                    .map_err(|e| (e, value))
            } else if loan.place.is_some() {
                // The register holds the reference to the place (not a managed value)
                self.task.regs[loan.value] = 0;
                self.give_place_back(loan, walked)
            } else {
                let value = std::mem::take(&mut self.task.regs[loan.value]);
                self.put_back(loan, walked, value).map_err(|e| (e, value))
            };
            match result {
                Ok(()) => {}
                Err((RefError::Rt(e), value)) => {
                    // The value is let go (a managed one holds a reference)
                    let dropped = if loan.boxed {
                        self.heap.release(value)
                    } else if loan.place.is_some() {
                        self.drop_place(loan)
                    } else {
                        self.heap.release_kind(value, loan.kind)
                    };
                    if let Err(b) = dropped {
                        fatal.get_or_insert(b);
                    }
                    first.get_or_insert(e);
                }
                Err((RefError::Fatal(m), _)) => {
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
        let from = self.heap.loans_from(self.task.frames.len());
        let loans = &self.heap.loans()[from..];
        if loans.is_empty() {
            return false;
        }
        for loan in loans {
            let Some((r, i)) = loan.loc else {
                return false;
            };
            if loan.place.is_some() {
                // A flat place lent from an element of a dynamic array that the path still
                // leads to
                if loan.boxed
                    || loan.evacuated
                    || is_handle(loan.root)
                    || !(loan.len == 1 || loan.len == 2)
                    || self.fast_ref_read(loan.root) != Some(r)
                    || !self.heap.place_back_ready(
                        r,
                        i,
                        self.task.regs[loan.path],
                        (loan.len == 2).then(|| self.task.regs[loan.path + 1]),
                    )
                {
                    return false;
                }
                continue;
            }
            if loan.len != 1
                || is_handle(loan.root)
                || self.task.regs[loan.path] != i as Slot
                || self.fast_ref_read(loan.root) != Some(r)
                || !self.heap.give_back_ready(r, i)
            {
                return false;
            }
        }
        let regs = &mut self.task.regs;
        self.heap
            .give_back_fast(from, |loan| std::mem::take(&mut regs[loan.value]));
        true
    }

    /// Writes a lent value back to the slot the path leads to again ([`Isolate::find_again`]),
    /// letting go of the value that slot holds (0 unless the container was replaced or grown
    /// back meanwhile).
    fn put_back(
        &mut self,
        loan: &Loan,
        walked: &[LoanContainer],
        value: Slot,
    ) -> Result<(), RefError> {
        let (r, i) = self.find_again(loan, walked)?;
        let kind = self.heap.slot_kind(self.heap.any_slots(r)?.0, i);
        let old = std::mem::replace(self.heap.any_slot_mut(r, i)?, value);
        self.heap.release_kind(old, kind)?;
        Ok(())
    }

    /// Writes the slots of the block `value` that a flat place was taken out into (`PlaceBox`)
    /// back to the slots the path leads to again, letting go of what those held (0 unless the
    /// container was replaced or grown back meanwhile), then frees the block (0 is a default
    /// value: the slots get 0).
    fn put_box_back(
        &mut self,
        loan: &Loan,
        walked: &[LoanContainer],
        value: Slot,
    ) -> Result<(), RefError> {
        let (r, i) = self.find_again(loan, walked)?;
        let ty = self.heap.any_slots(r)?.0;
        let mut values = std::mem::take(&mut self.place_buf);
        values.clear();
        if value == 0 {
            values.resize(loan.width, 0);
        } else {
            let (bty, n) = {
                let (t, s) = self.heap.any_slots(value)?;
                values.extend_from_slice(s);
                (t, s.len())
            };
            if n != loan.width {
                return Err(RefError::Fatal(format!(
                    "block {value} of {n} slots given back to a flat place of {}",
                    loan.width
                )));
            }
            if self.heap.ref_count(value)? == 1 {
                // The register holds the only reference (the loan's block, or a copy the callee
                // made): its values move out, and the empty block goes
                self.heap.any_slots_mut(value)?.fill(0);
            } else {
                // The callee shared the block (copied the parameter to another variable): its
                // values are copied, the managed ones counted, and the register's reference goes
                for (k, v) in values.iter().enumerate() {
                    let kind = self.heap.slot_kind(bty, k);
                    self.heap.add_ref_kind(*v, kind)?;
                }
            }
        }
        let result = (|| {
            for (k, v) in values.iter().enumerate() {
                let kind = self.heap.slot_kind(ty, i + k);
                let old = std::mem::replace(self.heap.any_slot_mut(r, i + k)?, *v);
                self.heap.release_kind(old, kind)?;
            }
            self.heap.release(value)?;
            Ok(())
        })();
        self.place_buf = values;
        result
    }

    /// `PlaceBox A B` (`docs/spec/bytecode.md` 2.19): the flat place the reference slot `rB`
    /// (kind 4) lends to the current frame's call is taken out into a new block of its type
    /// in `rA` (counted by the loan, as a value `Lend` takes out), for a `var` open array
    /// parameter, which reads its argument as a block. The place stays lent; `Unlend` writes
    /// the block's slots back.
    pub(super) fn op_place_box(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let (j, loan) = self.place_loan(self.task.regs[rb])?;
        if loan.depth != self.task.frames.len() {
            return Err(RefError::Fatal(
                "a flat place lent by another frame taken out".into(),
            ));
        }
        let ty = loan.place.expect("a flat place has a type");
        let (r, start) = loan.loc.expect("a lent flat place always has slots");
        let values = self
            .heap
            .any_slots(r)?
            .1
            .get(start..start + loan.width)
            .ok_or_else(|| RefError::Fatal(format!("flat place out of range in block {r}")))?
            .to_vec();
        let b = match self.heap.alloc_agg(ty, values) {
            Ok(b) => b,
            Err(_) => return Err(RefError::Rt(rt(exc::OUT_OF_MEMORY, "out of memory"))),
        };
        // The values moved: the place keeps 0 while it is lent, as a value taken out does
        self.heap.any_slots_mut(r)?[start..start + loan.width].fill(0);
        self.heap.set_loan_boxed(j, ra);
        // `rA` is 0 (the compiler clears it), so nothing is let go
        self.task.regs[ra] = b as Slot;
        Ok(())
    }

    /// Ends the loan of a flat place: when the path leads to the same slots again, the callee
    /// wrote them in place and there is nothing to do; otherwise (the container was replaced, or
    /// its slots moved out when it went) the values move from where the loan held them to the
    /// slots the path leads to now, letting go of what those held. On failure the error comes
    /// with 0 (the values stay where the loan held them, for [`Isolate::drop_place`]).
    fn give_place_back(
        &mut self,
        loan: &Loan,
        walked: &[LoanContainer],
    ) -> Result<(), (RefError, Slot)> {
        let (r, i) = self.find_again(loan, walked).map_err(|e| (e, 0))?;
        let (from, start) = loan.loc.expect("a lent flat place always has slots");
        if !loan.evacuated && (from, start) == (r, i) {
            return Ok(());
        }
        self.move_place(loan, Some((r, i)))
            .map_err(|e| (e.into(), 0))
    }

    /// Lets go of the values of a flat place whose loan ends without giving them back.
    fn drop_place(&mut self, loan: &Loan) -> Result<(), BadRef> {
        self.move_place(loan, None)
    }

    /// Takes the values of the flat place `loan` holds out of its slots (leaving 0), and puts them
    /// in the slots from `to` (letting go of what those held), or lets go of them when `to` is
    /// `None`. A block the slots were moved to when their container went is freed.
    fn move_place(&mut self, loan: &Loan, to: Option<(heap::StrRef, usize)>) -> Result<(), BadRef> {
        let ty = loan.place.expect("a flat place has a type");
        let (from, start) = loan.loc.expect("a lent flat place always has slots");
        let mut values = std::mem::take(&mut self.place_buf);
        values.clear();
        {
            let slots = self.heap.any_slots_mut(from)?;
            let range = slots
                .get_mut(start..start + loan.width)
                .ok_or_else(|| BadRef {
                    message: format!("flat place out of range in heap block {from}"),
                })?;
            values.extend_from_slice(range);
            range.fill(0);
        }
        let result = (|| {
            for (k, v) in values.iter().enumerate() {
                let kind = self.heap.slot_kind(ty, k);
                match to {
                    Some((r, i)) => {
                        let old = std::mem::replace(self.heap.any_slot_mut(r, i + k)?, *v);
                        self.heap.release_kind(old, kind)?;
                    }
                    None => self.heap.release_kind(*v, kind)?,
                }
            }
            if loan.evacuated {
                self.heap.release(from)?;
            }
            Ok(())
        })();
        self.place_buf = values;
        result
    }

    /// Follows the path of `loan` again from its root (`Unlend`): records and static arrays after
    /// the innermost dynamic array are made unique (a default one becomes a block of the type
    /// `Lend` saw), dynamic arrays are written shared and their indices checked. Returns the
    /// slot reached.
    fn find_again(
        &mut self,
        loan: &Loan,
        walked: &[LoanContainer],
    ) -> Result<(heap::StrRef, usize), RefError> {
        let last_dyn = walked.iter().rposition(|c| self.heap.is_dyn_type(c.ty));
        let (path, len) = (loan.path, loan.len);
        // An instance's handle as the root: it is checked again, so a freed instance raises
        // `EAccessViolation` (`docs/spec/objects.md` 14.3)
        let (mut at, mut k, mut ci, mut v, mut parent) = if is_handle(loan.root) {
            let r = self.heap.obj_index(loan.root)? as heap::StrRef + 1;
            let (ty, n) = {
                let (t, s) = self.heap.any_slots(r)?;
                (t, s.len())
            };
            let (i, used) = self.path_slot(ty, n, path, 0, len)?;
            (Some((r, i)), used, 1, 0, None)
        } else if deref(loan.root).0 == PLACE_REF {
            let (_, outer) = self.place_loan(loan.root)?;
            let (at, used) = self.place_path_slot(&outer, path, len)?;
            (Some(at), used, 1, 0, None)
        } else {
            let parent = self.ref_target(loan.root)?;
            (None, 0, 0, self.loc_get(parent)?, Some(parent))
        };
        loop {
            if let Some((r, i)) = at {
                if k == len {
                    return Ok((r, i));
                }
                let loc = self.slot_loc(r, i);
                v = self.loc_get(loc)?;
                parent = Some(loc);
            }
            let c = walked
                .get(ci)
                .ok_or_else(|| RefError::Fatal("lending path longer than its containers".into()))?;
            if last_dyn.is_none_or(|d| ci > d) {
                let n = self.heap.unique(v, c.ty)??;
                if n != v {
                    // `unique` let go of the old value when it copied it
                    let parent = parent.expect("a container is read from a location");
                    self.loc_replace(parent, n)?;
                    v = n;
                }
            } else if v == 0 {
                if self.heap.is_dyn_type(c.ty) {
                    return Err(RefError::Rt(rt(
                        exc::RANGE_ERROR,
                        dyn_index_message(self.task.regs[path + k], 0),
                    )));
                }
                // A default record before a dynamic array: everything in it is default too, so
                // its steps lead to 0 again
                let n = self
                    .heap
                    .type_entry(c.ty)
                    .and_then(TypeEntry::flat_width)
                    .unwrap_or(0) as usize;
                let (_, used) = self.path_slot(c.ty, n, path, k, len)?;
                at = None;
                k += used;
                ci += 1;
                continue;
            }
            let ty = self.heap.agg_type(v)?;
            let n = self.heap.slot_count(v)?;
            let (i, used) = self.path_slot(ty, n, path, k, len)?;
            at = Some((v, i));
            k += used;
            ci += 1;
        }
    }

    /// The location of slot `i` of the record, array or instance block `r`.
    pub(super) fn slot_loc(&self, r: heap::StrRef, i: usize) -> Loc {
        if self.heap.is_obj_block(r) {
            Loc::ObjSlot(r as usize - 1, i)
        } else {
            Loc::Slot(r, i as i64)
        }
    }
}
