//! Heap: one slab of reference-counted blocks, and usage accounting (`docs/design/vm.md` 3.3, 3.11; requirements §5.5).

use crate::bytecode::{TYPE_DYNARRAY, TYPE_SET, TypeEntry};
use crate::sema::types::MAX_STRING_LEN;

/// String storage. One byte per character if every character is below U+0080, otherwise UTF-16 (§5.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PStr {
    Ascii(Box<[u8]>),
    Utf16(Box<[u16]>),
}

impl PStr {
    pub fn from_units(units: &[u16]) -> PStr {
        if units.iter().all(|u| *u < 0x80) {
            PStr::Ascii(units.iter().map(|u| *u as u8).collect())
        } else {
            PStr::Utf16(units.into())
        }
    }

    pub fn from_text(s: &str) -> PStr {
        if s.is_ascii() {
            PStr::Ascii(s.as_bytes().into())
        } else {
            PStr::Utf16(s.encode_utf16().collect())
        }
    }

    pub fn len(&self) -> usize {
        match self {
            PStr::Ascii(a) => a.len(),
            PStr::Utf16(u) => u.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The code unit at a 0-based index.
    pub fn unit(&self, i: usize) -> u16 {
        match self {
            PStr::Ascii(a) => a[i] as u16,
            PStr::Utf16(u) => u[i],
        }
    }

    pub fn units(&self) -> Vec<u16> {
        match self {
            PStr::Ascii(a) => a.iter().map(|b| *b as u16).collect(),
            PStr::Utf16(u) => u.to_vec(),
        }
    }

    /// Number of bytes (for usage accounting).
    pub fn byte_size(&self) -> usize {
        match self {
            PStr::Ascii(a) => a.len(),
            PStr::Utf16(u) => u.len() * 2,
        }
    }

    /// UTF-8 (lone surrogates become U+FFFD; §5.5).
    pub fn to_utf8(&self) -> String {
        match self {
            PStr::Ascii(a) => String::from_utf8_lossy(a).into_owned(),
            PStr::Utf16(u) => String::from_utf16_lossy(u),
        }
    }

    pub fn concat(&self, other: &PStr) -> PStr {
        match (self, other) {
            (PStr::Ascii(a), PStr::Ascii(b)) => {
                let mut v = Vec::with_capacity(a.len() + b.len());
                v.extend_from_slice(a);
                v.extend_from_slice(b);
                PStr::Ascii(v.into())
            }
            _ => {
                let mut v = self.units();
                v.extend(other.units());
                PStr::Utf16(v.into())
            }
        }
    }

    /// Content equality (independent of representation).
    pub fn equals(&self, other: &PStr) -> bool {
        match (self, other) {
            (PStr::Ascii(a), PStr::Ascii(b)) => a == b,
            (PStr::Utf16(a), PStr::Utf16(b)) => a == b,
            _ => {
                self.len() == other.len() && (0..self.len()).all(|i| self.unit(i) == other.unit(i))
            }
        }
    }

    /// Comparison in code point order (§5.11).
    pub fn compare(&self, other: &PStr) -> std::cmp::Ordering {
        match (self, other) {
            (PStr::Ascii(a), PStr::Ascii(b)) => a.cmp(b),
            _ => crate::sema::compare_strings(&self.units(), &other.units()),
        }
    }

    /// Replaces one element in place when the representation can hold it (an ASCII string keeps
    /// only ASCII). Returns `false` and leaves the string alone otherwise. `i` is 0-based.
    pub fn set_unit_in_place(&mut self, i: usize, c: u16) -> bool {
        match self {
            PStr::Ascii(a) if c < 0x80 => {
                a[i] = c as u8;
                true
            }
            PStr::Ascii(_) => false,
            PStr::Utf16(u) => {
                u[i] = c;
                true
            }
        }
    }

    /// A new value (copy) with one element of the string replaced. `i` is 0-based.
    pub fn with_unit(&self, i: usize, c: u16) -> PStr {
        match self {
            PStr::Ascii(a) if c < 0x80 => {
                let mut v = a.to_vec();
                v[i] = c as u8;
                PStr::Ascii(v.into())
            }
            _ => {
                let mut v = self.units();
                v[i] = c;
                PStr::Utf16(v.into())
            }
        }
    }

    pub fn slice(&self, start: usize, end: usize) -> PStr {
        match self {
            PStr::Ascii(a) => PStr::Ascii(a[start..end].into()),
            PStr::Utf16(u) => PStr::from_units(&u[start..end]),
        }
    }
}

/// A piece of a dynamic array being built ([`Heap::build_dyn`]).
#[derive(Debug, Clone, Copy)]
pub enum Piece {
    /// The elements `start..end` of an array block (0 only with an empty range).
    Range(StrRef, usize, usize),
    /// One element value.
    Elem(i64),
}

/// The number of 64-bit words of a big set of `bits` bits.
pub fn set_words(bits: u32) -> usize {
    bits.div_ceil(64) as usize
}

/// A reference to a heap block (slab index + 1; 0 is the type's default value, e.g. the empty string).
pub type StrRef = i64;

/// The contents of a block (`docs/design/lifetime.md` 3.2).
#[derive(Debug)]
enum Body {
    Str(PStr),
    /// A `UTF8String` (`docs/spec/types.md` 8.5): valid UTF-8 bytes.
    U8(Box<[u8]>),
    /// A record, static array or dynamic array: the type index (`docs/spec/bytecode.md` 3.2) and
    /// the slots.
    Agg {
        ty: u32,
        slots: Box<[i64]>,
    },
    /// An exception object (`docs/spec/exceptions.md` 6.1): the class's type index, the slots
    /// (`Message`, `InnerException`, the class's properties), the stack trace once raised, and the
    /// exceptions in `Suppressed` (each holding a reference).
    Exc {
        ty: u32,
        slots: Box<[i64]>,
        trace: Option<Box<Trace>>,
        suppressed: Vec<i64>,
    },
    /// A cell (`docs/design/lifetime.md` 3.7): the root's reference slot, or a dynamic array
    /// block (counted), and the path of (slot number, type of the container holding that slot).
    Cell {
        root: i64,
        path: Box<[(u32, u32)]>,
    },
}

impl Body {
    /// Bytes counted toward usage (§5.12).
    fn size(&self) -> usize {
        match self {
            Body::Str(s) => s.byte_size() + std::mem::size_of::<PStr>(),
            Body::U8(b) => b.len() + std::mem::size_of::<Box<[u8]>>(),
            Body::Agg { slots, .. } => slots.len() * 8 + std::mem::size_of::<Body>(),
            Body::Cell { path, .. } => path.len() * 8 + std::mem::size_of::<Body>(),
            Body::Exc {
                slots,
                trace,
                suppressed,
                ..
            } => {
                (slots.len() + suppressed.len()) * 8
                    + trace.as_ref().map_or(0, |t| t.frames.len() * 8)
                    + std::mem::size_of::<Body>()
            }
        }
    }
}

/// The stack trace of an exception object (`docs/spec/exceptions.md` 6.1): (function index, word
/// position) from the innermost frame, at most 32 (16 innermost + 16 outermost), and the number of
/// frames omitted between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace {
    pub frames: Vec<(u32, u32)>,
    pub omitted: usize,
}

/// The path of a cell: (slot number, type of the container holding that slot), from the root.
pub type CellPath = Vec<(u32, u32)>;

/// The kind of reference slot that is a cell (`docs/spec/bytecode.md` 1.5): the upper 8 bits of
/// a cell's heap reference.
pub const CELL_TAG: u64 = 3;

/// The heap reference of a value, without the tag of a cell.
fn untag(r: StrRef) -> StrRef {
    if (r as u64) >> 56 == CELL_TAG {
        (r as u64 & 0x00FF_FFFF_FFFF_FFFF) as StrRef
    } else {
        r
    }
}

/// Whether a value is a heap reference rather than a reference slot (whose upper 8 bits hold its
/// kind; `docs/spec/bytecode.md` 1.5) or 0.
pub fn is_block_ref(v: i64) -> bool {
    v != 0 && (v as u64) >> 56 == 0
}

/// One slot of the slab. `body` is `None` once the block has been freed.
#[derive(Debug)]
struct Block {
    rc: u32,
    body: Option<Body>,
}

/// The isolate's heap: one slab of reference-counted blocks (`docs/design/lifetime.md` 3.2).
pub struct Heap {
    blocks: Vec<Block>,
    /// The type entries by type index: the VM's built-in types (0–63), then the module's type
    /// section (which slots hold references, names for the leak report).
    types: Vec<TypeEntry>,
    /// Indexes of freed blocks available for reuse (empty and unused when reuse is off).
    free: Vec<usize>,
    /// Reuse the indexes of freed blocks. Off under leak checking so that releasing too often always
    /// hits a freed block (`docs/design/lifetime.md` 3.12).
    reuse: bool,
    used: usize,
    limit: Option<usize>,
    empty: PStr,
}

/// Allocation failure (`EOutOfMemory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

/// Use of a reference that does not point to a live block (an internal error, F0001; `docs/design/lifetime.md` 3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadRef {
    pub message: String,
}

impl Heap {
    /// A heap for a module with the given type section.
    pub fn new(limit: Option<usize>, reuse: bool, section: &[TypeEntry]) -> Self {
        let mut types = crate::bytecode::builtin_types().to_vec();
        types.extend_from_slice(section);
        Heap {
            blocks: Vec::new(),
            types,
            free: Vec::new(),
            reuse,
            used: 0,
            limit,
            empty: PStr::Ascii(Box::new([])),
        }
    }

    pub fn used(&self) -> usize {
        self.used
    }

    /// An upper bound on the code units (or `UTF8String` bytes) of a new string: the length
    /// limit and, under a usage limit, what is left of it at one byte per unit with the smaller
    /// block header (the smallest form), so a longer string is always refused by `alloc_str` /
    /// `alloc_u8` and checking this first only fails earlier. Instructions that take the length
    /// of their result from an argument check it before building the text, so that a length
    /// within the length limit but far over the isolate's limit fails without first allocating
    /// gigabytes in Rust.
    pub fn str_room(&self) -> usize {
        let Some(limit) = self.limit else {
            return MAX_STRING_LEN;
        };
        let header = std::mem::size_of::<PStr>().min(std::mem::size_of::<Box<[u8]>>());
        limit.saturating_sub(self.used + header).min(MAX_STRING_LEN)
    }

    /// Puts a string on the heap and returns a reference that holds its one reference (the empty
    /// string is 0 and takes no block). Checks the length limit and the usage limit.
    pub fn alloc_str(&mut self, s: PStr) -> Result<StrRef, OutOfMemory> {
        if s.is_empty() {
            return Ok(0);
        }
        if s.len() > MAX_STRING_LEN {
            return Err(OutOfMemory);
        }
        self.alloc(Body::Str(s))
    }

    /// Puts a `UTF8String` on the heap (the empty one is 0). Checks the length limit (in bytes,
    /// the same limit as a `string`'s) and the usage limit.
    pub fn alloc_u8(&mut self, b: Vec<u8>) -> Result<StrRef, OutOfMemory> {
        if b.is_empty() {
            return Ok(0);
        }
        if b.len() > MAX_STRING_LEN {
            return Err(OutOfMemory);
        }
        self.alloc(Body::U8(b.into_boxed_slice()))
    }

    fn alloc(&mut self, body: Body) -> Result<StrRef, OutOfMemory> {
        if let Some(limit) = self.limit
            && self.used + body.size() > limit
        {
            return Err(OutOfMemory);
        }
        Ok(self.place(body))
    }

    /// Puts a block in the slab without checking the usage limit (the usage still counts it).
    fn place(&mut self, body: Body) -> StrRef {
        self.used += body.size();
        let block = Block {
            rc: 1,
            body: Some(body),
        };
        let index = match self.free.pop() {
            Some(i) => {
                self.blocks[i] = block;
                i
            }
            None => {
                self.blocks.push(block);
                self.blocks.len() - 1
            }
        };
        index as StrRef + 1
    }

    /// The live block a non-zero reference points to.
    fn block_mut(&mut self, r: StrRef, what: &str) -> Result<&mut Block, BadRef> {
        let r = untag(r);
        let index = usize::try_from(r - 1)
            .ok()
            .filter(|i| *i < self.blocks.len());
        let Some(index) = index else {
            return Err(BadRef {
                message: format!("{what} of an invalid heap reference {r}"),
            });
        };
        let block = &mut self.blocks[index];
        if block.body.is_none() {
            return Err(BadRef {
                message: format!("{what} of a freed heap block {r}"),
            });
        }
        Ok(block)
    }

    /// The string a reference points to (0 is empty). A reference outside the slab reads as empty
    /// (a mistyped register gives a wrong value, as in M1; `docs/design/vm.md` 3.12); a freed block is an error.
    pub fn get(&self, r: StrRef) -> Result<&PStr, BadRef> {
        let r = untag(r);
        let Some(block) = usize::try_from(r - 1).ok().and_then(|i| self.blocks.get(i)) else {
            return Ok(&self.empty);
        };
        match &block.body {
            Some(Body::Str(s)) => Ok(s),
            // A mistyped register gives a wrong value (`docs/design/vm.md` 3.12)
            Some(_) => Ok(&self.empty),
            None => Err(BadRef {
                message: format!("read of a freed heap block {r}"),
            }),
        }
    }

    /// The bytes of a `UTF8String` (0 is empty). Like [`Heap::get`], a reference to another kind
    /// of block reads as empty; a freed block is an error.
    pub fn get_u8(&self, r: StrRef) -> Result<&[u8], BadRef> {
        let r = untag(r);
        let Some(block) = usize::try_from(r - 1).ok().and_then(|i| self.blocks.get(i)) else {
            return Ok(&[]);
        };
        match &block.body {
            Some(Body::U8(b)) => Ok(b),
            Some(_) => Ok(&[]),
            None => Err(BadRef {
                message: format!("read of a freed heap block {r}"),
            }),
        }
    }

    /// The live block a non-zero reference points to (for reading).
    fn block(&self, r: StrRef, what: &str) -> Result<&Block, BadRef> {
        let r = untag(r);
        let block = usize::try_from(r - 1)
            .ok()
            .and_then(|i| self.blocks.get(i))
            .ok_or_else(|| BadRef {
                message: format!("{what} of an invalid heap reference {r}"),
            })?;
        if block.body.is_none() {
            return Err(BadRef {
                message: format!("{what} of a freed heap block {r}"),
            });
        }
        Ok(block)
    }

    fn bad(what: &str, r: StrRef) -> BadRef {
        BadRef {
            message: format!("{what} of heap block {r}, which is not a record or array"),
        }
    }

    /// The type entry of a type index.
    pub fn type_entry(&self, ty: u32) -> Option<&TypeEntry> {
        self.types.get(ty as usize)
    }

    /// A block of type `ty` with every slot at its default value (0), holding one reference.
    pub fn alloc_default(&mut self, ty: u32) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        let entry = self.type_entry(ty).ok_or_else(|| BadRef {
            message: format!("type {ty} out of range"),
        })?;
        // A big set holds its bits in words of 64 (`docs/spec/types.md` 8.3)
        let n = if entry.kind == TYPE_SET {
            set_words(entry.slots)
        } else {
            entry.slots as usize
        };
        if let Some(limit) = self.limit
            && self.used + n * 8 > limit
        {
            return Ok(Err(OutOfMemory));
        }
        let mut slots = Vec::new();
        if slots.try_reserve_exact(n).is_err() {
            return Ok(Err(OutOfMemory));
        }
        slots.resize(n, 0);
        Ok(self.alloc(Body::Agg {
            ty,
            slots: slots.into_boxed_slice(),
        }))
    }

    /// A big set block of type `ty` with the given bit words (padded to the type's size).
    pub fn alloc_set(
        &mut self,
        ty: u32,
        words: &[u64],
    ) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        let n = match self.type_entry(ty) {
            Some(t) if t.kind == TYPE_SET => set_words(t.slots),
            _ => {
                return Err(BadRef {
                    message: format!("type {ty} is not a set"),
                });
            }
        };
        if let Some(limit) = self.limit
            && self.used + n * 8 > limit
        {
            return Ok(Err(OutOfMemory));
        }
        let mut slots = vec![0i64; n];
        for (s, w) in slots.iter_mut().zip(words) {
            *s = *w as i64;
        }
        Ok(self.alloc_agg(ty, slots))
    }

    /// The bit words of a big set (empty for 0, the empty set).
    pub fn set_slots(&self, r: StrRef) -> Result<&[i64], BadRef> {
        if r == 0 {
            return Ok(&[]);
        }
        self.agg(r, "read").map(|(_, s)| s)
    }

    /// The bit words of a unique big set block for writing (made unique by `Unique`).
    pub fn set_slots_mut(&mut self, r: StrRef) -> Result<&mut [i64], BadRef> {
        let block = self.block_mut(r, "write")?;
        if block.rc != 1 {
            return Err(BadRef {
                message: format!("write to the shared heap block {r}"),
            });
        }
        match &mut block.body {
            Some(Body::Agg { slots, .. }) => Ok(slots),
            _ => Err(Self::bad("write", r)),
        }
    }

    /// A block holding the given slots (each a value the block now owns), holding one reference.
    pub fn alloc_agg(&mut self, ty: u32, slots: Vec<i64>) -> Result<StrRef, OutOfMemory> {
        self.alloc(Body::Agg {
            ty,
            slots: slots.into_boxed_slice(),
        })
    }

    /// The type and slots of a record or static array block.
    fn agg(&self, r: StrRef, what: &str) -> Result<(u32, &[i64]), BadRef> {
        match &self.block(r, what)?.body {
            Some(Body::Agg { ty, slots }) => Ok((*ty, slots)),
            _ => Err(Self::bad(what, r)),
        }
    }

    /// The type of a record or static array block.
    pub fn agg_type(&self, r: StrRef) -> Result<u32, BadRef> {
        self.agg(r, "read").map(|(t, _)| t)
    }

    /// Slot `i` of a record or static array (`None` if `i` is outside the block).
    pub fn slot(&self, r: StrRef, i: i64) -> Result<Option<i64>, BadRef> {
        let (_, slots) = self.agg(r, "read")?;
        Ok(usize::try_from(i).ok().and_then(|i| slots.get(i)).copied())
    }

    /// The number of slots of a record or static array.
    pub fn slot_count(&self, r: StrRef) -> Result<usize, BadRef> {
        self.agg(r, "read").map(|(_, s)| s.len())
    }

    /// Whether a type is a dynamic array.
    pub fn is_dyn_type(&self, ty: u32) -> bool {
        self.types
            .get(ty as usize)
            .is_some_and(|t| t.kind == TYPE_DYNARRAY)
    }

    /// Slot `i` of a record, static array or dynamic array for writing. A record or static array
    /// must hold its only reference (made unique by `Unique`); anything else is an internal error.
    /// A dynamic array is written while shared (`docs/spec/types.md` 2.3).
    pub fn slot_mut(&mut self, r: StrRef, i: i64) -> Result<&mut i64, BadRef> {
        let ty = self.agg(r, "write")?.0;
        let shared_ok = self.is_dyn_type(ty);
        let block = self.block_mut(r, "write")?;
        if block.rc != 1 && !shared_ok {
            return Err(BadRef {
                message: format!("write to the shared heap block {r}"),
            });
        }
        match &mut block.body {
            Some(Body::Agg { slots, .. }) => usize::try_from(i)
                .ok()
                .and_then(|i| slots.get_mut(i))
                .ok_or_else(|| BadRef {
                    message: format!("slot {i} out of range in heap block {r}"),
                }),
            _ => Err(Self::bad("write", r)),
        }
    }

    /// Whether slot `i` of a block of type `ty` holds a heap reference.
    fn slot_is_ref(&self, ty: u32, i: usize) -> bool {
        self.types
            .get(ty as usize)
            .is_some_and(|t| t.slot_is_ref(i))
    }

    /// Makes a record or static array value unique for writing (`Unique`; `docs/design/lifetime.md`
    /// 3.6): 0 becomes a new block of type `ty` with default slots, a shared block is copied (the
    /// copy counts the references in its slots, the original loses one reference). Returns the
    /// value that now takes the place of `r`.
    pub fn unique(&mut self, r: StrRef, ty: u32) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        if r == 0 {
            // An empty dynamic array (a value open array parameter) stays empty
            if self.is_dyn_type(ty) {
                return Ok(Ok(0));
            }
            return self.alloc_default(ty);
        }
        let rc = self.block(r, "write")?.rc;
        if rc == 1 {
            return Ok(Ok(r));
        }
        let (bty, slots) = self.agg(r, "copy")?;
        let slots = slots.to_vec();
        if let Some(limit) = self.limit
            && self.used + slots.len() * 8 > limit
        {
            return Ok(Err(OutOfMemory));
        }
        for (i, v) in slots.iter().enumerate() {
            if self.slot_is_ref(bty, i) {
                self.add_ref(*v)?;
            }
        }
        let copy = match self.alloc_agg(bty, slots.clone()) {
            Ok(c) => c,
            Err(e) => {
                for (i, v) in slots.iter().enumerate() {
                    if self.slot_is_ref(bty, i) {
                        self.release(*v)?;
                    }
                }
                return Ok(Err(e));
            }
        };
        self.release(r)?;
        Ok(Ok(copy))
    }

    /// Makes a record or static array value unique (`UniqueVal`; a `var` open array parameter):
    /// like `unique` with the block's own type, while 0 and a dynamic array are left alone.
    pub fn unique_val(&mut self, r: StrRef) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        if r == 0 {
            return Ok(Ok(0));
        }
        let ty = self.agg(r, "write")?.0;
        if self.is_dyn_type(ty) {
            return Ok(Ok(r));
        }
        self.unique(r, ty)
    }

    /// The number of elements of an array block (0 is an empty dynamic array).
    pub fn len(&self, r: StrRef) -> Result<usize, BadRef> {
        if r == 0 {
            return Ok(0);
        }
        self.slot_count(r)
    }

    /// A new dynamic array of type `ty` with `n` default elements, holding one reference (0 for
    /// `n == 0`).
    pub fn alloc_dyn(&mut self, ty: u32, n: usize) -> Result<StrRef, OutOfMemory> {
        if n == 0 {
            return Ok(0);
        }
        if n > MAX_STRING_LEN {
            return Err(OutOfMemory);
        }
        self.alloc_slots(ty, vec![], n)
    }

    /// A block of type `ty` with the given slots, padded with defaults to `n` (each given slot is
    /// a value the block now owns). Checks the usage limit before allocating.
    fn alloc_slots(
        &mut self,
        ty: u32,
        mut slots: Vec<i64>,
        n: usize,
    ) -> Result<StrRef, OutOfMemory> {
        if let Some(limit) = self.limit
            && self.used + n * 8 > limit
        {
            return Err(OutOfMemory);
        }
        if slots.try_reserve_exact(n - slots.len()).is_err() {
            return Err(OutOfMemory);
        }
        slots.resize(n, 0);
        self.alloc_agg(ty, slots)
    }

    /// `SetLength` of a dynamic array of type `ty` (`docs/spec/types.md` 2.3): returns the value that
    /// takes the place of `r`. A block holding its only reference is resized in place (dropped
    /// elements are released); a shared one is copied (the copy counts its elements, the original
    /// loses one reference). The length 0 is the empty array (0).
    pub fn set_len(
        &mut self,
        r: StrRef,
        ty: u32,
        n: usize,
    ) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        if n > MAX_STRING_LEN {
            return Ok(Err(OutOfMemory));
        }
        if r == 0 {
            return Ok(self.alloc_dyn(ty, n));
        }
        if n == 0 {
            self.release(r)?;
            return Ok(Ok(0));
        }
        let (bty, old) = {
            let (t, s) = self.agg(r, "resize")?;
            (t, s.len())
        };
        let refs = self
            .types
            .get(bty as usize)
            .is_some_and(|t| t.slot_is_ref(0));
        if self.block(r, "resize")?.rc == 1 {
            if n > old
                && let Some(limit) = self.limit
                && self.used + (n - old) * 8 > limit
            {
                return Ok(Err(OutOfMemory));
            }
            let dropped: Option<Vec<i64>> = {
                let block = self.block_mut(r, "resize")?;
                let Some(Body::Agg { slots, .. }) = &mut block.body else {
                    return Err(Self::bad("resize", r));
                };
                let mut v = std::mem::take(slots).into_vec();
                let dropped = if n < old { v.split_off(n) } else { Vec::new() };
                let grown = v.try_reserve_exact(n.saturating_sub(v.len())).is_ok();
                if grown {
                    v.resize(n, 0);
                }
                *slots = v.into_boxed_slice();
                grown.then_some(dropped)
            };
            let Some(dropped) = dropped else {
                return Ok(Err(OutOfMemory));
            };
            if n > old {
                self.used += (n - old) * 8;
            } else {
                self.used -= (old - n) * 8;
            }
            if refs {
                for v in dropped {
                    self.release(v)?;
                }
            }
            return Ok(Ok(r));
        }
        let kept: Vec<i64> = self.agg(r, "copy")?.1[..old.min(n)].to_vec();
        if refs {
            for v in &kept {
                self.add_ref(*v)?;
            }
        }
        match self.alloc_slots(bty, kept.clone(), n) {
            Ok(copy) => {
                self.release(r)?;
                Ok(Ok(copy))
            }
            Err(e) => {
                if refs {
                    for v in kept {
                        self.release(v)?;
                    }
                }
                Ok(Err(e))
            }
        }
    }

    /// `Copy` of an array (`docs/spec/types.md` chapter 7): a new dynamic array of type `ty` with
    /// the elements of `r` from `index`, at most `count` of them (the range is cut to the array;
    /// the result may be empty, 0). `r` is a dynamic or static array block, or 0.
    pub fn copy_range(
        &mut self,
        r: StrRef,
        ty: u32,
        index: i64,
        count: i64,
    ) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        let len = self.len(r)? as i64;
        let start = index.clamp(0, len);
        let end = start.saturating_add(count.max(0)).min(len);
        if end <= start {
            return Ok(Ok(0));
        }
        self.build_dyn(ty, &[Piece::Range(r, start as usize, end as usize)])
    }

    /// A new dynamic array of type `ty` made of the given pieces in order (`ArrConcat`,
    /// `ArrInsert`, `ArrDelete`; `docs/spec/types.md` 8.6): the elements are counted, the pieces
    /// themselves are only borrowed. An empty result is 0. A length above 2^31 - 1 is
    /// `OutOfMemory`, decided before anything is allocated.
    pub fn build_dyn(
        &mut self,
        ty: u32,
        pieces: &[Piece],
    ) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        let mut n: usize = 0;
        for p in pieces {
            n = n.saturating_add(match *p {
                Piece::Range(_, start, end) => end - start,
                Piece::Elem(_) => 1,
            });
        }
        if n > MAX_STRING_LEN {
            return Ok(Err(OutOfMemory));
        }
        if n == 0 {
            return Ok(Ok(0));
        }
        let mut items: Vec<i64> = Vec::new();
        if items.try_reserve_exact(n).is_err() {
            return Ok(Err(OutOfMemory));
        }
        for p in pieces {
            match *p {
                Piece::Range(r, start, end) if end > start => {
                    items.extend_from_slice(&self.agg(r, "copy")?.1[start..end]);
                }
                Piece::Range(..) => {}
                Piece::Elem(v) => items.push(v),
            }
        }
        // The usage limit is checked before the elements are counted, so a failure has nothing
        // to give back
        let body = Body::Agg {
            ty,
            slots: items.into_boxed_slice(),
        };
        if let Some(limit) = self.limit
            && self.used + body.size() > limit
        {
            return Ok(Err(OutOfMemory));
        }
        let refs = self
            .types
            .get(ty as usize)
            .is_some_and(|t| t.slot_is_ref(0));
        if refs && let Body::Agg { slots, .. } = &body {
            for v in slots.iter() {
                self.add_ref(*v)?;
            }
        }
        Ok(Ok(self.place(body)))
    }

    /// A cell with one reference (`MakeCell`), tagged as a reference slot of kind 3.
    pub fn alloc_cell(&mut self, root: i64, path: CellPath) -> Result<StrRef, OutOfMemory> {
        let r = self.alloc(Body::Cell {
            root,
            path: path.into_boxed_slice(),
        })?;
        Ok(((CELL_TAG << 56) | r as u64) as StrRef)
    }

    /// The root and path of a cell.
    pub fn cell(&self, r: StrRef) -> Result<(i64, CellPath), BadRef> {
        match &self.block(r, "read")?.body {
            Some(Body::Cell { root, path }) => Ok((*root, path.to_vec())),
            _ => Err(BadRef {
                message: format!("heap block {r} is not a cell"),
            }),
        }
    }

    /// The string a reference points to, for writing in place, when the reference is its only one
    /// (`None` for 0 and for a shared string; `StrSetIndex`, `docs/spec/bytecode.md` 2.4).
    pub fn unique_str_mut(&mut self, r: StrRef) -> Result<Option<&mut PStr>, BadRef> {
        if r == 0 {
            return Ok(None);
        }
        let block = self.block_mut(r, "write")?;
        match (block.rc, &mut block.body) {
            (1, Some(Body::Str(s))) => Ok(Some(s)),
            _ => Ok(None),
        }
    }

    /// Adds one reference (0 is the default value and holds nothing).
    pub fn add_ref(&mut self, r: StrRef) -> Result<(), BadRef> {
        if r == 0 {
            return Ok(());
        }
        let block = self.block_mut(r, "add_ref")?;
        block.rc += 1;
        Ok(())
    }

    /// Drops one reference and frees the block when it was the last one (0 is the default value and holds nothing).
    /// The references held by a freed record or array are dropped through a work list, not by
    /// recursion, so deep nesting does not use up the Rust stack (`docs/design/lifetime.md` 3.2).
    pub fn release(&mut self, r: StrRef) -> Result<(), BadRef> {
        let mut work = vec![r];
        while let Some(r) = work.pop() {
            if r == 0 {
                continue;
            }
            let r = untag(r);
            let block = self.block_mut(r, "release")?;
            block.rc -= 1;
            if block.rc > 0 {
                continue;
            }
            let body = block.body.take().expect("block_mut returns live blocks");
            self.used -= body.size();
            if self.reuse {
                self.free.push((r - 1) as usize);
            }
            match &body {
                Body::Exc {
                    ty,
                    slots,
                    suppressed,
                    ..
                } => {
                    for v in suppressed.iter().rev() {
                        work.push(*v);
                    }
                    for (i, v) in slots.iter().enumerate().rev() {
                        if *v != 0 && self.slot_is_ref(*ty, i) {
                            work.push(*v);
                        }
                    }
                }
                Body::Agg { ty, slots } => {
                    // Released in slot order (`docs/design/lifetime.md` 3.2): the work list is a stack
                    for (i, v) in slots.iter().enumerate().rev() {
                        if *v != 0 && self.slot_is_ref(*ty, i) {
                            work.push(*v);
                        }
                    }
                }
                // A cell whose root is a dynamic array block holds a reference of it
                Body::Cell { root, .. } if is_block_ref(*root) => work.push(*root),
                _ => {}
            }
        }
        Ok(())
    }

    /// Type names of the blocks still on the heap (the leak check; `docs/spec/diagnostics.md` 8.1).
    pub fn live_type_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.blocks.iter().filter_map(|b| match b.body.as_ref()? {
            Body::Str(_) => Some("string"),
            Body::U8(_) => Some("UTF8String"),
            Body::Agg { ty, .. } | Body::Exc { ty, .. } => Some(
                self.types
                    .get(*ty as usize)
                    .map_or("<unknown>", |t| t.name.as_str()),
            ),
            Body::Cell { .. } => Some("<cell>"),
        })
    }

    /// A new exception object of class `ty` with `Message` set to `msg` (a string the object now
    /// owns) and the other slots at their default values, holding one reference. `force` skips the
    /// usage limit (`EOutOfMemory`, `EStackOverflow`; `docs/spec/exceptions.md` 5.5).
    pub fn alloc_exc(&mut self, ty: u32, msg: i64, force: bool) -> Result<StrRef, OutOfMemory> {
        let n = self.types.get(ty as usize).map_or(2, |t| t.slots as usize);
        let mut slots = vec![0; n.max(1)];
        slots[crate::bytecode::EXC_SLOT_MESSAGE as usize] = msg;
        let body = Body::Exc {
            ty,
            slots: slots.into_boxed_slice(),
            trace: None,
            suppressed: Vec::new(),
        };
        if force {
            Ok(self.place(body))
        } else {
            self.alloc(body)
        }
    }

    /// A string block, skipping the usage limit when `force` (the message of a VM exception).
    pub fn alloc_str_force(&mut self, s: PStr) -> StrRef {
        if s.is_empty() {
            return 0;
        }
        self.place(Body::Str(s))
    }

    fn exc(&self, r: StrRef, what: &str) -> Result<&Body, BadRef> {
        match &self.block(r, what)?.body {
            Some(b @ Body::Exc { .. }) => Ok(b),
            _ => Err(BadRef {
                message: format!("{what} of heap block {r}, which is not an exception"),
            }),
        }
    }

    /// The class (type index) of an exception object.
    pub fn exc_type(&self, r: StrRef) -> Result<u32, BadRef> {
        match self.exc(r, "read")? {
            Body::Exc { ty, .. } => Ok(*ty),
            _ => unreachable!("exc returns exception blocks"),
        }
    }

    /// Slot `i` of an exception object (`None` outside the object).
    pub fn exc_slot(&self, r: StrRef, i: u32) -> Result<Option<i64>, BadRef> {
        match self.exc(r, "read")? {
            Body::Exc { slots, .. } => Ok(slots.get(i as usize).copied()),
            _ => unreachable!("exc returns exception blocks"),
        }
    }

    /// Replaces slot `i` of an exception object (the object is a reference type, written while
    /// shared) and returns the old value. `None` outside the object.
    pub fn exc_replace(&mut self, r: StrRef, i: u32, v: i64) -> Result<Option<i64>, BadRef> {
        let block = self.block_mut(r, "write")?;
        match &mut block.body {
            Some(Body::Exc { slots, .. }) => {
                Ok(slots.get_mut(i as usize).map(|s| std::mem::replace(s, v)))
            }
            _ => Err(BadRef {
                message: format!("write of heap block {r}, which is not an exception"),
            }),
        }
    }

    /// The stack trace of an exception object (`None` before it is raised).
    pub fn exc_trace(&self, r: StrRef) -> Result<Option<&Trace>, BadRef> {
        match self.exc(r, "read")? {
            Body::Exc { trace, .. } => Ok(trace.as_deref()),
            _ => unreachable!("exc returns exception blocks"),
        }
    }

    /// Records the stack trace of an exception object (counted toward usage without the limit,
    /// like the object itself when it was raised by the VM).
    pub fn set_exc_trace(&mut self, r: StrRef, t: Trace) -> Result<(), BadRef> {
        let add = t.frames.len() * 8;
        let block = self.block_mut(r, "write")?;
        match &mut block.body {
            Some(Body::Exc { trace, .. }) => *trace = Some(Box::new(t)),
            _ => {
                return Err(BadRef {
                    message: format!("write of heap block {r}, which is not an exception"),
                });
            }
        }
        self.used += add;
        Ok(())
    }

    /// The exceptions in `Suppressed` of an exception object.
    pub fn exc_suppressed(&self, r: StrRef) -> Result<&[i64], BadRef> {
        match self.exc(r, "read")? {
            Body::Exc { suppressed, .. } => Ok(suppressed),
            _ => unreachable!("exc returns exception blocks"),
        }
    }

    /// Appends `v` (whose reference the object takes) to `Suppressed` of an exception object
    /// (counted toward usage without the limit, like the trace).
    pub fn exc_push_suppressed(&mut self, r: StrRef, v: i64) -> Result<(), BadRef> {
        let block = self.block_mut(r, "write")?;
        match &mut block.body {
            Some(Body::Exc { suppressed, .. }) => suppressed.push(v),
            _ => {
                return Err(BadRef {
                    message: format!("write of heap block {r}, which is not an exception"),
                });
            }
        }
        self.used += 8;
        Ok(())
    }

    /// Whether `target` is `from`, or is reached from it through `Suppressed` and `InnerException`
    /// (attaching `from` to `target` would make a cycle; `docs/spec/exceptions.md` 5.4).
    pub fn exc_reaches(&self, from: StrRef, target: StrRef) -> Result<bool, BadRef> {
        let mut work = vec![from];
        let mut seen = std::collections::HashSet::new();
        while let Some(x) = work.pop() {
            if x == target {
                return Ok(true);
            }
            if x == 0 || !seen.insert(x) {
                continue;
            }
            work.extend_from_slice(self.exc_suppressed(x)?);
            if let Some(inner) = self.exc_slot(x, crate::bytecode::EXC_SLOT_INNER)? {
                work.push(inner);
            }
        }
        Ok(false)
    }

    /// Whether exception class `ty` is `ancestor` or a descendant of it.
    pub fn class_is(&self, ty: u32, ancestor: u32) -> bool {
        let mut cur = Some(ty);
        let mut steps = 0;
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            steps += 1;
            if steps > self.types.len() {
                return false;
            }
            cur = self.types.get(c as usize).and_then(|t| t.parent);
        }
        false
    }

    /// The name of a type (the class name of an exception, by its type entry).
    pub fn type_name(&self, ty: u32) -> &str {
        self.types
            .get(ty as usize)
            .map_or("<unknown>", |t| t.name.as_str())
    }

    /// Counts growth of `regs` and the like toward usage.
    pub fn account(&mut self, bytes: usize) -> Result<(), OutOfMemory> {
        if let Some(limit) = self.limit
            && self.used + bytes > limit
        {
            return Err(OutOfMemory);
        }
        self.used += bytes;
        Ok(())
    }

    /// A string block with a reference count of 1 (tests that build blocks directly).
    #[cfg(test)]
    pub(crate) fn alloc_test_str(&mut self, s: &str) -> StrRef {
        self.alloc_str(PStr::from_text(s))
            .expect("test strings fit the limit")
    }
}
