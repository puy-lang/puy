//! Heap: one slab of reference-counted blocks, and usage accounting (`docs/design/vm.md` 3.3, 3.11; requirements §5.5).

use super::hash::HashKeys;
use super::sort;
use crate::bytecode::{
    SLOT_OBJ, SLOT_PLAIN, SLOT_REF, TYPE_ARRAY, TYPE_DYNARRAY, TYPE_SET, TypeEntry,
};
use crate::sema::types::MAX_STRING_LEN;
use std::any::Any;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::rc::Rc;

/// String storage. One byte per character if every character is below U+0080, otherwise UTF-16 (§5.5).
///
/// A string block can hold more capacity than its length, so that appending to the only reference
/// grows it in place (`AppendStr`; `docs/spec/types.md` 9.1). A one-byte string of at most
/// [`SHORT_LEN`] characters is kept in the value itself ([`PStr::Short`]), so that making and
/// freeing it allocates nothing besides its block (`docs/design/vm.md` 3.3).
#[derive(Debug, Clone)]
pub enum PStr {
    Ascii(Vec<u8>),
    Utf16(Vec<u16>),
    Short(ShortStr),
}

/// The most characters of a one-byte string kept in place: what fits beside the length without
/// making a [`PStr`] (and so a block) larger than one with a buffer.
pub const SHORT_LEN: usize = 30;

/// A one-byte string of at most [`SHORT_LEN`] characters, in place.
#[derive(Debug, Clone, Copy)]
pub struct ShortStr {
    len: u8,
    bytes: [u8; SHORT_LEN],
}

impl ShortStr {
    fn len(&self) -> usize {
        usize::from(self.len)
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len()]
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        let n = self.len();
        &mut self.bytes[..n]
    }

    /// Whether `n` more characters fit in place.
    fn fits(&self, n: usize) -> bool {
        self.len() + n <= SHORT_LEN
    }

    /// Appends `b`, which [`ShortStr::fits`].
    fn push(&mut self, b: &[u8]) {
        let n = self.len();
        self.bytes[n..n + b.len()].copy_from_slice(b);
        self.len = u8::try_from(n + b.len()).expect("SHORT_LEN fits in a u8");
    }
}

/// The elements of a dynamic array of one-slot elements or of bytes, borrowed as they are held.
#[derive(Clone, Copy)]
pub enum Elems<'a> {
    Slots(&'a [i64]),
    Bytes(&'a [u8]),
}

impl Elems<'_> {
    pub fn len(&self) -> usize {
        match self {
            Elems::Slots(s) => s.len(),
            Elems::Bytes(b) => b.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Element `i` (`None` outside the array).
    pub fn get(&self, i: i64) -> Option<i64> {
        let i = usize::try_from(i).ok()?;
        match self {
            Elems::Slots(s) => s.get(i).copied(),
            Elems::Bytes(b) => b.get(i).map(|v| i64::from(*v)),
        }
    }
}

/// The code units of a [`PStr`], borrowed in either representation.
#[derive(Clone, Copy)]
enum View<'a> {
    Ascii(&'a [u8]),
    Utf16(&'a [u16]),
}

/// Reserves room for exactly `additional` more elements of `v`, or `OutOfMemory` when the system
/// allocator refuses. The one place where the VM allocates fallibly: every buffer whose size a
/// Pascal value decides (the text of a string, the slots of an array, the copies an instruction
/// works on) is reserved here before it is filled, so that the refusal becomes `EOutOfMemory`
/// (§5.12; `docs/spec/semantics.md` 3.5, 7.1). The reservation runs under the mark of
/// `puy_host::alloc`, so that the runtime's allocator wrapper does not take the refusal for the
/// failure of an allocation inside Rust (F0003).
pub fn try_reserve<T>(v: &mut Vec<T>, additional: usize) -> Result<(), OutOfMemory> {
    let before = v.capacity();
    puy_host::alloc::fallible(|| v.try_reserve_exact(additional)).map_err(|_| OutOfMemory)?;
    #[cfg(test)]
    if v.capacity() != before {
        reserved::add(v.capacity());
    }
    #[cfg(not(test))]
    let _ = before;
    Ok(())
}

/// The number of elements of every buffer [`try_reserve`] allocated or grew on this thread (its
/// new capacity each time), for the tests of the complexity guarantees (`docs/spec/types.md` 9.1):
/// growing a buffer by a constant each time adds up to O(n²), doubling it to O(n).
#[cfg(test)]
pub mod reserved {
    use std::cell::Cell;

    thread_local! {
        static TOTAL: Cell<u64> = const { Cell::new(0) };
    }

    pub(super) fn add(n: usize) {
        TOTAL.with(|t| t.set(t.get() + n as u64));
    }

    /// The total so far.
    pub fn total() -> u64 {
        TOTAL.with(Cell::get)
    }
}

/// Appends `items` to a buffer being built, growing it by at least double through
/// [`try_reserve`] (so that building piece by piece stays linear).
pub fn try_extend<T: Copy>(v: &mut Vec<T>, items: &[T]) -> Result<(), OutOfMemory> {
    grow(v, items.len())?;
    v.extend_from_slice(items);
    Ok(())
}

/// Appends one element ([`try_extend`]).
pub fn try_push<T>(v: &mut Vec<T>, item: T) -> Result<(), OutOfMemory> {
    grow(v, 1)?;
    v.push(item);
    Ok(())
}

/// Appends `n` copies of `item` ([`try_extend`]).
pub fn try_fill<T: Copy>(v: &mut Vec<T>, item: T, n: usize) -> Result<(), OutOfMemory> {
    grow(v, n)?;
    v.extend(std::iter::repeat_n(item, n));
    Ok(())
}

/// Makes room for `n` more elements: nothing when they fit, otherwise the larger of what is needed
/// and double the capacity.
fn grow<T>(v: &mut Vec<T>, n: usize) -> Result<(), OutOfMemory> {
    let need = v.len().checked_add(n).ok_or(OutOfMemory)?;
    if need <= v.capacity() {
        return Ok(());
    }
    let want = need.max(v.capacity().saturating_mul(2));
    try_reserve(v, want - v.len())
}

/// Makes room for `n` more elements of a live block's buffer, adding at most `room` bytes of
/// capacity (what is left under the usage limit): double the capacity when that fits, otherwise
/// exactly what is needed, otherwise `OutOfMemory`.
fn grow_within<T>(v: &mut Vec<T>, n: usize, room: usize) -> Result<(), OutOfMemory> {
    let need = v.len().checked_add(n).ok_or(OutOfMemory)?;
    if need <= v.capacity() {
        return Ok(());
    }
    let afford = v
        .capacity()
        .saturating_add(room / std::mem::size_of::<T>().max(1));
    if need > afford {
        return Err(OutOfMemory);
    }
    let want = need.max(v.capacity().saturating_mul(2)).min(afford);
    try_reserve(v, want - v.len())
}

/// Element `i` of a dynamic array of bytes (the block `r`) := the low 8 bits of `v` (the
/// compiler wrote a `Byte`); returns the old value, zero-extended as a read gives it.
fn replace_byte(bytes: &mut [u8], r: StrRef, i: usize, v: i64) -> Result<i64, BadRef> {
    let b = bytes.get_mut(i).ok_or_else(|| slot_out_of_range(r, i))?;
    Ok(i64::from(std::mem::replace(b, v as u8)))
}

/// The error of slot `i` outside the block `r`.
fn slot_out_of_range(r: StrRef, i: impl std::fmt::Display) -> BadRef {
    BadRef {
        message: format!("slot {i} out of range in heap block {r}"),
    }
}

/// A new buffer of exactly `n` bytes: a copy of `prefix` (at most `n` long) padded with 0
/// (reserved through [`try_reserve`]).
fn padded_bytes(prefix: &[u8], n: usize) -> Result<Vec<u8>, OutOfMemory> {
    let mut bytes = Vec::new();
    try_reserve(&mut bytes, n)?;
    bytes.extend_from_slice(prefix);
    bytes.resize(n, 0);
    Ok(bytes)
}

/// Appends the buffer's own elements to it (a string appended to itself), growing it within
/// `room` ([`grow_within`]) and copying inside it, so no other buffer is allocated.
fn append_within<T: Copy>(v: &mut Vec<T>, room: usize) -> Result<(), OutOfMemory> {
    let n = v.len();
    grow_within(v, n, room)?;
    v.extend_from_within(..n);
    Ok(())
}

/// The pieces joined into one new buffer of exactly their total length.
pub fn try_join<T: Copy>(parts: &[&[T]]) -> Result<Vec<T>, OutOfMemory> {
    let n = parts
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(p.len()))
        .ok_or(OutOfMemory)?;
    let mut v = Vec::new();
    try_reserve(&mut v, n)?;
    for p in parts {
        v.extend_from_slice(p);
    }
    Ok(v)
}

/// The code units of a string being built, reserved through [`try_reserve`]. Also a `fmt::Write`
/// sink: `core::fmt` itself does not allocate, so `write!` of a number with a precision of
/// millions of digits fails with `OutOfMemory` here instead of ending the process.
#[derive(Debug, Default)]
pub struct TextBuf {
    units: Vec<u16>,
    failed: bool,
}

impl TextBuf {
    /// An empty buffer with room for `n` units: a text known to be at least that long (the
    /// digits a precision asks for) is refused before any of it is written.
    pub fn with_room(n: usize) -> Result<TextBuf, OutOfMemory> {
        let mut units = Vec::new();
        try_reserve(&mut units, n)?;
        Ok(TextBuf {
            units,
            failed: false,
        })
    }

    pub fn push_units(&mut self, units: &[u16]) -> Result<(), OutOfMemory> {
        self.check()?;
        try_extend(&mut self.units, units)
    }

    /// Appends `n` copies of `unit`.
    pub fn fill(&mut self, unit: u16, n: usize) -> Result<(), OutOfMemory> {
        self.check()?;
        try_fill(&mut self.units, unit, n)
    }

    pub fn len(&self) -> usize {
        self.units.len()
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// The units written so far (to read back what a `write!` wrote).
    pub fn units_mut(&mut self) -> &mut Vec<u16> {
        &mut self.units
    }

    /// The text, or `OutOfMemory` if any write was refused.
    pub fn finish(self) -> Result<Vec<u16>, OutOfMemory> {
        self.check()?;
        Ok(self.units)
    }

    fn check(&self) -> Result<(), OutOfMemory> {
        if self.failed {
            Err(OutOfMemory)
        } else {
            Ok(())
        }
    }
}

impl std::fmt::Write for TextBuf {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.failed {
            return Err(std::fmt::Error);
        }
        let r = if s.is_ascii() {
            grow(&mut self.units, s.len()).map(|()| self.units.extend(s.bytes().map(u16::from)))
        } else {
            let n = s.encode_utf16().count();
            grow(&mut self.units, n).map(|()| self.units.extend(s.encode_utf16()))
        };
        self.failed = r.is_err();
        r.map_err(|_| std::fmt::Error)
    }
}

impl PStr {
    /// A one-byte string of `n` characters that `fill` writes (all below U+0080): in place when it
    /// is short, otherwise in a new buffer of exactly `n` bytes.
    fn build_ascii(n: usize, fill: impl FnOnce(&mut [u8])) -> Result<PStr, OutOfMemory> {
        if n <= SHORT_LEN {
            let mut s = ShortStr {
                len: u8::try_from(n).expect("SHORT_LEN fits in a u8"),
                bytes: [0; SHORT_LEN],
            };
            fill(s.bytes_mut());
            return Ok(PStr::Short(s));
        }
        let mut v = Vec::new();
        try_reserve(&mut v, n)?;
        v.resize(n, 0);
        fill(&mut v);
        Ok(PStr::Ascii(v))
    }

    /// A one-byte string of `bytes` (all below U+0080).
    pub fn ascii(bytes: &[u8]) -> Result<PStr, OutOfMemory> {
        PStr::build_ascii(bytes.len(), |buf| buf.copy_from_slice(bytes))
    }

    /// A string of the given code units (one byte each when all are below U+0080).
    pub fn from_units(units: &[u16]) -> Result<PStr, OutOfMemory> {
        if units.iter().all(|u| *u < 0x80) {
            PStr::build_ascii(units.len(), |buf| {
                for (b, u) in buf.iter_mut().zip(units) {
                    *b = *u as u8;
                }
            })
        } else {
            Ok(PStr::Utf16(try_join(&[units])?))
        }
    }

    pub fn from_text(s: &str) -> Result<PStr, OutOfMemory> {
        if s.is_ascii() {
            PStr::ascii(s.as_bytes())
        } else {
            let mut v = Vec::new();
            try_reserve(&mut v, s.encode_utf16().count())?;
            v.extend(s.encode_utf16());
            Ok(PStr::Utf16(v))
        }
    }

    /// The string as an extension borrows it (`docs/design/extensions.md` 3.5).
    pub fn ext_view(&self) -> puy_ext::PasStr<'_> {
        match self.view() {
            // One byte per character means every character is below U+0080
            View::Ascii(a) => {
                puy_ext::PasStr::Ascii(std::str::from_utf8(a).expect("a one-byte string is ASCII"))
            }
            View::Utf16(u) => puy_ext::PasStr::Utf16(u),
        }
    }

    /// The code units, borrowed.
    fn view(&self) -> View<'_> {
        match self {
            PStr::Ascii(a) => View::Ascii(a),
            PStr::Short(s) => View::Ascii(s.bytes()),
            PStr::Utf16(u) => View::Utf16(u),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            PStr::Ascii(a) => a.len(),
            PStr::Utf16(u) => u.len(),
            PStr::Short(s) => s.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The code unit at a 0-based index.
    pub fn unit(&self, i: usize) -> u16 {
        match self.view() {
            View::Ascii(a) => a[i] as u16,
            View::Utf16(u) => u[i],
        }
    }

    /// The code units, without copying (an iterator over either representation).
    pub fn iter_units(&self) -> impl Iterator<Item = u16> + '_ {
        let (a, u): (&[u8], &[u16]) = match self.view() {
            View::Ascii(a) => (a, &[]),
            View::Utf16(u) => (&[], u),
        };
        a.iter().map(|b| u16::from(*b)).chain(u.iter().copied())
    }

    /// A copy of the code units.
    pub fn units(&self) -> Result<Vec<u16>, OutOfMemory> {
        let mut v = Vec::new();
        try_reserve(&mut v, self.len())?;
        self.append_units(&mut v);
        Ok(v)
    }

    /// Appends the code units to `v`, which has room for them.
    fn append_units(&self, v: &mut Vec<u16>) {
        match self.view() {
            View::Ascii(a) => v.extend(a.iter().map(|b| u16::from(*b))),
            View::Utf16(u) => v.extend_from_slice(u),
        }
    }

    /// Number of bytes of the buffer, capacity included (for usage accounting); 0 for a string
    /// kept in place.
    fn heap_size(&self) -> usize {
        match self {
            PStr::Ascii(a) => a.capacity(),
            PStr::Utf16(u) => u.capacity() * 2,
            PStr::Short(_) => 0,
        }
    }

    /// Gives the buffer back its unused capacity (a new block holds exactly its text).
    fn fit(&mut self) {
        match self {
            PStr::Ascii(a) => a.shrink_to_fit(),
            PStr::Utf16(u) => u.shrink_to_fit(),
            PStr::Short(_) => {}
        }
    }

    /// Appends the string to itself in place ([`append_within`]).
    fn append_self(&mut self, room: usize) -> Result<(), OutOfMemory> {
        match self {
            PStr::Ascii(a) => append_within(a, room),
            PStr::Utf16(u) => append_within(u, room),
            PStr::Short(s) => {
                let copy = PStr::Short(*s);
                self.append(&copy, room)
            }
        }
    }

    /// Appends `other` in place, growing the buffer by at most `room` bytes of capacity
    /// ([`grow_within`]). An ASCII string becomes UTF-16 when `other` is not ASCII (once: the
    /// copy is made with room to spare), and a short one moves to a buffer of its own when the
    /// result no longer fits in place (with room to spare too). On `OutOfMemory` the string is
    /// unchanged.
    fn append(&mut self, other: &PStr, room: usize) -> Result<(), OutOfMemory> {
        match (&mut *self, other.view()) {
            (PStr::Short(s), View::Ascii(b)) if s.fits(b.len()) => s.push(b),
            (PStr::Short(s), View::Ascii(b)) => {
                let need = s.len() + b.len();
                if need > room {
                    return Err(OutOfMemory);
                }
                let mut v = Vec::new();
                try_reserve(&mut v, need.max(s.len() * 2).min(room))?;
                v.extend_from_slice(s.bytes());
                v.extend_from_slice(b);
                *self = PStr::Ascii(v);
            }
            (PStr::Ascii(a), View::Ascii(b)) => {
                grow_within(a, b.len(), room)?;
                a.extend_from_slice(b);
            }
            (PStr::Ascii(_) | PStr::Short(_), View::Utf16(_)) => {
                let len = self.len();
                let need = len.checked_add(other.len()).ok_or(OutOfMemory)?;
                let afford = (self.heap_size().saturating_add(room)) / 2;
                if need > afford {
                    return Err(OutOfMemory);
                }
                let mut v = Vec::new();
                try_reserve(&mut v, need.max(len.saturating_mul(2)).min(afford))?;
                self.append_units(&mut v);
                other.append_units(&mut v);
                *self = PStr::Utf16(v);
            }
            (PStr::Utf16(u), _) => {
                grow_within(u, other.len(), room)?;
                other.append_units(u);
            }
        }
        Ok(())
    }

    /// The number of bytes the buffer needs to hold this string followed by `other` with no
    /// capacity to spare ([`PStr::append_exact`]).
    fn joined_size(&self, other: &PStr) -> usize {
        let n = self.len() + other.len();
        match (self.view(), other.view()) {
            (View::Ascii(_), View::Ascii(_)) => n,
            _ => n * 2,
        }
    }

    /// Appends `other` in place growing the buffer to exactly the joined length (a new value
    /// holds no capacity to spare: `docs/spec/types.md` 9.1). An ASCII string becomes UTF-16 when
    /// `other` is not ASCII. On `OutOfMemory` the string is unchanged.
    fn append_exact(&mut self, other: &PStr) -> Result<(), OutOfMemory> {
        match (&mut *self, other.view()) {
            (PStr::Short(s), View::Ascii(b)) if s.fits(b.len()) => s.push(b),
            (PStr::Ascii(a), View::Ascii(b)) => {
                try_reserve(a, b.len())?;
                a.extend_from_slice(b);
            }
            (PStr::Utf16(u), _) => {
                try_reserve(u, other.len())?;
                other.append_units(u);
            }
            _ => *self = self.concat(other)?,
        }
        Ok(())
    }

    /// The characters (lone surrogates become U+FFFD; §5.5).
    fn chars(&self) -> impl Iterator<Item = char> + '_ {
        char::decode_utf16(self.iter_units()).map(|c| c.unwrap_or('\u{FFFD}'))
    }

    /// UTF-8 for a Pascal value (`UTF8String`, the text of a conversion), reserved through
    /// [`try_reserve`].
    pub fn to_utf8(&self) -> Result<String, OutOfMemory> {
        let mut v = Vec::new();
        try_reserve(&mut v, self.chars().map(char::len_utf8).sum())?;
        let mut buf = [0u8; 4];
        for c in self.chars() {
            v.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
        Ok(String::from_utf8(v).expect("encoded from chars"))
    }

    /// UTF-8 for the VM's own texts (the report of an exception), allocated like any Rust value:
    /// a refusal is the failure of an allocation inside Rust (F0003), not `EOutOfMemory`.
    pub fn text(&self) -> String {
        self.chars().collect()
    }

    pub fn concat(&self, other: &PStr) -> Result<PStr, OutOfMemory> {
        PStr::join([self, other].into_iter())
    }

    /// The strings `parts` joined into a new string of exactly their total length, reserved once
    /// (`ConcatStr`, `ConcatStrN`): one-byte when every part is, otherwise UTF-16. `OutOfMemory`
    /// when the total is over the length limit or the system refuses.
    pub fn join<'a>(parts: impl Iterator<Item = &'a PStr> + Clone) -> Result<PStr, OutOfMemory> {
        let (len, ascii) = parts.clone().fold((0usize, true), |(n, ascii), p| {
            (
                n.saturating_add(p.len()),
                ascii && matches!(p.view(), View::Ascii(_)),
            )
        });
        if len > MAX_STRING_LEN {
            return Err(OutOfMemory);
        }
        if ascii {
            PStr::build_ascii(len, |buf| {
                let mut at = 0;
                for p in parts {
                    if let View::Ascii(a) = p.view() {
                        buf[at..at + a.len()].copy_from_slice(a);
                        at += a.len();
                    }
                }
            })
        } else {
            let mut v = Vec::new();
            try_reserve(&mut v, len)?;
            for p in parts {
                p.append_units(&mut v);
            }
            Ok(PStr::Utf16(v))
        }
    }

    /// Content equality (independent of representation).
    pub fn equals(&self, other: &PStr) -> bool {
        match (self.view(), other.view()) {
            (View::Ascii(a), View::Ascii(b)) => a == b,
            (View::Utf16(a), View::Utf16(b)) => a == b,
            _ => self.len() == other.len() && self.iter_units().eq(other.iter_units()),
        }
    }

    /// Comparison in code point order (§5.11).
    pub fn compare(&self, other: &PStr) -> std::cmp::Ordering {
        match (self.view(), other.view()) {
            (View::Ascii(a), View::Ascii(b)) => a.cmp(b),
            _ => crate::sema::compare_units(self.iter_units(), other.iter_units()),
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
            PStr::Short(s) if c < 0x80 => {
                s.bytes_mut()[i] = c as u8;
                true
            }
            PStr::Ascii(_) | PStr::Short(_) => false,
            PStr::Utf16(u) => {
                u[i] = c;
                true
            }
        }
    }

    /// A new value (copy) with one element of the string replaced. `i` is 0-based.
    pub fn with_unit(&self, i: usize, c: u16) -> Result<PStr, OutOfMemory> {
        match self.view() {
            View::Ascii(a) if c < 0x80 => PStr::build_ascii(a.len(), |buf| {
                buf.copy_from_slice(a);
                buf[i] = c as u8;
            }),
            _ => {
                let mut v = self.units()?;
                v[i] = c;
                Ok(PStr::Utf16(v))
            }
        }
    }

    pub fn slice(&self, start: usize, end: usize) -> Result<PStr, OutOfMemory> {
        match self.view() {
            View::Ascii(a) => PStr::ascii(&a[start..end]),
            View::Utf16(u) => PStr::from_units(&u[start..end]),
        }
    }

    /// The 0-based index of the first occurrence of `sub` in the code units (`Pos`), compared in
    /// place in either representation; `None` when `sub` is empty or does not occur.
    pub fn find(&self, sub: &PStr) -> Option<usize> {
        match (self.view(), sub.view()) {
            (View::Ascii(s), View::Ascii(t)) => find_units(s, t),
            (View::Ascii(s), View::Utf16(t)) => find_units(s, t),
            (View::Utf16(s), View::Ascii(t)) => find_units(s, t),
            (View::Utf16(s), View::Utf16(t)) => find_units(s, t),
        }
    }

    /// A copy with the letters `a`..`z` changed to `A`..`Z` (`upper`) or the other way round,
    /// and every other code unit kept (`UpperCase` / `LowerCase`).
    pub fn map_ascii_case(&self, upper: bool) -> Result<PStr, OutOfMemory> {
        match self.view() {
            View::Ascii(a) => PStr::build_ascii(a.len(), |buf| {
                buf.copy_from_slice(a);
                if upper {
                    buf.make_ascii_uppercase();
                } else {
                    buf.make_ascii_lowercase();
                }
            }),
            View::Utf16(u) => {
                let mut v = try_join(&[u])?;
                for x in &mut v {
                    if upper && (97..=122).contains(x) {
                        *x -= 32;
                    } else if !upper && (65..=90).contains(x) {
                        *x += 32;
                    }
                }
                PStr::from_units(&v)
            }
        }
    }
}

impl PartialEq for PStr {
    fn eq(&self, other: &PStr) -> bool {
        self.equals(other)
    }
}

impl Eq for PStr {}

/// The 0-based index of the first occurrence of `sub` in `s`, comparing code units; `None` when
/// `sub` is empty or does not occur ([`PStr::find`]).
fn find_units<A, B>(s: &[A], sub: &[B]) -> Option<usize>
where
    A: Copy + Into<u16>,
    B: Copy + Into<u16>,
{
    let (first, rest) = sub.split_first()?;
    let first: u16 = (*first).into();
    let last = s.len().checked_sub(sub.len())?;
    (0..=last).find(|&i| {
        s[i].into() == first
            && s[i + 1..i + sub.len()]
                .iter()
                .zip(rest)
                .all(|(x, y)| (*x).into() == (*y).into())
    })
}

/// The result of joining strings ([`Heap::join_strs`]).
#[derive(Debug)]
pub enum Joined {
    /// An existing string (or 0), shared.
    Shared(StrRef),
    /// A new string.
    New(PStr),
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
    /// A `UTF8String` (`docs/spec/types.md` 8.5): valid UTF-8 bytes (capacity to spare after
    /// `AppendU8`, like a [`PStr`]).
    U8(Vec<u8>),
    /// A record, static array or dynamic array: the type index (`docs/spec/bytecode.md` 3.2) and
    /// the slots (a dynamic array grown by `SetLen` in place keeps capacity to spare;
    /// `docs/spec/types.md` 9.1).
    Agg {
        ty: u32,
        slots: Vec<i64>,
    },
    /// A dynamic array of bytes (`docs/spec/bytecode.md` 2.9; requirements §5.5): the type index
    /// (an entry with [`crate::bytecode::DYN_BYTES`]) and one byte per element (capacity to
    /// spare after `SetLen`, as an `Agg`'s slots).
    Bytes {
        ty: u32,
        bytes: Vec<u8>,
    },
    /// An instance of a class (`docs/spec/objects.md` 14.1): the class's type index and the
    /// fields. The handle's generation is in the block's tag. `dying` is the mark of an instance
    /// being destroyed (3.3), and `dtor` its destruction state: the most distant ancestor whose
    /// destructor body has started (`DtorEnter`; 8.5). An exception object (14.8) also has its
    /// stack trace and the exceptions in `Suppressed` once it has any (boxed, so that an instance
    /// that never carries them stays small; `docs/design/objects.md` 3.2 B).
    Obj {
        ty: u32,
        slots: Box<[i64]>,
        dying: bool,
        dtor: Option<u32>,
        extra: Option<Box<ExcExtra>>,
    },
    /// A callable value (`docs/spec/objects.md` 14.9).
    Callable(Callable),
    /// A closure (`docs/spec/objects.md` 14.9): the function (`NO_FUNC` for a group block) and
    /// the cells it captured (each holding a reference).
    Closure {
        func: u32,
        caps: Caps,
    },
    /// A cell of a captured variable (`docs/spec/objects.md` 14.9): the kind of its value (0, 1 or
    /// 2, as a slot's), the value, and where it was made (the function and the register, whose
    /// local name the leak report shows).
    Cell {
        kind: u8,
        value: i64,
        func: u32,
        reg: u16,
    },
}

/// The most cells a closure keeps in its block; more go in a box of their own ([`Caps`]).
const INLINE_CAPS: usize = 3;

/// The captured cells of a closure: a few in the block itself, so that making and freeing a
/// closure that captures little allocates nothing besides the block (`docs/design/objects.md`
/// 4); more in a box of their own.
#[derive(Debug)]
pub enum Caps {
    Inline { len: u8, cells: [i64; INLINE_CAPS] },
    Boxed(Box<[i64]>),
}

impl Caps {
    /// A copy of `cells`.
    pub fn new(cells: &[i64]) -> Result<Caps, OutOfMemory> {
        if cells.len() <= INLINE_CAPS {
            let len = u8::try_from(cells.len()).expect("INLINE_CAPS fits in a u8");
            let mut inline = [0; INLINE_CAPS];
            inline[..cells.len()].copy_from_slice(cells);
            return Ok(Caps::Inline { len, cells: inline });
        }
        let mut list = Vec::new();
        try_reserve(&mut list, cells.len())?;
        list.extend_from_slice(cells);
        Ok(Caps::Boxed(list.into_boxed_slice()))
    }

    /// Bytes counted toward usage (§5.12) besides the block's.
    fn heap_size(&self) -> usize {
        match self {
            Caps::Inline { .. } => 0,
            Caps::Boxed(b) => b.len() * 8,
        }
    }
}

impl std::ops::Deref for Caps {
    type Target = [i64];

    fn deref(&self) -> &[i64] {
        match self {
            Caps::Inline { len, cells } => &cells[..*len as usize],
            Caps::Boxed(b) => b,
        }
    }
}

/// The function of a closure without one (a group block; `MakeClosure` with `-1`).
pub const NO_FUNC: u32 = u32::MAX;

/// The variant of a callable value block (`docs/spec/objects.md` 14.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableKind {
    /// A routine (a procedural value as an anonymous method type's value, a static method).
    Routine,
    /// A method of an instance, held weakly (its handle, not counted).
    Method,
    /// A class method of a class reference.
    ClassMethod,
    /// A closure: an anonymous method's value (its captured cells are in the block).
    Closure,
}

/// A callable value block (`docs/spec/objects.md` 14.9): its variant, the function (an index of
/// the function table, or a built-in body `0xFFFFFF00 + k`), and the target (the instance's
/// handle or the class reference; 0 for a routine). Never written once made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Callable {
    pub kind: CallableKind,
    pub func: u32,
    pub target: i64,
}

/// What is left of a destroyed instance (`Heap::end_destroy`): its fields (and an exception's
/// `Suppressed`), for a release job to let go of ([`Heap::job_push_fields`]).
#[derive(Debug)]
pub struct Remains(Body);

/// What an instance holds besides its slots: an exception object once raised, its stack trace
/// and the exceptions in `Suppressed` (each holding a reference); an instance of an extension
/// class, the Rust value (`docs/design/extensions.md` 3.4), dropped with the body.
#[derive(Default)]
struct ExcExtra {
    trace: Option<Trace>,
    suppressed: Vec<i64>,
    /// The subexpressions and values a failed `Assert` recorded (`EAssertionFailed.Values`;
    /// `docs/spec/semantics.md` 5.4).
    values: Vec<(String, String)>,
    ext: Option<Rc<dyn Any>>,
}

impl ExcExtra {
    /// Bytes counted toward usage: kept out of line, as only exception objects and instances of
    /// extension classes have it, and inlined it slows down the release of every block.
    #[cold]
    #[inline(never)]
    fn size(&self) -> usize {
        self.suppressed.len() * 8
            + self.trace.as_ref().map_or(0, |t| t.frames.len() * 8)
            + values_size(&self.values)
    }
}

impl std::fmt::Debug for ExcExtra {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExcExtra")
            .field("trace", &self.trace)
            .field("suppressed", &self.suppressed)
            .field("values", &self.values)
            .field("ext", &self.ext.is_some())
            .finish()
    }
}

/// The bytes the values recorded on an exception object count toward usage.
fn values_size(values: &[(String, String)]) -> usize {
    values.iter().map(|(e, v)| e.len() + v.len() + 48).sum()
}

impl Body {
    /// Bytes counted toward usage (§5.12).
    fn size(&self) -> usize {
        match self {
            Body::Str(s) => s.heap_size() + std::mem::size_of::<PStr>(),
            Body::U8(b) => b.capacity() + std::mem::size_of::<Vec<u8>>(),
            Body::Agg { slots, .. } => slots.capacity() * 8 + std::mem::size_of::<Body>(),
            Body::Bytes { bytes, .. } => bytes.capacity() + std::mem::size_of::<Body>(),
            Body::Obj { slots, extra, .. } => {
                slots.len() * 8
                    + extra.as_ref().map_or(0, |x| x.size())
                    + std::mem::size_of::<Body>()
            }
            Body::Callable(_) | Body::Cell { .. } => std::mem::size_of::<Body>(),
            Body::Closure { caps, .. } => caps.heap_size() + std::mem::size_of::<Body>(),
        }
    }
}

/// The stack trace of an exception object (`docs/spec/exceptions.md` 6.1): (function index, line)
/// from the innermost frame (an inlined call is a frame of its own), at most 32 (16 innermost +
/// 16 outermost), and the number of frames omitted between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace {
    pub frames: Vec<(u32, u32)>,
    pub omitted: usize,
}

/// One location lent to a `var` / `out` parameter (`Lend`, `LendPlace`;
/// `docs/design/lifetime.md` 3.7), an entry of the lending table.
#[derive(Debug, Clone, Copy)]
pub struct Loan {
    /// The register (absolute position) holding the value taken out (`Lend`), the reference
    /// to the flat place (`LendPlace`), or the block it was taken out into (`PlaceBox`).
    pub value: usize,
    /// The root: a reference slot (kind 1 to 4), or an instance's handle.
    pub root: i64,
    /// The first register (absolute position) of the path and its length.
    pub path: usize,
    pub len: usize,
    /// The number of frames when the location was lent (the frame of the call).
    pub depth: usize,
    /// The lent slots: the block and the first slot number, `None` once the block of a lent
    /// value was freed or the slot dropped. A flat place is never `None`: when its block goes,
    /// its slots move to a block of their own ([`Loan::evacuated`]).
    pub loc: Option<(StrRef, usize)>,
    /// The number of slots lent from `loc`: 1 for a value, the width of a flat place.
    pub width: usize,
    /// The type of a flat place (`LendPlace`; `docs/spec/bytecode.md` 2.8), `None` for a value.
    pub place: Option<u32>,
    /// The kind of the slot of a lent value ([`SLOT_PLAIN`], [`SLOT_REF`], [`SLOT_OBJ`]), for
    /// letting go of it when it cannot be given back.
    pub kind: u8,
    /// Whether the slots of the flat place were moved out of a block that was freed or shrunk
    /// into a block of the place's type that the loan holds (`loc` names it, at slot 0).
    pub evacuated: bool,
    /// Whether the values of the flat place were taken out into a block in the register
    /// `value` (`PlaceBox`; `docs/spec/bytecode.md` 2.8): the loan then lends `loc` as `Lend`
    /// lends a value, and gives the block's slots back.
    pub boxed: bool,
    /// The first of its containers in the containers list ([`Heap::loan_containers`]) and their
    /// number.
    pub containers: usize,
    pub ncontainers: usize,
}

impl Loan {
    /// Whether slot `i` of the block `r` is one of the lent slots.
    #[inline]
    pub fn covers(&self, r: StrRef, i: usize) -> bool {
        matches!(self.loc, Some((b, s)) if b == r && i >= s && i - s < self.width)
    }

    /// Whether one of the slots `start..end` of the block `r` is one of the lent slots.
    pub fn overlaps(&self, r: StrRef, start: usize, end: usize) -> bool {
        matches!(self.loc, Some((b, s)) if b == r && start < s + self.width && s < end)
    }
}

/// A container walked by `Lend`, from the root: the block (`None` once freed), its type, and
/// whether `Lend` marked it.
#[derive(Debug, Clone, Copy)]
pub struct LoanContainer {
    pub block: Option<StrRef>,
    pub ty: u32,
    pub marked: bool,
}

/// Why touching a location is refused while it is lent (`EAliasError`; `docs/spec/types.md`
/// chapter 3). The VM turns it into the message (`docs/spec/semantics.md` 7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aliased {
    /// A slot read, written or lent while lent: an element of a dynamic or open array (with its
    /// index), of a static array, or a field; `twice` when lent again in the same call.
    Elem {
        index: Option<i64>,
        twice: bool,
    },
    Field {
        twice: bool,
    },
    /// A field of an instance (`docs/spec/objects.md` 2.2).
    ObjField {
        twice: bool,
    },
    /// A value containing a lent location copied.
    Copy,
}

/// Whether a value is a heap reference rather than a reference slot (whose upper 8 bits hold its
/// kind; `docs/spec/bytecode.md` 1.5) or 0.
pub fn is_block_ref(v: i64) -> bool {
    v != 0 && super::deref(v).0 == 0
}

/// One slot of the slab. `body` is `None` once the block has been freed.
#[derive(Debug)]
struct Block {
    rc: u32,
    /// The number of lent locations the block holds or contains (`docs/design/lifetime.md` 3.7);
    /// 0 for nearly every block, so the checks look at it first.
    lent: u32,
    /// The head word (`docs/design/objects.md` 3.2): the generation in the upper 24 bits (advanced
    /// each time the block is freed, so a handle to a freed instance no longer matches) and the
    /// kind in the lower 8 ([`TAG_OBJ_MANUAL`], …).
    tag: u32,
    /// The hash a string or `UTF8String` block remembers (`HashStr` / `HashUtf8`;
    /// `docs/design/vm.md` 3.18), or 0 when it has none: set to 0 when the block is placed and
    /// whenever its content is rewritten ([`Block::body_to_rewrite`]). It fills the padding
    /// between the head and the body, so the block is no larger.
    hash: u32,
    body: Option<Body>,
}

impl Block {
    /// The body, to change a string's or `UTF8String`'s content in place: the remembered hash is
    /// forgotten first, as it no longer matches (`docs/spec/bytecode.md` 2.21). Every in-place
    /// rewrite of a text goes through here.
    fn body_to_rewrite(&mut self) -> &mut Option<Body> {
        self.hash = 0;
        &mut self.body
    }
}

/// The kind in a block's tag of a block that is not an instance.
const TAG_PLAIN: u32 = 0;
/// The kind in a block's tag of an instance of a class managed by hand.
const TAG_OBJ_MANUAL: u32 = 1;
/// The kind in a block's tag of an instance of a reference counted class.
const TAG_OBJ_COUNTED: u32 = 2;
/// The kind in a block's tag of a tombstone (`docs/spec/objects.md` 3.3): an instance of a
/// reference counted class destroyed while references to it remained (it was resurrected). The
/// body is gone and the generation still matches those references, which `rc` counts; the index
/// is reused once they are all let go. Accesses through them see a freed instance (the body is
/// gone).
const TAG_TOMB: u32 = 3;
/// The bits of the kind in a block's tag.
const TAG_KIND: u32 = 0xFF;
/// The largest generation; a block that reaches it is retired (its index is not reused), so a
/// handle never matches a later instance (`docs/design/vm.md` 3.3).
const MAX_GENERATION: u32 = 0xFF_FFFF;

/// Why a handle does not reach a live instance (`docs/spec/objects.md` 14.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadHandle {
    /// `nil`.
    Nil,
    /// The instance was destroyed (the generation does not match).
    Freed,
    /// The generation matches but the block is not an instance, or the index or slot is out of
    /// range: only forged bytecode does this (F0001).
    Fatal(String),
}

/// The `next` of an entry of a hash table that was removed (a hole; `docs/spec/bytecode.md` 2.23).
pub const HASH_HOLE: i64 = -2;

/// The slab index of a heap reference or a handle: its lower 32 bits less 1 (a handle's
/// generation is in the bits above; `docs/spec/objects.md` 14.1). `None` for 0.
fn slab_index(r: i64) -> Option<usize> {
    usize::try_from((r & 0xFFFF_FFFF) - 1).ok()
}

/// The handle of the instance in slab index `index` with generation `generation`: the index + 1
/// in the lower 32 bits and the generation in bits 32–55 (`docs/spec/objects.md` 14.1).
fn make_handle(index: usize, generation: u32) -> i64 {
    (i64::from(generation) << 32) | (index as i64 + 1)
}

/// The most slots of an instance whose slot box [`Heap::slot_pool`] keeps.
const SLOT_POOL_LENS: usize = 8;
/// The most slot boxes of one length [`Heap::slot_pool`] keeps.
const SLOT_POOL_KEEP: usize = 16;

/// The isolate's heap: one slab of reference-counted blocks (`docs/design/lifetime.md` 3.2).
/// The most entries `Heap::release` keeps of its work list between calls; a longer list (from
/// releasing a large array) is shrunk back to this.
const RELEASE_WORK_KEEP: usize = 1024;

pub struct Heap {
    blocks: Vec<Block>,
    /// The type entries by type index: the VM's built-in types (0–63), then the module's type
    /// section (which slots hold references, names for the leak report).
    types: Vec<TypeEntry>,
    /// The ancestors of each class by type index, the root first and the class itself last
    /// (empty for the other kinds): the class `a` descends from `b` when
    /// `ancestors[a][depth of b] == b` (`docs/design/objects.md` 3.4).
    ancestors: Vec<Box<[u32]>>,
    /// Whether the destruction of an instance of each type runs Pascal code
    /// ([`Heap::destruction_runs_code`]), by type index; computed once, as freeing an instance
    /// asks it each time.
    runs_code: Vec<bool>,
    /// Whether each type index is a dynamic array whose elements take one slot, the arrays the
    /// hash table instructions read ([`Heap::hash_arrays`]); computed once, as each of those
    /// instructions asks it of 3 or 4 arrays.
    hash_types: Vec<bool>,
    /// The slot boxes of freed instances kept for reuse, by length less 1 (at most
    /// [`SLOT_POOL_LENS`] slots, [`SLOT_POOL_KEEP`] boxes per length), so that an instance made
    /// right after one of the same size is freed does not go to the allocator. Not counted in
    /// `used`: a box here belongs to no block, like memory the allocator keeps.
    slot_pool: Vec<Vec<Box<[i64]>>>,
    /// Indexes of freed blocks available for reuse (empty and unused when reuse is off).
    free: Vec<usize>,
    /// Reuse the indexes of freed blocks. Off under leak checking so that releasing too often always
    /// hits a freed block (`docs/design/lifetime.md` 3.12).
    reuse: bool,
    /// The bytes counted against the limit: the blocks, and the capacity of the release work
    /// lists and of the job stack (`Heap::count_capacity`).
    used: usize,
    peak: usize,
    limit: Option<usize>,
    empty: PStr,
    /// The work list of `release`'s cascades, kept empty between calls and reused (its capacity
    /// is counted in `used`).
    release_work: Vec<Work>,
    /// The release jobs (`docs/design/objects.md` 3.8 B), the current one last.
    jobs: Vec<Job>,
    /// Whether the last job is open: made by a release of the instruction running now (the VM
    /// stamps it when the instruction ends). Further releases go behind its work, so that values
    /// are let go in the order they were released.
    open: bool,
    /// The lending table (`docs/design/lifetime.md` 3.7): the locations lent to calls in
    /// progress, oldest first, and the containers each walked. Kept here so that freeing a
    /// marked block can forget it.
    loans: Vec<Loan>,
    loan_containers: Vec<LoanContainer>,
    /// The loans whose flat places move out of a block being freed (kept empty between calls
    /// and reused; [`Heap::forget_loaned_block`]).
    evacuating: Vec<(usize, usize)>,
}

/// One value of a release job's work (`docs/design/objects.md` 3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    /// A value holding a reference of its own (a slot of kind 1).
    Ref(i64),
    /// A value counted by its contents (a slot of kind 2).
    Obj(i64),
    /// An instance of a reference counted class whose last reference was let go: its destruction
    /// comes next.
    Destroy(i64),
}

/// Where a release job came from, which says what happens when it is done
/// (`docs/spec/objects.md` 8.5). The VM gives it when it stamps the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobOrigin {
    /// Not stamped yet: made by the instruction running now.
    Open,
    /// An instruction of the frame below: the exception is raised there (`pc`, its position).
    Instr { pc: u32 },
    /// `Ret` / `RetVoid` of a frame that was not the last: the exception is raised at the caller's
    /// `Call` (`pc`). The result was written to `result` (an absolute register; its kind is
    /// `kind`, 0 for none), which is let go if the job ends with an exception.
    Ret { pc: u32, result: u32, kind: u16 },
    /// `Ret` / `RetVoid` of the last frame: the frames are done.
    RetLast,
    /// The unwinding of an exception (the job's exception) dropped a frame: it goes on from
    /// `pc` of the frame below.
    Unwind { pc: Option<u32> },
    /// The unwinding of `Halt` dropped a frame: it goes on.
    Halt,
    /// The global variables of a unit at the end of the isolate (`docs/spec/objects.md` 8.5).
    Globals { unit: u32 },
    /// A job made while another job was being run (by the VM, not an instruction): its
    /// exception goes to the job below.
    Nested,
}

/// A release job: values to let go of, from the back; the exception recorded so far (holding its
/// reference; 0 for none); where it came from; the number of frames when it was stamped (it runs
/// only while that many frames remain; a destructor's frames go above it).
#[derive(Debug)]
pub struct Job {
    pub work: VecDeque<Work>,
    pub exc: i64,
    pub origin: JobOrigin,
    pub depth: u32,
}

/// What the top job asks of the VM (`Heap::run_job`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStep {
    /// Its work is done: the VM takes it off with [`Heap::pop_job`].
    Done,
    /// The instance needs its destruction procedure run in a frame (a destructor or
    /// `BeforeDestruction` written in Pascal); it is marked as being destroyed.
    Destroy(i64),
}

/// How a release left a block (`Heap::decrement`).
enum Dec {
    /// Still referenced (or not counted).
    Kept,
    /// Freed: its body, whose references are let go next.
    Freed(Body),
    /// An instance of a reference counted class lost its last reference (its handle).
    Dying(i64),
}

/// Allocation failure (`EOutOfMemory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

/// Use of a reference that does not point to a live block (an internal error, F0001; `docs/design/lifetime.md` 3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadRef {
    pub message: String,
}

/// The ancestors of each class in `types` (`Heap::ancestors`). A parent comes before its children
/// (the verifier checks it), so its list is already made; a parent that does not is taken as no
/// parent.
fn class_ancestors(types: &[TypeEntry]) -> Vec<Box<[u32]>> {
    let mut out: Vec<Box<[u32]>> = Vec::with_capacity(types.len());
    for (i, t) in types.iter().enumerate() {
        if t.kind != crate::bytecode::TYPE_CLASS {
            out.push(Box::new([]));
            continue;
        }
        let mut list = t
            .parent
            .and_then(|p| out.get(p as usize))
            .map_or_else(Vec::new, |l| l.to_vec());
        list.push(i as u32);
        out.push(list.into_boxed_slice());
    }
    out
}

/// Whether the destruction of an instance of the class of `t` runs Pascal code (a destructor or a
/// `BeforeDestruction` of its own; [`Heap::destruction_runs_code`]).
fn entry_runs_code(t: &TypeEntry) -> bool {
    [
        crate::bytecode::VMT_DESTROY,
        crate::bytecode::VMT_BEFORE_DESTRUCTION,
    ]
    .iter()
    .any(|&k| {
        t.vmt
            .get(k as usize)
            .is_some_and(|&f| f != crate::bytecode::VMT_BUILTIN_NOTHING)
    })
}

impl Heap {
    /// A heap for a module with the given type section.
    pub fn new(limit: Option<usize>, reuse: bool, section: &[TypeEntry]) -> Self {
        let mut types = crate::bytecode::builtin_types().to_vec();
        types.extend_from_slice(section);
        let ancestors = class_ancestors(&types);
        let runs_code = types.iter().map(entry_runs_code).collect();
        let hash_types = types
            .iter()
            .map(|t| t.kind == TYPE_DYNARRAY && t.elem_width() == 1)
            .collect();
        Heap {
            blocks: Vec::new(),
            types,
            ancestors,
            runs_code,
            hash_types,
            slot_pool: (0..SLOT_POOL_LENS).map(|_| Vec::new()).collect(),
            free: Vec::new(),
            reuse,
            used: 0,
            peak: 0,
            limit,
            empty: PStr::Ascii(Vec::new()),
            release_work: Vec::new(),
            jobs: Vec::new(),
            open: false,
            loans: Vec::new(),
            loan_containers: Vec::new(),
            evacuating: Vec::new(),
        }
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn peak(&self) -> usize {
        self.peak
    }

    pub fn limit(&self) -> Option<usize> {
        self.limit
    }

    pub fn reset_peak(&mut self) {
        self.peak = self.used;
    }

    /// Replaces `before` already counted bytes with `after` bytes and records a new peak.
    fn change_used(&mut self, before: usize, after: usize) {
        self.used = self.used - before + after;
        self.peak = self.peak.max(self.used);
    }

    /// The bytes left under the usage limit (no limit: unbounded).
    fn room(&self) -> usize {
        self.limit
            .map_or(usize::MAX, |l| l.saturating_sub(self.used))
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
        let header = std::mem::size_of::<PStr>().min(std::mem::size_of::<Vec<u8>>());
        limit.saturating_sub(self.used + header).min(MAX_STRING_LEN)
    }

    /// Puts a string on the heap and returns a reference that holds its one reference (the empty
    /// string is 0 and takes no block). Checks the length limit and the usage limit.
    pub fn alloc_str(&mut self, mut s: PStr) -> Result<StrRef, OutOfMemory> {
        if s.is_empty() {
            return Ok(0);
        }
        if s.len() > MAX_STRING_LEN {
            return Err(OutOfMemory);
        }
        s.fit();
        self.alloc(Body::Str(s))
    }

    /// Puts a `UTF8String` on the heap (the empty one is 0). Checks the length limit (in bytes,
    /// the same limit as a `string`'s) and the usage limit.
    pub fn alloc_u8(&mut self, mut b: Vec<u8>) -> Result<StrRef, OutOfMemory> {
        if b.is_empty() {
            return Ok(0);
        }
        if b.len() > MAX_STRING_LEN {
            return Err(OutOfMemory);
        }
        b.shrink_to_fit();
        self.alloc(Body::U8(b))
    }

    /// Whether `size` more bytes stay within the usage limit (§5.12).
    fn fits(&self, size: usize) -> bool {
        self.limit.is_none_or(|limit| self.used + size <= limit)
    }

    fn alloc(&mut self, body: Body) -> Result<StrRef, OutOfMemory> {
        if !self.fits(body.size()) {
            return Err(OutOfMemory);
        }
        Ok(self.place(body))
    }

    /// Puts a block in the slab without checking the usage limit (the usage still counts it).
    fn place(&mut self, body: Body) -> StrRef {
        self.place_tagged(body, TAG_PLAIN) as StrRef + 1
    }

    /// Puts a block of kind `kind` in the slab (without checking the usage limit) and returns its
    /// index. A reused index keeps its generation.
    fn place_tagged(&mut self, body: Body, kind: u32) -> usize {
        self.change_used(0, body.size());
        // An instance's handle has a generation of 1 or more, so that it never reads as the
        // reference of a record or array block (a cursor's container may be either; `docs/spec/
        // bytecode.md` 1.5): a stale handle then fails the generation check
        let first = if kind == TAG_PLAIN { 0 } else { 1 << 8 };
        match self.free.pop() {
            Some(i) => {
                let block = &mut self.blocks[i];
                block.rc = 1;
                block.lent = 0;
                let generation = (block.tag & !TAG_KIND).max(first);
                block.tag = generation | kind;
                block.hash = 0;
                block.body = Some(body);
                i
            }
            None => {
                self.blocks.push(Block {
                    rc: 1,
                    lent: 0,
                    tag: first | kind,
                    hash: 0,
                    body: Some(body),
                });
                self.blocks.len() - 1
            }
        }
    }

    /// Frees the block at `index` (its body already taken): advances its generation and makes the
    /// index available for reuse unless the generation is used up.
    fn retire(&mut self, index: usize) {
        let block = &mut self.blocks[index];
        let generation = (block.tag >> 8) + 1;
        block.tag = generation << 8;
        if self.reuse && generation < MAX_GENERATION {
            self.free.push(index);
        }
    }

    /// A new instance of the class `ty` with every slot 0 (`New`; `docs/spec/objects.md` 14.3),
    /// as its handle.
    pub fn alloc_obj(&mut self, ty: u32) -> Result<Result<i64, OutOfMemory>, BadRef> {
        let entry = self.type_entry(ty).ok_or_else(|| BadRef {
            message: format!("type {ty} out of range"),
        })?;
        let n = entry.slots as usize;
        let kind = if entry.flags & crate::bytecode::CLASS_REF_COUNTED != 0 {
            TAG_OBJ_COUNTED
        } else {
            TAG_OBJ_MANUAL
        };
        if !self.fits(n * 8 + std::mem::size_of::<Body>()) {
            return Ok(Err(OutOfMemory));
        }
        let slots = match self.slot_pool_of(n).and_then(Vec::pop) {
            Some(mut pooled) => {
                pooled.fill(0);
                pooled
            }
            None => {
                let mut slots = Vec::new();
                if try_reserve(&mut slots, n).is_err() {
                    return Ok(Err(OutOfMemory));
                }
                slots.resize(n, 0);
                slots.into_boxed_slice()
            }
        };
        let body = Body::Obj {
            ty,
            slots,
            dying: false,
            dtor: None,
            extra: None,
        };
        let index = self.place_tagged(body, kind);
        Ok(Ok(make_handle(index, self.blocks[index].tag >> 8)))
    }

    /// The slab index of the live instance a handle refers to (`docs/spec/objects.md` 14.4).
    pub fn obj_index(&self, h: i64) -> Result<usize, BadHandle> {
        if h == 0 {
            return Err(BadHandle::Nil);
        }
        let index = (h & 0xFFFF_FFFF) - 1;
        let generation = ((h as u64) >> 32) as u32;
        let block = usize::try_from(index)
            .ok()
            .and_then(|i| self.blocks.get(i))
            .ok_or_else(|| BadHandle::Fatal(format!("handle {h} out of range")))?;
        if block.tag >> 8 != generation || block.body.is_none() {
            return Err(BadHandle::Freed);
        }
        match block.body {
            Some(Body::Obj { .. }) => Ok(index as usize),
            _ => Err(BadHandle::Fatal(format!(
                "handle {h} does not refer to an instance"
            ))),
        }
    }

    /// What a weak handle reads (`LoadWeak`; `docs/spec/objects.md` 4.4): the handle while it
    /// matches a live instance that is not being destroyed, otherwise 0. An instance of a
    /// reference counted class that lost its last reference waits for its destruction (a job),
    /// and reads as being destroyed already.
    pub fn weak_target(&self, h: i64) -> Result<i64, BadHandle> {
        let index = match self.obj_index(h) {
            Ok(i) => i,
            Err(BadHandle::Nil | BadHandle::Freed) => return Ok(0),
            Err(e) => return Err(e),
        };
        let block = &self.blocks[index];
        let dying = match &block.body {
            Some(Body::Obj { dying, .. }) => *dying,
            _ => unreachable!("obj_index returns instances"),
        };
        let unreferenced = block.tag & TAG_KIND == TAG_OBJ_COUNTED && block.rc == 0;
        Ok(if dying || unreferenced { 0 } else { h })
    }

    /// The class of the live instance a handle refers to, or `None` when the handle does not
    /// check (the fast path of the method calls and of `IsClass` / `AsClass`; [`Heap::obj_class`]
    /// reports why).
    #[inline(always)]
    pub fn obj_class_fast(&self, h: i64) -> Option<u32> {
        let block = self.blocks.get(slab_index(h)?)?;
        if block.tag >> 8 != ((h as u64) >> 32) as u32 {
            return None;
        }
        match block.body.as_ref()? {
            Body::Obj { ty, .. } => Some(*ty),
            _ => None,
        }
    }

    /// Whether the instance `h` is of the class `ty` or a descendant (`IsClass`; `nil` is not),
    /// or `None` when the handle does not check (the fast path of `IsClass` / `AsClass`). Not
    /// inlined: inlined into `run_loop`, it left one more instruction in the dispatch of every
    /// instruction (`docs/notes/benchmark-m4c.md`, M4-60).
    #[inline(never)]
    pub fn is_class_fast(&self, h: i64, ty: u32) -> Option<bool> {
        if h == 0 {
            return Some(false);
        }
        Some(self.class_inherits(self.obj_class_fast(h)?, ty))
    }

    /// The class (type index) of the live instance a handle refers to (`docs/spec/objects.md`
    /// 14.4).
    pub fn obj_class(&self, h: i64) -> Result<u32, BadHandle> {
        let index = self.obj_index(h)?;
        Ok(self.obj_at(index).0)
    }

    /// The class (type index) and the slots of the live instance at slab index `index`.
    fn obj_at(&self, index: usize) -> (u32, &[i64]) {
        match &self.blocks[index].body {
            Some(Body::Obj { ty, slots, .. }) => (*ty, slots),
            _ => unreachable!("obj_index returns instances"),
        }
    }

    /// Slot `i` of the live instance at slab index `index` (from [`Heap::obj_index`], so the
    /// handle is checked once per instruction).
    pub fn obj_slot_at(&self, index: usize, i: usize) -> Result<i64, BadHandle> {
        self.obj_at(index)
            .1
            .get(i)
            .copied()
            .ok_or_else(|| BadHandle::Fatal(format!("slot {i} out of range in an instance")))
    }

    /// The slots of the live instance a handle refers to, or `None` when the handle does not
    /// check or one of the instance's slots is lent (the fast path of `ObjGet` / `ObjSet`;
    /// [`Heap::obj_index`] and [`Heap::check_slot`] report why).
    #[inline(always)]
    fn obj_slots_fast(&mut self, h: i64) -> Option<&mut [i64]> {
        let block = self.blocks.get_mut(slab_index(h)?)?;
        if block.tag >> 8 != ((h as u64) >> 32) as u32 || block.lent != 0 {
            return None;
        }
        match block.body.as_mut()? {
            Body::Obj { slots, .. } => Some(slots),
            _ => None,
        }
    }

    /// Slot `i` of the instance `h` (`ObjGet`), or `None` when the handle does not check, `i` is
    /// out of its slots or one of its slots is lent (the fast path).
    #[inline(always)]
    pub fn obj_get_fast(&mut self, h: i64, i: usize) -> Option<i64> {
        self.obj_slots_fast(h)?.get(i).copied()
    }

    /// Slots `i` to `i + N - 1` of the instance `h` (the fields `HashLookupInt` /
    /// `HashLookupStr` read; `docs/spec/bytecode.md` 2.23), or `None` under the same conditions
    /// as [`Heap::obj_get_fast`]: the block is looked at once for all of them.
    #[inline(always)]
    pub fn obj_fields_fast<const N: usize>(&mut self, h: i64, i: usize) -> Option<[i64; N]> {
        let slots = self.obj_slots_fast(h)?;
        slots.get(i..i.checked_add(N)?)?.try_into().ok()
    }

    /// Writes `v` to slot `i` of the instance `h` (`ObjSet` of an unmanaged slot), or `None` with
    /// nothing written under the same conditions as [`Heap::obj_get_fast`].
    #[inline(always)]
    pub fn obj_set_fast(&mut self, h: i64, i: usize, v: i64) -> Option<()> {
        *self.obj_slots_fast(h)?.get_mut(i)? = v;
        Some(())
    }

    /// Replaces slot `i` of the live instance at slab index `index` (shared, so written in place)
    /// and returns the old value.
    pub fn obj_replace_at(&mut self, index: usize, i: usize, v: i64) -> Result<i64, BadHandle> {
        match &mut self.blocks[index].body {
            Some(Body::Obj { slots, .. }) => slots
                .get_mut(i)
                .map(|s| std::mem::replace(s, v))
                .ok_or_else(|| BadHandle::Fatal(format!("slot {i} out of range in an instance"))),
            _ => unreachable!("obj_index returns instances"),
        }
    }

    /// Whether the live block `r` (slab index + 1) is an instance.
    pub fn is_obj_block(&self, r: StrRef) -> bool {
        matches!(
            self.block(r, "read"),
            Ok(Block {
                body: Some(Body::Obj { .. }),
                ..
            })
        )
    }

    /// Slot `i` of the live instance block `r` (slab index + 1) for writing.
    pub fn obj_slot_mut(&mut self, r: StrRef, i: usize) -> Result<&mut i64, BadRef> {
        match &mut self.block_mut(r, "write")?.body {
            Some(Body::Obj { slots, .. }) => slots.get_mut(i).ok_or_else(|| BadRef {
                message: format!("slot {i} out of range in an instance"),
            }),
            _ => Err(Self::bad("write", r)),
        }
    }

    /// Attaches the Rust value of an extension class to a new instance
    /// (`docs/design/extensions.md` 3.4).
    pub fn set_obj_ext(&mut self, h: i64, value: Rc<dyn Any>) -> Result<(), BadHandle> {
        let index = self.obj_index(h)?;
        if let Some(Body::Obj { extra, .. }) = &mut self.blocks[index].body {
            extra.get_or_insert_with(Box::default).ext = Some(value);
        }
        Ok(())
    }

    /// The Rust value an instance of an extension class holds (`None` for any other instance).
    pub fn obj_ext(&self, h: i64) -> Result<Option<Rc<dyn Any>>, BadHandle> {
        let index = self.obj_index(h)?;
        Ok(match &self.blocks[index].body {
            Some(Body::Obj { extra, .. }) => extra.as_ref().and_then(|x| x.ext.clone()),
            _ => None,
        })
    }

    /// Starts the destruction of the instance a handle refers to (`docs/spec/objects.md` 3.3, step
    /// 1): marks it as being destroyed and returns its class. `Ok(Err(None))` when it is already
    /// being destroyed (`EInvalidOperation`; 3.2).
    pub fn begin_destroy(&mut self, h: i64) -> Result<u32, Option<BadHandle>> {
        let index = self.obj_index(h).map_err(Some)?;
        match &mut self.blocks[index].body {
            Some(Body::Obj { dying: true, .. }) => Err(None),
            Some(Body::Obj { ty, dying, .. }) => {
                *dying = true;
                Ok(*ty)
            }
            _ => unreachable!("obj_index returns instances"),
        }
    }

    /// `DtorEnter` (`docs/spec/objects.md` 14.8): records that the destructor of `class` started
    /// on the instance `h`, when no destructor of a more distant ancestor did. Nothing for an
    /// instance that is not being destroyed.
    pub fn dtor_enter(&mut self, h: i64, class: u32) -> Result<(), BadHandle> {
        let index = self.obj_index(h)?;
        let recorded = match &self.blocks[index].body {
            Some(Body::Obj {
                dying: true, dtor, ..
            }) => *dtor,
            _ => return Ok(()),
        };
        let replace = match recorded {
            None => true,
            Some(r) => r != class && self.class_is(r, class),
        };
        if replace && let Some(Body::Obj { dtor, .. }) = &mut self.blocks[index].body {
            *dtor = Some(class);
        }
        Ok(())
    }

    /// The destruction state of the instance `h` (see [`Heap::dtor_enter`]).
    pub fn dtor_state(&self, h: i64) -> Result<Option<u32>, BadHandle> {
        let index = self.obj_index(h)?;
        match &self.blocks[index].body {
            Some(Body::Obj { dtor, .. }) => Ok(*dtor),
            _ => unreachable!("obj_index returns instances"),
        }
    }

    /// The last steps of the destruction of the instance a handle refers to
    /// (`docs/spec/objects.md` 3.3, steps 4 and 5): the memory is freed and the handle stops
    /// matching; the class and the slots are returned, for the caller to let go of the managed
    /// fields in declaration order (through a job). An instance of a reference counted class that
    /// references still point to (it was resurrected) leaves a tombstone.
    pub fn end_destroy(&mut self, h: i64) -> Result<Remains, BadHandle> {
        let index = self.obj_index(h)?;
        let block = &mut self.blocks[index];
        let lent = std::mem::take(&mut block.lent);
        let tomb = block.tag & TAG_KIND == TAG_OBJ_COUNTED && block.rc > 0;
        let mut body = block.body.take().expect("obj_index returns live blocks");
        self.used -= body.size();
        if lent != 0 {
            self.forget_loaned_block(index as StrRef + 1, &mut body);
        }
        if tomb {
            let block = &mut self.blocks[index];
            block.tag = (block.tag & !TAG_KIND) | TAG_TOMB;
        } else {
            self.retire(index);
        }
        Ok(Remains(body))
    }

    /// Whether `h` is an instance of a reference counted class (`docs/spec/objects.md` 3.2).
    pub fn obj_counted(&self, h: i64) -> Result<bool, BadHandle> {
        let index = self.obj_index(h)?;
        Ok(self.blocks[index].tag & TAG_KIND == TAG_OBJ_COUNTED)
    }

    /// The reference count of the instance `h` (`RefCount`; 0 for an instance managed by hand).
    pub fn obj_ref_count(&self, h: i64) -> Result<u32, BadHandle> {
        let index = self.obj_index(h)?;
        let block = &self.blocks[index];
        Ok(if block.tag & TAG_KIND == TAG_OBJ_COUNTED {
            block.rc
        } else {
            0
        })
    }

    /// Drops the reference of a new instance of a reference counted class without starting its
    /// destruction (`DestroyFailed`: the clean-up after a constructor's exception destroys it
    /// anyway; `docs/spec/objects.md` 3.1).
    pub fn obj_disown(&mut self, h: i64) -> Result<(), BadHandle> {
        let index = self.obj_index(h)?;
        let block = &mut self.blocks[index];
        if block.tag & TAG_KIND == TAG_OBJ_COUNTED {
            block.rc = block.rc.saturating_sub(1);
        }
        Ok(())
    }

    /// The live block a non-zero reference points to.
    fn block_mut(&mut self, r: StrRef, what: &str) -> Result<&mut Block, BadRef> {
        let index = slab_index(r).filter(|i| *i < self.blocks.len());
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

    /// The live block `r` refers to, or `None` (what [`Heap::block_mut`] accepts, without the
    /// message of a refusal).
    #[inline]
    fn live_block_mut(&mut self, r: StrRef) -> Option<&mut Block> {
        self.blocks
            .get_mut(slab_index(r)?)
            .filter(|b| b.body.is_some())
    }

    /// The string a reference points to (0 is empty). A reference outside the slab reads as empty
    /// (a mistyped register gives a wrong value, as in M1; `docs/design/vm.md` 3.12); a freed block is an error.
    pub fn get(&self, r: StrRef) -> Result<&PStr, BadRef> {
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

    /// `StrLen` of `r` when it is 0 or a live string (the fast path): `None` otherwise, so that the
    /// body decides (a freed block is an error there).
    #[inline(always)]
    pub fn str_len_fast(&self, r: StrRef) -> Option<i64> {
        if r == 0 {
            return Some(0);
        }
        match &self.blocks.get(usize::try_from(r - 1).ok()?)?.body {
            Some(Body::Str(s)) => Some(s.len() as i64),
            _ => None,
        }
    }

    /// The bytes of a `UTF8String` (0 is empty). Like [`Heap::get`], a reference to another kind
    /// of block reads as empty; a freed block is an error.
    pub fn get_u8(&self, r: StrRef) -> Result<&[u8], BadRef> {
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

    /// `HashStr` of the string `r` (0 is empty; `docs/spec/bytecode.md` 2.21): the hash its block
    /// remembers, or else worked out from its code units and remembered.
    pub fn str_hash(&mut self, r: StrRef, keys: &HashKeys) -> Result<u32, BadRef> {
        self.text_hash(r, keys, |body| match body {
            Body::Str(s) => Some(keys.units(s.iter_units())),
            _ => None,
        })
    }

    /// `HashUtf8` of the `UTF8String` `r` (0 is empty), remembered as by [`Heap::str_hash`].
    pub fn u8_hash(&mut self, r: StrRef, keys: &HashKeys) -> Result<u32, BadRef> {
        self.text_hash(r, keys, |body| match body {
            Body::U8(b) => Some(keys.bytes(b)),
            _ => None,
        })
    }

    /// The hash the text block `r` remembers, or else `hash` of its body remembered (`None` for a
    /// block of another kind, which reads as empty as in [`Heap::get`]; nothing is remembered). A
    /// remembered 0 cannot be told from none, so a text hashing to 0 is worked out each time
    /// (`docs/design/vm.md` 3.18).
    #[inline(always)]
    fn text_hash(
        &mut self,
        r: StrRef,
        keys: &HashKeys,
        hash: impl FnOnce(&Body) -> Option<u32>,
    ) -> Result<u32, BadRef> {
        let Some(block) = usize::try_from(r - 1)
            .ok()
            .and_then(|i| self.blocks.get_mut(i))
        else {
            return Ok(keys.empty_text);
        };
        let Some(body) = &block.body else {
            return Err(BadRef {
                message: format!("read of a freed heap block {r}"),
            });
        };
        if block.hash != 0 {
            return Ok(block.hash);
        }
        Ok(match hash(body) {
            Some(h) => {
                block.hash = h;
                h
            }
            None => keys.empty_text,
        })
    }

    /// The live block a non-zero reference points to (for reading).
    fn block(&self, r: StrRef, what: &str) -> Result<&Block, BadRef> {
        let block = slab_index(r)
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

    /// Whether the class `a` is `b` or a descendant of it (`docs/spec/objects.md` chapter 5,
    /// 14.3): one look into `a`'s ancestors at `b`'s depth (`docs/design/objects.md` 3.4).
    pub fn class_inherits(&self, a: u32, b: u32) -> bool {
        let ancestors = |t: u32| self.ancestors.get(t as usize).map_or(&[][..], |l| &l[..]);
        let depth = ancestors(b).len().wrapping_sub(1);
        ancestors(a).get(depth) == Some(&b)
    }

    /// Whether the class `class` implements the interface `intf`: its interface table has an entry
    /// for it (`docs/spec/objects.md` 7.3, 14.7).
    pub fn class_implements(&self, class: u32, intf: u32) -> bool {
        self.type_entry(class)
            .is_some_and(|e| e.intf_entry(intf).is_some())
    }

    /// A block of type `ty` with every slot at its default value (0), holding one reference.
    pub fn alloc_default(&mut self, ty: u32) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        let entry = self.type_entry(ty).ok_or_else(|| BadRef {
            message: format!("type {ty} out of range"),
        })?;
        // A big set holds its bits in words of 64 (`docs/spec/types.md` 8.3); a static array
        // its elements times their width
        let n = match entry.kind {
            TYPE_SET => set_words(entry.slots),
            TYPE_ARRAY => entry.slots as usize * entry.elem_width(),
            _ => entry.slots as usize,
        };
        if !self.fits(n * 8) {
            return Ok(Err(OutOfMemory));
        }
        let mut slots = Vec::new();
        if try_reserve(&mut slots, n).is_err() {
            return Ok(Err(OutOfMemory));
        }
        slots.resize(n, 0);
        Ok(self.alloc(Body::Agg { ty, slots }))
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
        if !self.fits(n * 8) {
            return Ok(Err(OutOfMemory));
        }
        let mut slots = Vec::new();
        if try_reserve(&mut slots, n).is_err() {
            return Ok(Err(OutOfMemory));
        }
        slots.resize(n, 0);
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
    pub fn alloc_agg(&mut self, ty: u32, mut slots: Vec<i64>) -> Result<StrRef, OutOfMemory> {
        slots.shrink_to_fit();
        self.alloc(Body::Agg { ty, slots })
    }

    /// The type and slots of a record or static array block.
    fn agg(&self, r: StrRef, what: &str) -> Result<(u32, &[i64]), BadRef> {
        match &self.block(r, what)?.body {
            Some(Body::Agg { ty, slots }) => Ok((*ty, slots)),
            _ => Err(Self::bad(what, r)),
        }
    }

    /// The type of a record, static array or dynamic array block (a dynamic array of bytes
    /// included).
    pub fn agg_type(&self, r: StrRef) -> Result<u32, BadRef> {
        match &self.block(r, "read")?.body {
            Some(Body::Agg { ty, .. } | Body::Bytes { ty, .. }) => Ok(*ty),
            _ => Err(Self::bad("read", r)),
        }
    }

    /// Slot `i` of a record, static array or dynamic array (`None` if `i` is outside the block);
    /// an element of a dynamic array of bytes reads as its value.
    pub fn slot(&self, r: StrRef, i: i64) -> Result<Option<i64>, BadRef> {
        let i = usize::try_from(i).ok();
        match &self.block(r, "read")?.body {
            Some(Body::Agg { slots, .. }) => Ok(i.and_then(|i| slots.get(i)).copied()),
            Some(Body::Bytes { bytes, .. }) => {
                Ok(i.and_then(|i| bytes.get(i)).map(|b| i64::from(*b)))
            }
            _ => Err(Self::bad("read", r)),
        }
    }

    /// The bytes of a dynamic array of bytes (0 is empty), as an extension borrows them
    /// (`docs/design/extensions.md` 3.5).
    pub fn bytes_of(&self, r: StrRef) -> Result<&[u8], BadRef> {
        if r == 0 {
            return Ok(&[]);
        }
        match &self.block(r, "read")?.body {
            Some(Body::Bytes { bytes, .. }) => Ok(bytes),
            _ => Err(Self::bad("read", r)),
        }
    }

    /// The bytes of the dynamic array of bytes `r` (0 is empty) to write in place (`BytesCopy` /
    /// `BytesFill`; written while shared, as [`Heap::slot_mut`]). An error for any other block.
    pub fn bytes_of_mut(&mut self, r: StrRef) -> Result<&mut [u8], BadRef> {
        if r == 0 {
            return Ok(&mut []);
        }
        match &mut self.block_mut(r, "write")?.body {
            Some(Body::Bytes { bytes, .. }) => Ok(bytes),
            _ => Err(Self::bad("write", r)),
        }
    }

    /// Copies the bytes `from..from + count` of the dynamic array of bytes `src` to `to` in `dst`
    /// (`BytesCopy`; the two may be the same block, the ranges may overlap). The ranges are in
    /// the arrays (the caller checked them).
    pub fn copy_bytes(
        &mut self,
        dst: StrRef,
        to: usize,
        src: StrRef,
        from: usize,
        count: usize,
    ) -> Result<(), BadRef> {
        if count == 0 {
            return Ok(());
        }
        if dst == src {
            self.bytes_of_mut(dst)?.copy_within(from..from + count, to);
            return Ok(());
        }
        // Two distinct live blocks: take the source's buffer out while the destination is written
        let taken = match &mut self.block_mut(src, "read")?.body {
            Some(Body::Bytes { bytes, .. }) => std::mem::take(bytes),
            _ => return Err(Self::bad("read", src)),
        };
        let written = self
            .bytes_of_mut(dst)
            .map(|d| d[to..to + count].copy_from_slice(&taken[from..from + count]));
        if let Some(Body::Bytes { bytes, .. }) = &mut self.block_mut(src, "read")?.body {
            *bytes = taken;
        }
        written
    }

    /// A dynamic array of bytes of type `ty` that takes `bytes` as its buffer, without copying
    /// (what an extension gives back; 0 when empty).
    pub fn alloc_bytes_owned(&mut self, ty: u32, bytes: Vec<u8>) -> Result<StrRef, OutOfMemory> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if bytes.len() > MAX_STRING_LEN {
            return Err(OutOfMemory);
        }
        let body = Body::Bytes { ty, bytes };
        if !self.fits(body.size()) {
            return Err(OutOfMemory);
        }
        Ok(self.place(body))
    }

    /// The slots of an array block (0 is an empty dynamic array): [`Heap::len`] and
    /// [`Heap::slot`] with one look at the block.
    pub fn elems(&self, r: StrRef) -> Result<&[i64], BadRef> {
        if r == 0 {
            return Ok(&[]);
        }
        self.agg(r, "read").map(|(_, s)| s)
    }

    /// The elements of the dynamic arrays `refs` (0 is empty) whose elements take one slot each,
    /// for the hash table instructions (`docs/spec/bytecode.md` 2.23); anything else is an error
    /// naming the array by its entry of `whats`. One look at each block and at the table of the
    /// types that qualify ([`Heap::hash_types`]), so that an instruction checks all its arrays
    /// at once.
    pub fn hash_arrays<const N: usize>(
        &self,
        refs: [StrRef; N],
        whats: [&str; N],
    ) -> Result<[&[i64]; N], BadRef> {
        let mut out: [&[i64]; N] = [&[]; N];
        for (k, &r) in refs.iter().enumerate() {
            if r == 0 {
                continue;
            }
            match self.hash_slots(r) {
                Some(slots) => out[k] = slots,
                None => return Err(self.hash_array_error(r, whats[k])),
            }
        }
        Ok(out)
    }

    /// Element `i` of the dynamic or open array `r` (0 is empty) whose elements take one slot
    /// each (`DynGet`, `BytesGet`), or, out of range, `Err` with the number of elements. One look
    /// at the block.
    pub fn elem(&self, r: StrRef, i: i64) -> Result<Result<i64, usize>, BadRef> {
        if r == 0 {
            return Ok(Err(0));
        }
        let at = usize::try_from(i).ok();
        Ok(match &self.block(r, "read")?.body {
            Some(Body::Agg { slots, .. }) => {
                at.and_then(|i| slots.get(i)).copied().ok_or(slots.len())
            }
            Some(Body::Bytes { bytes, .. }) => at
                .and_then(|i| bytes.get(i))
                .map(|b| i64::from(*b))
                .ok_or(bytes.len()),
            _ => return Err(Self::bad("read", r)),
        })
    }

    /// The keys of a hash table (`HashLookupInt`, `HashLookupStr`; `docs/spec/bytecode.md` 2.23):
    /// the elements of the dynamic array `r` of one-slot elements or of bytes (0 is empty);
    /// anything else is an error.
    pub fn hash_keys(&self, r: StrRef) -> Result<Elems<'_>, BadRef> {
        if r == 0 {
            return Ok(Elems::Slots(&[]));
        }
        if let Some(slots) = self.hash_slots(r) {
            return Ok(Elems::Slots(slots));
        }
        match self.block(r, "read").map(|b| &b.body) {
            Ok(Some(Body::Bytes { bytes, .. })) => Ok(Elems::Bytes(bytes)),
            _ => Err(self.hash_array_error(r, "the keys")),
        }
    }

    /// The slots of the block `r` (not 0) if it is a dynamic array of one-slot elements.
    #[inline]
    fn hash_slots(&self, r: StrRef) -> Option<&[i64]> {
        if (r as u64) >> 32 != 0 {
            return None;
        }
        match self.blocks.get(slab_index(r)?)?.body.as_ref()? {
            Body::Agg { ty, slots } if self.hash_types.get(*ty as usize) == Some(&true) => {
                Some(slots)
            }
            _ => None,
        }
    }

    /// The error of [`Heap::hash_arrays`] for the block `r` that is not a dynamic array of
    /// one-slot elements: an invalid or freed reference as [`Heap::block`] names it, otherwise
    /// the kind of the block.
    #[cold]
    fn hash_array_error(&self, r: StrRef, what: &str) -> BadRef {
        let bad = || BadRef {
            message: format!(
                "{what} of a hash table: heap block {r} is not a dynamic array of one-slot elements"
            ),
        };
        if (r as u64) >> 32 != 0 {
            return bad();
        }
        match self.block(r, "read") {
            Err(e) => e,
            Ok(_) => bad(),
        }
    }

    /// The string `r` (0 is empty) for the key comparison of `HashLookupStr`
    /// (`docs/spec/bytecode.md` 2.23); a block that is not a string is an error (unlike
    /// [`Heap::get`]).
    pub fn hash_key_str(&self, r: StrRef) -> Result<&PStr, BadRef> {
        if r == 0 {
            return Ok(&self.empty);
        }
        match &self.block(r, "read")?.body {
            Some(Body::Str(s)) => Ok(s),
            _ => Err(BadRef {
                message: format!("key of a hash table: heap block {r} is not a string"),
            }),
        }
    }

    /// `HashRelink` (`docs/spec/bytecode.md` 2.23): sets every bucket of `buckets` to -1, then
    /// links the first `n` entries that are not holes (their `next` is not -2) into the chains
    /// of their buckets, writing `buckets` and `next` in place (while shared, as a dynamic array
    /// is written). Returns the number of entries linked. The three arrays must be distinct
    /// blocks.
    pub fn hash_relink(
        &mut self,
        buckets: StrRef,
        hashes: StrRef,
        next: StrRef,
        n: i64,
    ) -> Result<usize, BadRef> {
        let fail = |message: String| Err(BadRef { message });
        let lens = self
            .hash_arrays(
                [buckets, hashes, next],
                ["the buckets", "the hashes", "the next entries"],
            )?
            .map(<[i64]>::len);
        if lens[0] != 0 && !lens[0].is_power_of_two() {
            return fail(format!(
                "the buckets of a hash table: length {} is not a power of 2",
                lens[0]
            ));
        }
        let Some(n) = usize::try_from(n)
            .ok()
            .filter(|n| *n <= lens[1].min(lens[2]))
        else {
            return fail(format!(
                "`HashRelink` of {n} entries out of range 0..{}",
                lens[1].min(lens[2])
            ));
        };
        let distinct = [buckets, hashes, next];
        for (i, a) in distinct.iter().enumerate() {
            if *a != 0 && distinct[i + 1..].contains(a) {
                return fail(format!("`HashRelink` of the heap block {a} twice"));
            }
        }
        // The two arrays written are taken out of their blocks while the chains are made (the
        // blocks are distinct, so the hashes are read from their own), and put back after
        let take = |heap: &mut Heap, r: StrRef| -> Vec<i64> {
            if r == 0 {
                return Vec::new();
            }
            match heap.blocks[slab_index(r).expect("checked above")]
                .body
                .as_mut()
            {
                Some(Body::Agg { slots, .. }) => std::mem::take(slots),
                _ => unreachable!("checked above"),
            }
        };
        let put = |heap: &mut Heap, r: StrRef, v: Vec<i64>| {
            if r != 0
                && let Some(Body::Agg { slots, .. }) = heap.blocks
                    [slab_index(r).expect("checked above")]
                .body
                .as_mut()
            {
                *slots = v;
            }
        };
        let mut b = take(self, buckets);
        let mut x = take(self, next);
        let [h] = self.hash_arrays([hashes], ["the hashes"])?;
        b.fill(-1);
        let mut linked = 0;
        let mut result: Result<(), BadRef> = Ok(());
        for i in 0..n {
            if x[i] == HASH_HOLE {
                continue;
            }
            if b.is_empty() {
                result = Err(BadRef {
                    message: "`HashRelink` of an entry with no bucket".into(),
                });
                break;
            }
            let k = (h[i] as u64 & (b.len() as u64 - 1)) as usize;
            x[i] = b[k];
            b[k] = i as i64;
            linked += 1;
        }
        put(self, buckets, b);
        put(self, next, x);
        result.map(|()| linked)
    }

    /// `ArraySortRange` (`I96`; `docs/spec/bytecode.md` 2.22): sorts the elements `lo..hi` of the
    /// dynamic array `r` of one-slot elements stably by `kind` (`bytecode::SORT_INT` ...), in
    /// place (while shared, as a dynamic array is written; the values only change places, so no
    /// count changes). Returns the number of comparisons, or `OutOfMemory` (nothing moved) when
    /// the room of the merges (`hi - lo` slots, counted as a Pascal value against the usage
    /// limit while the sort runs) cannot be had. The caller has checked the kind (one of
    /// `SORT_INT..=SORT_UTF8`; anything past `SORT_STR` compares as `SORT_UTF8`), the range and
    /// the loans; an element that is not a string of the kind is an internal error.
    pub fn sort_slots(
        &mut self,
        r: StrRef,
        lo: usize,
        hi: usize,
        kind: i64,
    ) -> Result<Result<u64, OutOfMemory>, BadRef> {
        use crate::bytecode::{SORT_DOUBLE, SORT_INT, SORT_SINGLE, SORT_STR, SORT_UINT, SORT_UTF8};
        let index = slab_index(r).expect("the caller checked the array");
        if let Some(Body::Bytes { bytes, .. }) = self.blocks[index].body.as_mut() {
            return Self::sort_bytes(bytes, r, lo, hi, kind).map(Ok);
        }
        let elems = match &self.blocks[index].body {
            Some(Body::Agg { slots, .. }) => slots.get(lo..hi),
            _ => None,
        }
        .ok_or_else(|| BadRef {
            message: format!("`ArraySortRange` of the slots {lo}..{hi} of heap block {r}"),
        })?;
        if kind == SORT_STR || kind == SORT_UTF8 {
            for &x in elems {
                let fits = x == 0
                    || (x as u64) >> 32 == 0
                        && matches!(
                            (kind, &self.block(x, "read")?.body),
                            (SORT_STR, Some(Body::Str(_))) | (SORT_UTF8, Some(Body::U8(_)))
                        );
                if !fits {
                    return Err(BadRef {
                        message: format!(
                            "`ArraySortRange` of heap block {r}: element {x} is not a {}",
                            if kind == SORT_STR {
                                "string"
                            } else {
                                "`UTF8String`"
                            }
                        ),
                    });
                }
            }
        }
        let n = hi - lo;
        let size = n * std::mem::size_of::<i64>();
        let mut work = Vec::new();
        if !self.fits(size) || try_reserve(&mut work, n).is_err() {
            return Ok(Err(OutOfMemory));
        }
        work.resize(n, 0);
        // The slots are taken out of their block while they are sorted (the strings they point to
        // are other blocks, read in place), and put back after
        let Some(Body::Agg { slots, .. }) = self.blocks[index].body.as_mut() else {
            unreachable!("checked above");
        };
        let mut slots = std::mem::take(slots);
        self.change_used(0, size);
        let v = &mut slots[lo..hi];
        let count = match kind {
            SORT_INT => sort::merge_sort(v, &mut work, |a, b| a.cmp(&b)),
            SORT_UINT => sort::merge_sort(v, &mut work, |a, b| (a as u64).cmp(&(b as u64))),
            SORT_SINGLE => sort::merge_sort(v, &mut work, |a, b| {
                sort::cmp_f32(f32::from_bits(a as u32), f32::from_bits(b as u32))
            }),
            SORT_DOUBLE => sort::merge_sort(v, &mut work, |a, b| {
                sort::cmp_f64(f64::from_bits(a as u64), f64::from_bits(b as u64))
            }),
            // The same comparisons as `CmpStr` and `CmpU8`; the elements were checked above
            SORT_STR => sort::merge_sort(v, &mut work, |a, b| {
                let s = |x| self.get(x).expect("checked above");
                s(a).compare(s(b))
            }),
            // `SORT_UTF8`
            _ => sort::merge_sort(v, &mut work, |a, b| {
                let s = |x| self.get_u8(x).expect("checked above");
                s(a).cmp(s(b))
            }),
        };
        self.used -= size;
        if let Some(Body::Agg { slots: back, .. }) = self.blocks[index].body.as_mut() {
            *back = slots;
        }
        Ok(Ok(count))
    }

    /// [`Heap::sort_slots`] of a dynamic array of bytes: the bytes compare as integers whether
    /// signed or not (`SORT_INT`, `SORT_UINT`; another kind is an internal error), and equal ones
    /// cannot be told apart, so a counting sort is stable. Returns the number of elements as the
    /// number of comparisons (the work is linear).
    fn sort_bytes(
        bytes: &mut [u8],
        r: StrRef,
        lo: usize,
        hi: usize,
        kind: i64,
    ) -> Result<u64, BadRef> {
        use crate::bytecode::{SORT_INT, SORT_UINT};
        if kind != SORT_INT && kind != SORT_UINT {
            return Err(BadRef {
                message: format!("`ArraySortRange` of the bytes of heap block {r} by kind {kind}"),
            });
        }
        let range = bytes.get_mut(lo..hi).ok_or_else(|| BadRef {
            message: format!("`ArraySortRange` of the slots {lo}..{hi} of heap block {r}"),
        })?;
        let mut counts = [0usize; 256];
        for b in range.iter() {
            counts[*b as usize] += 1;
        }
        let mut at = 0;
        for (b, c) in counts.iter().enumerate() {
            range[at..at + c].fill(b as u8);
            at += c;
        }
        Ok((hi - lo) as u64)
    }

    /// The slots of the live record or array block `r` for the fast paths of the slot reads and
    /// writes (`DynGet` / `FieldGet` / `ValGet`, the `Set` forms and the counted forms), or `None`
    /// when `r` is not a live record or array block, one of its slots is lent (only then does
    /// [`Heap::check_slot`] refuse a slot of it), or (`write`) the block is shared and not a
    /// dynamic array (the body reports why).
    #[inline(always)]
    fn elems_fast(&mut self, r: StrRef, write: bool) -> Option<&mut [i64]> {
        // An instance's handle (a cursor's container may be one) has its generation above
        if (r as u64) >> 32 != 0 {
            return None;
        }
        let index = slab_index(r)?;
        let block = self.blocks.get(index)?;
        if block.lent != 0 {
            return None;
        }
        if write && block.rc != 1 {
            // Only a dynamic array is written while shared (`Heap::slot_mut`)
            let Some(Body::Agg { ty, .. }) = &block.body else {
                return None;
            };
            if !self.is_dyn_type(*ty) {
                return None;
            }
        }
        match self.blocks[index].body.as_mut()? {
            Body::Agg { slots, .. } => Some(slots),
            _ => None,
        }
    }

    /// The bytes of the live dynamic array of bytes `r` for the fast paths of `BytesGet` /
    /// `BytesSet` (written while shared, as a dynamic array is), or `None` when `r` is not one or
    /// one of its elements is lent (the body reports why, and reads any other array).
    #[inline(always)]
    fn bytes_fast(&mut self, r: StrRef) -> Option<&mut Vec<u8>> {
        // An instance's handle has its generation above
        if (r as u64) >> 32 != 0 {
            return None;
        }
        let block = self.blocks.get_mut(slab_index(r)?)?;
        if block.lent != 0 {
            return None;
        }
        match block.body.as_mut()? {
            Body::Bytes { bytes, .. } => Some(bytes),
            _ => None,
        }
    }

    /// Element `i` of the dynamic array of bytes `r` (`BytesGet`), or `None` as
    /// [`Heap::bytes_fast`] says or when `i` is out of range.
    #[inline(always)]
    pub fn bytes_get_fast(&mut self, r: StrRef, i: i64) -> Option<i64> {
        self.bytes_fast(r)?
            .get(usize::try_from(i).ok()?)
            .map(|b| i64::from(*b))
    }

    /// Writes the low 8 bits of `v` to element `i` of the dynamic array of bytes `r`
    /// (`BytesSet`), or `None` with nothing written as [`Heap::bytes_get_fast`] says.
    #[inline(always)]
    pub fn bytes_set_fast(&mut self, r: StrRef, i: i64, v: i64) -> Option<()> {
        *self.bytes_fast(r)?.get_mut(usize::try_from(i).ok()?)? = v as u8;
        Some(())
    }

    /// `ElemAt` on the fast path: the first slot of element `i` of the array block `r`, or `None`
    /// when `r` is not a plain array block or `i` is out of range.
    #[inline(always)]
    pub fn elem_at_fast(&self, r: StrRef, i: i64) -> Option<i64> {
        let index = self.plain_index(r)?;
        let Some(Body::Agg { ty, slots }) = &self.blocks.get(index)?.body else {
            return None;
        };
        let t = self.types.get(*ty as usize)?;
        if !matches!(t.kind, TYPE_ARRAY | TYPE_DYNARRAY) {
            return None;
        }
        let w = t.elem_width();
        let i = usize::try_from(i).ok().filter(|i| *i < slots.len() / w)?;
        Some((i * w) as i64)
    }

    /// Element `i` of the array `r` (`DynGet`), or `None` as [`Heap::elems_fast`] says or when `i`
    /// is out of range.
    #[inline(always)]
    pub fn dyn_get_fast(&mut self, r: StrRef, i: i64) -> Option<i64> {
        self.elems_fast(r, false)?
            .get(usize::try_from(i).ok()?)
            .copied()
    }

    /// Writes `v` to element `i` of the array `r` (`DynSet` of an unmanaged element), or `None`
    /// with nothing written as [`Heap::dyn_get_fast`] says.
    #[inline(always)]
    pub fn dyn_set_fast(&mut self, r: StrRef, i: i64, v: i64) -> Option<()> {
        *self
            .elems_fast(r, true)?
            .get_mut(usize::try_from(i).ok()?)? = v;
        Some(())
    }

    /// Puts the managed value `v` in slot `i` of the record or array `r` (`DynSetRef`,
    /// `FieldSetRef`, `ValSetRef`), counting it and letting go of the old value, or `None` with
    /// nothing changed when [`Heap::dyn_set_fast`] or [`Heap::assign_ref_fast`] would refuse.
    #[inline(always)]
    pub fn slot_set_ref_fast(&mut self, r: StrRef, i: i64, v: i64) -> Option<()> {
        let i = usize::try_from(i).ok()?;
        let old = *self.elems_fast(r, true)?.get(i)?;
        if !self.assign_ref_fast(v, old) {
            return None;
        }
        // The counts changed the blocks' counts only, so the slot is still there and writable
        let slot = self
            .elems_fast(r, true)
            .and_then(|s| s.get_mut(i))
            .expect("the slot was checked above");
        *slot = v;
        Some(())
    }

    /// `Unique` of `r` when it is already the only reference to a live record or array (the fast
    /// path): nothing to do, as in [`Heap::unique`] (only copying a shared block looks at the
    /// lending table). `false` otherwise.
    #[inline(always)]
    pub fn unique_fast(&self, r: StrRef) -> bool {
        self.plain_index(r).is_some_and(|i| self.blocks[i].rc == 1)
    }

    /// The number of slots of a record or array (the elements of a dynamic array of bytes).
    pub fn slot_count(&self, r: StrRef) -> Result<usize, BadRef> {
        match &self.block(r, "read")?.body {
            Some(Body::Agg { slots, .. }) => Ok(slots.len()),
            Some(Body::Bytes { bytes, .. }) => Ok(bytes.len()),
            _ => Err(Self::bad("read", r)),
        }
    }

    /// Whether a type is a dynamic array of bytes ([`TypeEntry::is_dyn_bytes`]).
    pub fn is_bytes_type(&self, ty: u32) -> bool {
        self.types
            .get(ty as usize)
            .is_some_and(TypeEntry::is_dyn_bytes)
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

    /// Writes `v` into slot `i` of a record, static array or dynamic array as
    /// [`Heap::slot_mut`] allows it, and returns the old value. An element of a dynamic array of
    /// bytes keeps the low 8 bits of `v` (the compiler wrote a `Byte`).
    pub fn slot_replace(&mut self, r: StrRef, i: i64, v: i64) -> Result<i64, BadRef> {
        if let Some(Body::Bytes { bytes, .. }) = &mut self.block_mut(r, "write")?.body {
            let at = usize::try_from(i).map_err(|_| slot_out_of_range(r, i))?;
            return replace_byte(bytes, r, at, v);
        }
        Ok(std::mem::replace(self.slot_mut(r, i)?, v))
    }

    /// Rotates the slots `lo..hi` of the dynamic array `r` (written while shared, as
    /// [`Heap::slot_mut`]) by `by` places to the left (`left`) or to the right (`ArrayMoveRange`;
    /// `docs/spec/bytecode.md` 2.22). The values only change places, so no count changes. The
    /// caller has checked the range; a block that is not a dynamic array is an internal error.
    pub fn rotate_slots(
        &mut self,
        r: StrRef,
        lo: usize,
        hi: usize,
        by: usize,
        left: bool,
    ) -> Result<(), BadRef> {
        let ty = self.agg_type(r)?;
        if !self.is_dyn_type(ty) {
            return Err(Self::bad("rotate", r));
        }
        fn rotate<T>(
            range: Option<&mut [T]>,
            r: StrRef,
            lo: usize,
            hi: usize,
            by: usize,
            left: bool,
        ) -> Result<(), BadRef> {
            let range = range.ok_or_else(|| BadRef {
                message: format!("slots {lo}..{hi} out of range in heap block {r}"),
            })?;
            if left {
                range.rotate_left(by);
            } else {
                range.rotate_right(by);
            }
            Ok(())
        }
        match &mut self.block_mut(r, "write")?.body {
            Some(Body::Agg { slots, .. }) => rotate(slots.get_mut(lo..hi), r, lo, hi, by, left),
            Some(Body::Bytes { bytes, .. }) => rotate(bytes.get_mut(lo..hi), r, lo, hi, by, left),
            _ => Err(Self::bad("rotate", r)),
        }
    }

    // ---- Flat places (`docs/spec/bytecode.md` 2.8; `docs/design/records.md` 3.4) ----

    /// The type and slots of the live record, array or instance block `r` (an instance by its
    /// slab index + 1).
    pub fn any_slots(&self, r: StrRef) -> Result<(u32, &[i64]), BadRef> {
        match &self.block(r, "read")?.body {
            Some(Body::Agg { ty, slots }) => Ok((*ty, slots)),
            Some(Body::Obj { ty, slots, .. }) => Ok((*ty, slots)),
            _ => Err(Self::bad("read", r)),
        }
    }

    /// The type and number of slots of the live record, array or instance block `r` (the
    /// elements of a dynamic array of bytes).
    pub fn any_shape(&self, r: StrRef) -> Result<(u32, usize), BadRef> {
        match &self.block(r, "read")?.body {
            Some(Body::Bytes { ty, bytes }) => Ok((*ty, bytes.len())),
            _ => self.any_slots(r).map(|(t, s)| (t, s.len())),
        }
    }

    /// Slot `i` of the live record, array or instance block `r` (an element of a dynamic array
    /// of bytes reads as its value).
    pub fn any_slot(&self, r: StrRef, i: usize) -> Result<i64, BadRef> {
        let v = match &self.block(r, "read")?.body {
            Some(Body::Bytes { bytes, .. }) => bytes.get(i).map(|b| i64::from(*b)),
            _ => self.any_slots(r)?.1.get(i).copied(),
        };
        v.ok_or_else(|| slot_out_of_range(r, i))
    }

    /// Writes `v` into slot `i` of the live record, array or instance block `r` as
    /// [`Heap::any_slots_mut`] allows it, and returns the old value (an element of a dynamic
    /// array of bytes keeps the low 8 bits of `v`, as [`Heap::slot_replace`]).
    pub fn any_replace(&mut self, r: StrRef, i: usize, v: i64) -> Result<i64, BadRef> {
        if let Some(Body::Bytes { bytes, .. }) = &mut self.block_mut(r, "write")?.body {
            return replace_byte(bytes, r, i, v);
        }
        Ok(std::mem::replace(self.any_slot_mut(r, i)?, v))
    }

    /// The slots of the live record, array or instance block `r` for writing: a record or static
    /// array must hold its only reference (made unique by `Unique`), as for [`Heap::slot_mut`];
    /// a dynamic array and an instance are written while shared.
    pub fn any_slots_mut(&mut self, r: StrRef) -> Result<&mut [i64], BadRef> {
        let (ty, _) = self.any_slots(r)?;
        let shared_ok = self.is_dyn_type(ty);
        let block = self.block_mut(r, "write")?;
        match &mut block.body {
            Some(Body::Agg { slots, .. }) if block.rc == 1 || shared_ok => Ok(slots),
            Some(Body::Obj { slots, .. }) => Ok(slots),
            Some(Body::Agg { .. }) => Err(BadRef {
                message: format!("write to the shared heap block {r}"),
            }),
            _ => Err(Self::bad("write", r)),
        }
    }

    /// Slot `i` of the live record, array or instance block `r` for writing
    /// ([`Heap::any_slots_mut`]).
    pub fn any_slot_mut(&mut self, r: StrRef, i: usize) -> Result<&mut i64, BadRef> {
        self.any_slots_mut(r)?.get_mut(i).ok_or_else(|| BadRef {
            message: format!("slot {i} out of range in heap block {r}"),
        })
    }

    /// The block the container of a cursor names (`docs/spec/bytecode.md` 2.8): `None` for 0 (a
    /// record or array at its default value), the block of a live record or array, or the block
    /// (slab index + 1) of the live instance a handle refers to (checked against its generation,
    /// so a destroyed instance is refused).
    pub fn cursor_block(&self, c: i64) -> Result<Option<StrRef>, BadHandle> {
        if c == 0 {
            return Ok(None);
        }
        let plain = (c as u64) >> 32 == 0
            && matches!(
                slab_index(c).and_then(|i| self.blocks.get(i)),
                Some(Block {
                    body: Some(Body::Agg { .. }),
                    ..
                })
            );
        if plain {
            return Ok(Some(c));
        }
        // An instance, or a block that is no record or array: `obj_index` says which
        Ok(Some(self.obj_index(c)? as StrRef + 1))
    }

    /// Whether `r` is a record or static array block of type `ty` that only one register holds
    /// and that contains no lent location, so `AggLoad` may write a value into it in place of
    /// making another (`docs/design/records.md` 3.5 B).
    pub fn reusable_box(&self, r: StrRef, ty: u32) -> bool {
        self.plain_index(r).is_some_and(|i| {
            let block = &self.blocks[i];
            block.rc == 1
                && block.lent == 0
                && matches!(&block.body, Some(Body::Agg { ty: t, .. }) if *t == ty)
        })
    }

    /// The kind of slot `i` of a block of type `ty` ([`SLOT_PLAIN`], [`SLOT_REF`], [`SLOT_OBJ`]).
    pub fn slot_kind(&self, ty: u32, i: usize) -> u8 {
        self.types
            .get(ty as usize)
            .map_or(SLOT_PLAIN, |t| t.slot_kind(i))
    }

    /// Counts a value held in a slot of kind `kind`.
    pub fn add_ref_kind(&mut self, v: i64, kind: u8) -> Result<(), BadRef> {
        match kind {
            SLOT_REF => self.add_ref(v),
            SLOT_OBJ => self.add_ref_obj(v),
            _ => Ok(()),
        }
    }

    /// Lets go of a value held in a slot of kind `kind`.
    pub fn release_kind(&mut self, v: i64, kind: u8) -> Result<(), BadRef> {
        match kind {
            SLOT_REF => self.release(v),
            SLOT_OBJ => self.release_obj(v),
            _ => Ok(()),
        }
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
        if let Some(Body::Bytes { ty, bytes }) = &self.block(r, "copy")?.body {
            // A value open array parameter of bytes: the copy holds no references
            let copy = match self.new_bytes(*ty, bytes, bytes.len()) {
                Ok(body) => self.place(body),
                Err(e) => return Ok(Err(e)),
            };
            self.release(r)?;
            return Ok(Ok(copy));
        }
        let (bty, slots) = self.agg(r, "copy")?;
        let slots = slots.to_vec();
        if !self.fits(slots.len() * 8) {
            return Ok(Err(OutOfMemory));
        }
        for (i, v) in slots.iter().enumerate() {
            self.add_ref_kind(*v, self.slot_kind(bty, i))?;
        }
        let copy = match self.alloc_agg(bty, slots.clone()) {
            Ok(c) => c,
            Err(e) => {
                for (i, v) in slots.iter().enumerate() {
                    self.release_kind(*v, self.slot_kind(bty, i))?;
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
        let ty = self.agg_type(r)?;
        if self.is_dyn_type(ty) {
            return Ok(Ok(r));
        }
        self.unique(r, ty)
    }

    /// The number of elements of an array block (0 is an empty dynamic array): its slots over
    /// the width of an element.
    pub fn len(&self, r: StrRef) -> Result<usize, BadRef> {
        if r == 0 {
            return Ok(0);
        }
        match &self.block(r, "read")?.body {
            Some(Body::Agg { ty, slots }) => Ok(slots.len() / self.width_of(*ty)),
            Some(Body::Bytes { bytes, .. }) => Ok(bytes.len()),
            _ => Err(Self::bad("read", r)),
        }
    }

    /// The number of slots an element of an array of type `ty` takes (1 for any other type).
    #[inline]
    pub fn width_of(&self, ty: u32) -> usize {
        self.types.get(ty as usize).map_or(1, TypeEntry::elem_width)
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
        if self.is_bytes_type(ty) {
            return self.alloc_bytes(ty, &[], n);
        }
        self.alloc_slots(ty, &[], n * self.width_of(ty))
    }

    /// A dynamic array of bytes of type `ty` with a copy of `prefix`, padded with 0 to `n`
    /// elements. Checks the usage limit before allocating.
    fn alloc_bytes(&mut self, ty: u32, prefix: &[u8], n: usize) -> Result<StrRef, OutOfMemory> {
        let body = self.new_bytes(ty, prefix, n)?;
        Ok(self.place(body))
    }

    /// The body of a dynamic array of bytes of type `ty`: a copy of `prefix` (which may be the
    /// bytes of a block of this heap) padded with 0 to `n` elements, made only when it fits in the
    /// usage limit. The caller places it ([`Heap::place`]).
    fn new_bytes(&self, ty: u32, prefix: &[u8], n: usize) -> Result<Body, OutOfMemory> {
        if !self.fits(n + std::mem::size_of::<Body>()) {
            return Err(OutOfMemory);
        }
        Ok(Body::Bytes {
            ty,
            bytes: padded_bytes(prefix, n)?,
        })
    }

    /// A block of type `ty` with copies of the given slots, padded with defaults to `n` (each
    /// given slot is a value the block now owns). Checks the usage limit before allocating.
    fn alloc_slots(&mut self, ty: u32, prefix: &[i64], n: usize) -> Result<StrRef, OutOfMemory> {
        if !self.fits(n * 8) {
            return Err(OutOfMemory);
        }
        let mut slots = Vec::new();
        try_reserve(&mut slots, n)?;
        slots.extend_from_slice(prefix);
        slots.resize(n, 0);
        self.alloc_agg(ty, slots)
    }

    /// `SetLength` of a dynamic array of type `ty` (`docs/spec/types.md` 2.3) to `n` elements:
    /// returns the value that takes the place of `r`. A block holding its only reference is
    /// resized in place (dropped elements are released); a shared one is copied (the copy counts
    /// its elements, the original loses one reference). The length 0 is the empty array (0).
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
        if matches!(self.block(r, "resize")?.body, Some(Body::Bytes { .. })) {
            return self.set_bytes_len(r, n);
        }
        let (bty, old) = {
            let (t, s) = self.agg(r, "resize")?;
            (t, s.len())
        };
        // In slots (an element of a flat record takes several; `docs/spec/bytecode.md` 3.2)
        let n = n * self.width_of(bty);
        if self.block(r, "resize")?.rc == 1 {
            // Grown in place with capacity to spare, so that growing one element at a time is
            // amortized O(1) (`docs/spec/types.md` 9.1); shrunk in place, giving the capacity
            // back once less than a quarter of it is used
            let room = self.room();
            let resized: Result<(Vec<i64>, usize, usize), OutOfMemory> = {
                let block = self.block_mut(r, "resize")?;
                let Some(Body::Agg { slots, .. }) = &mut block.body else {
                    return Err(Self::bad("resize", r));
                };
                let before = slots.capacity();
                if n > old {
                    grow_within(slots, n - old, room).map(|()| {
                        slots.resize(n, 0);
                        (Vec::new(), before, slots.capacity())
                    })
                } else {
                    let dropped = slots.split_off(n);
                    if n < slots.capacity() / 4 {
                        slots.shrink_to_fit();
                    }
                    Ok((dropped, before, slots.capacity()))
                }
            };
            let Ok((mut dropped, before, after)) = resized else {
                return Ok(Err(OutOfMemory));
            };
            self.change_used(before * 8, after * 8);
            // A lent element dropped here is lent no more: an element grown back in its place
            // is a new one (`docs/spec/types.md` chapter 3). A lent flat place moves out first
            if n < old && self.block(r, "resize")?.lent != 0 {
                self.forget_lent_slots(r, n, &mut dropped);
            }
            for (k, v) in dropped.into_iter().enumerate() {
                self.release_kind(v, self.slot_kind(bty, n + k))?;
            }
            return Ok(Ok(r));
        }
        // The copy is made before its elements are counted, so a failure has nothing to give back
        let kept = match try_join(&[&self.agg(r, "copy")?.1[..old.min(n)]]) {
            Ok(k) => k,
            Err(e) => return Ok(Err(e)),
        };
        let copy = match self.alloc_slots(bty, &kept, n) {
            Ok(c) => c,
            Err(e) => return Ok(Err(e)),
        };
        for (i, v) in kept.into_iter().enumerate() {
            self.add_ref_kind(v, self.slot_kind(bty, i))?;
        }
        self.release(r)?;
        Ok(Ok(copy))
    }

    /// [`Heap::set_len`] of the dynamic array of bytes `r` (not 0) to `n` (not 0) elements, in
    /// place with capacity to spare when `r` holds its only reference, otherwise in a copy.
    fn set_bytes_len(
        &mut self,
        r: StrRef,
        n: usize,
    ) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        let block = self.block(r, "resize")?;
        let Some(Body::Bytes { ty, bytes }) = &block.body else {
            return Err(Self::bad("resize", r));
        };
        let (bty, old) = (*ty, bytes.len());
        if block.rc == 1 {
            let room = self.room();
            let resized: Result<(usize, usize), OutOfMemory> = {
                let Some(Body::Bytes { bytes, .. }) = &mut self.block_mut(r, "resize")?.body else {
                    unreachable!("checked above");
                };
                let before = bytes.capacity();
                if n > old {
                    grow_within(bytes, n - old, room).map(|()| {
                        bytes.resize(n, 0);
                        (before, bytes.capacity())
                    })
                } else {
                    bytes.truncate(n);
                    if n < bytes.capacity() / 4 {
                        bytes.shrink_to_fit();
                    }
                    Ok((before, bytes.capacity()))
                }
            };
            let Ok((before, after)) = resized else {
                return Ok(Err(OutOfMemory));
            };
            self.change_used(before, after);
            // A lent element dropped here is lent no more (as in `set_len`)
            if n < old && self.block(r, "resize")?.lent != 0 {
                self.forget_lent_slots(r, n, &mut []);
            }
            return Ok(Ok(r));
        }
        let copy = match &self.block(r, "copy")?.body {
            Some(Body::Bytes { bytes, .. }) => self.new_bytes(bty, &bytes[..old.min(n)], n),
            _ => unreachable!("checked above"),
        };
        let copy = match copy {
            Ok(body) => self.place(body),
            Err(e) => return Ok(Err(e)),
        };
        self.release(r)?;
        Ok(Ok(copy))
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
    /// themselves are only borrowed. A range is in elements of its array, whose element width
    /// must be that of `ty`; an element of an array whose elements are flat records or static
    /// arrays (width 2 or more) is the value of that record or array (0 is the default), whose
    /// slots are copied. An empty result is 0. A length above 2^31 - 1 is `OutOfMemory`, decided
    /// before anything is allocated.
    pub fn build_dyn(
        &mut self,
        ty: u32,
        pieces: &[Piece],
    ) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        let w = self.width_of(ty);
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
        if self.is_bytes_type(ty) {
            return self.build_bytes(ty, pieces, n);
        }
        let mut items: Vec<i64> = Vec::new();
        if try_reserve(&mut items, n * w).is_err() {
            return Ok(Err(OutOfMemory));
        }
        for p in pieces {
            match *p {
                Piece::Range(r, start, end) if end > start => {
                    if let Some(Body::Bytes { bytes, .. }) = &self.block(r, "copy")?.body {
                        // Bytes into an array of one-slot elements
                        if w != 1 {
                            return Err(BadRef {
                                message: format!("elements of heap block {r} have another width"),
                            });
                        }
                        items.extend(bytes[start..end].iter().map(|b| i64::from(*b)));
                        continue;
                    }
                    let (rty, slots) = self.agg(r, "copy")?;
                    if self.width_of(rty) != w {
                        return Err(BadRef {
                            message: format!("elements of heap block {r} have another width"),
                        });
                    }
                    items.extend_from_slice(&slots[start * w..end * w]);
                }
                Piece::Range(..) => {}
                Piece::Elem(v) if w == 1 => items.push(v),
                Piece::Elem(0) => items.resize(items.len() + w, 0),
                Piece::Elem(v) => {
                    let (_, slots) = self.agg(v, "copy")?;
                    if slots.len() != w {
                        return Err(BadRef {
                            message: format!("heap block {v} is not an element of width {w}"),
                        });
                    }
                    items.extend_from_slice(slots);
                }
            }
        }
        // The usage limit is checked before the elements are counted, so a failure has nothing
        // to give back
        let body = Body::Agg { ty, slots: items };
        if !self.fits(body.size()) {
            return Ok(Err(OutOfMemory));
        }
        if let Body::Agg { slots, .. } = &body {
            for (i, v) in slots.iter().enumerate() {
                self.add_ref_kind(*v, self.slot_kind(ty, i))?;
            }
        }
        Ok(Ok(self.place(body)))
    }

    /// [`Heap::build_dyn`] of the dynamic array of bytes of type `ty` from pieces of `n` (1 to
    /// 2^31 - 1) elements in all: a range of an array of bytes, or of an array of one-slot
    /// elements (an open array parameter given a static array of `Byte`), and elements, whose low
    /// 8 bits are taken (the compiler gave `Byte`s).
    fn build_bytes(
        &mut self,
        ty: u32,
        pieces: &[Piece],
        n: usize,
    ) -> Result<Result<StrRef, OutOfMemory>, BadRef> {
        let mut bytes: Vec<u8> = Vec::new();
        if try_reserve(&mut bytes, n).is_err() {
            return Ok(Err(OutOfMemory));
        }
        for p in pieces {
            match *p {
                Piece::Range(r, start, end) if end > start => match &self.block(r, "copy")?.body {
                    Some(Body::Bytes { bytes: b, .. }) => bytes.extend_from_slice(&b[start..end]),
                    Some(Body::Agg { ty: rty, slots }) if self.width_of(*rty) == 1 => {
                        bytes.extend(slots[start..end].iter().map(|v| *v as u8));
                    }
                    _ => {
                        return Err(BadRef {
                            message: format!("heap block {r} is not an array of bytes"),
                        });
                    }
                },
                Piece::Range(..) => {}
                Piece::Elem(v) => bytes.push(v as u8),
            }
        }
        let body = Body::Bytes { ty, bytes };
        if !self.fits(body.size()) {
            return Ok(Err(OutOfMemory));
        }
        Ok(Ok(self.place(body)))
    }

    /// The string a reference points to, for writing in place, when the reference is its only one
    /// (`None` for 0 and for a shared string; `StrSetIndex`, `docs/spec/bytecode.md` 2.4).
    pub fn unique_str_mut(&mut self, r: StrRef) -> Result<Option<&mut PStr>, BadRef> {
        if r == 0 {
            return Ok(None);
        }
        let block = self.block_mut(r, "write")?;
        if block.rc != 1 || !matches!(block.body, Some(Body::Str(_))) {
            return Ok(None);
        }
        match block.body_to_rewrite() {
            Some(Body::Str(s)) => Ok(Some(s)),
            _ => unreachable!("checked to be a string above"),
        }
    }

    /// Appends the string `x` to the string block `r` in place (`AppendStr`; the caller has checked
    /// that the block may be written: `docs/spec/bytecode.md` 2.4). The buffer grows with capacity
    /// to spare within the usage limit; on `OutOfMemory` the string is unchanged. `x` may be `r`.
    pub fn append_str(&mut self, r: StrRef, x: StrRef) -> Result<Result<(), OutOfMemory>, BadRef> {
        // The text of `x` is read while the block of `r` is out of the slab; a string appended to
        // itself is copied within its own buffer
        let room = self.room();
        let mut body = self.take_body(r, "append")?;
        let Body::Str(s) = &mut body else {
            self.put_body(r, body);
            return Err(Self::bad("append", r));
        };
        let before = s.heap_size();
        let done = if x == r {
            Ok(s.append_self(room))
        } else {
            self.get(x).map(|other| s.append(other, room))
        };
        let after = s.heap_size();
        self.put_body(r, body);
        self.change_used(before, after);
        done
    }

    /// Appends the string `x` to the string block `r` in place with no capacity to spare, as
    /// `ConcatStr` makes a new value (`docs/spec/types.md` 9.1): the caller has checked that `r`
    /// is its only reference and is not `x`. On `OutOfMemory` the string is unchanged.
    pub fn concat_in_place(
        &mut self,
        r: StrRef,
        x: StrRef,
    ) -> Result<Result<(), OutOfMemory>, BadRef> {
        let mut body = self.take_body(r, "append")?;
        let Body::Str(s) = &mut body else {
            self.put_body(r, body);
            return Err(Self::bad("append", r));
        };
        let before = s.heap_size();
        // The limit is checked as for a new value (`alloc_str`) with the old one still counted,
        // so the result near the limit is the same as making a new string and letting go of the
        // old one (`docs/spec/bytecode.md` 2.4)
        let done = self.get(x).map(|other| {
            if self.fits(s.joined_size(other) + std::mem::size_of::<PStr>()) {
                s.append_exact(other)
            } else {
                Err(OutOfMemory)
            }
        });
        let after = s.heap_size();
        self.put_body(r, body);
        self.change_used(before, after);
        done
    }

    /// Appends the `UTF8String` `x` to the `UTF8String` block `r` in place (`AppendU8`, like
    /// [`Heap::append_str`]).
    pub fn append_u8(&mut self, r: StrRef, x: StrRef) -> Result<Result<(), OutOfMemory>, BadRef> {
        let room = self.room();
        let mut body = self.take_body(r, "append")?;
        let Body::U8(b) = &mut body else {
            self.put_body(r, body);
            return Err(Self::bad("append", r));
        };
        let before = b.capacity();
        let done = if x == r {
            Ok(append_within(b, room))
        } else {
            self.get_u8(x)
                .map(|other| grow_within(b, other.len(), room).map(|()| b.extend_from_slice(other)))
        };
        let after = b.capacity();
        self.put_body(r, body);
        self.change_used(before, after);
        done
    }

    /// The strings `refs` joined (`ConcatStr`, `ConcatStrN`; `docs/spec/bytecode.md` 2.4): with
    /// fewer than two non-empty strings, the one there is (0 for none) to be shared; otherwise a
    /// new string ([`PStr::join`]). `OutOfMemory` when the total is over the length limit or the
    /// system refuses.
    pub fn join_strs(&self, refs: &[StrRef]) -> Result<Result<Joined, OutOfMemory>, BadRef> {
        let (mut non_empty, mut only) = (0usize, 0);
        for &r in refs {
            if !self.get(r)?.is_empty() {
                non_empty += 1;
                only = r;
            }
        }
        if non_empty < 2 {
            return Ok(Ok(Joined::Shared(only)));
        }
        let parts = refs
            .iter()
            .map(|&r| self.get(r).expect("each reference was read above"));
        Ok(PStr::join(parts).map(Joined::New))
    }

    /// Takes the body out of a live block, to change it while reading other blocks (forgetting
    /// the hash it remembers); it must be put back with [`Heap::put_body`] before anything else
    /// looks at the block.
    fn take_body(&mut self, r: StrRef, what: &str) -> Result<Body, BadRef> {
        let block = self.block_mut(r, what)?;
        Ok(block
            .body_to_rewrite()
            .take()
            .expect("a live block has a body"))
    }

    fn put_body(&mut self, r: StrRef, body: Body) {
        let block = &mut self.blocks[(r - 1) as usize];
        block.body = Some(body);
    }

    // ---- Lending (`docs/design/lifetime.md` 3.7) ----

    /// Whether any location is lent: every check below is skipped while none is.
    pub fn lending(&self) -> bool {
        !self.loans.is_empty()
    }

    /// The lending table, oldest first.
    pub fn loans(&self) -> &[Loan] {
        &self.loans
    }

    /// Marks the loan at position `j` of the table as taken out into a block (`PlaceBox`) in
    /// the register `value` (absolute position).
    pub fn set_loan_boxed(&mut self, j: usize, value: usize) {
        self.loans[j].boxed = true;
        self.loans[j].value = value;
    }

    /// The containers list of the lending table.
    pub fn loan_containers(&self) -> &[LoanContainer] {
        &self.loan_containers
    }

    /// Adds a container walked by the loan being made (before [`Heap::push_loan`]).
    pub fn push_loan_container(&mut self, c: LoanContainer) -> Result<(), OutOfMemory> {
        try_push(&mut self.loan_containers, c)
    }

    /// Drops the containers pushed for a loan that failed.
    pub fn truncate_loan_containers(&mut self, n: usize) {
        self.loan_containers.truncate(n);
    }

    /// Adds a loan whose containers were pushed since `loan.containers` and marks the blocks
    /// that hold the lent location: its own container, and the records and static arrays
    /// holding that one, outward up to a dynamic array (`docs/design/lifetime.md` 3.7).
    pub fn push_loan(&mut self, loan: Loan) -> Result<(), OutOfMemory> {
        try_push(&mut self.loans, loan)?;
        self.mark_containers(loan.containers);
        Ok(())
    }

    /// Marks the blocks holding the location lent by the newest loan, whose containers start at
    /// position `first` of the containers list ([`Heap::push_loan`]).
    #[inline]
    fn mark_containers(&mut self, first: usize) {
        // The lent location's own container, then the records and static arrays holding it. A
        // dynamic array is shared: the blocks holding it do not contain the lent location
        let last = self.loan_containers.len() - 1;
        self.mark_container(last);
        if first == last || self.is_dyn_type(self.loan_containers[last].ty) {
            return;
        }
        for k in (first..last).rev() {
            if self.is_dyn_type(self.loan_containers[k].ty) {
                break;
            }
            self.mark_container(k);
        }
    }

    /// Marks entry `k` of the containers list and its block.
    #[inline]
    fn mark_container(&mut self, k: usize) {
        let c = &mut self.loan_containers[k];
        c.marked = true;
        if let Some(block) = c.block.and_then(|b| self.live_block_mut(b)) {
            block.lent += 1;
        }
    }

    /// Unmarks the blocks the containers from position `first` of the containers list on had
    /// marked, and drops those containers (the loans using them are being ended).
    #[inline]
    fn unmark_containers(&mut self, first: usize) {
        for k in first..self.loan_containers.len() {
            let c = self.loan_containers[k];
            if c.marked
                && let Some(block) = c.block.and_then(|b| self.live_block_mut(b))
            {
                block.lent -= 1;
            }
        }
        self.loan_containers.truncate(first);
    }

    /// `Lend` of slot `i` of the record or array `r` that a reference slot points to (a path of
    /// one step; the fast path): takes the value out and records `loan` as [`Heap::push_loan`]
    /// does, with its container and lent slot filled in here. `None`, with nothing changed, when
    /// `r` is not a live record or array block, `i` is out of its slots, the slot is lent, a
    /// record or static array is shared, or a table would have to grow (the body reports the
    /// errors and grows the tables).
    #[inline]
    pub fn lend_fast(&mut self, r: StrRef, i: i64, mut loan: Loan) -> Option<i64> {
        if self.loans.len() == self.loans.capacity()
            || self.loan_containers.len() == self.loan_containers.capacity()
        {
            return None;
        }
        let index = self.plain_index(r)?;
        let i = usize::try_from(i).ok()?;
        let block = &self.blocks[index];
        let Some(Body::Agg { ty, slots }) = &block.body else {
            return None;
        };
        let ty = *ty;
        // An element of a flat record takes more than one slot: the path reads differently
        if i >= slots.len() || (block.rc != 1 && !self.is_dyn_type(ty)) || self.width_of(ty) != 1 {
            return None;
        }
        if block.lent != 0 && self.loans.iter().any(|l| l.covers(r, i)) {
            return None;
        }
        let Some(Body::Agg { slots, .. }) = &mut self.blocks[index].body else {
            unreachable!("the block was checked above");
        };
        let value = std::mem::take(&mut slots[i]);
        loan.containers = self.loan_containers.len();
        loan.ncontainers = 1;
        loan.loc = Some((r, i));
        loan.kind = self.slot_kind(ty, i);
        self.loan_containers.push(LoanContainer {
            block: Some(r),
            ty,
            marked: false,
        });
        self.loans.push(loan);
        self.mark_containers(loan.containers);
        Some(value)
    }

    /// `LendPlace` on the fast path: lends the flat place of `width` slots (of type `place`)
    /// that the step `step` (an element index), followed by `inner` (the slot number in the
    /// element) when given, selects in the dynamic array `r`, when the element is in range, no
    /// loan holds a slot of it and the tables have room. Returns the loan's position in the table,
    /// or `None` with nothing changed (the general path then raises what it must).
    pub fn lend_place_fast(
        &mut self,
        r: StrRef,
        step: i64,
        inner: Option<i64>,
        width: usize,
        place: u32,
        mut loan: Loan,
    ) -> Option<usize> {
        if self.loans.len() == self.loans.capacity()
            || self.loan_containers.len() == self.loan_containers.capacity()
        {
            return None;
        }
        let index = self.plain_index(r)?;
        let block = &self.blocks[index];
        let Some(Body::Agg { ty, slots }) = &block.body else {
            return None;
        };
        let ty = *ty;
        if !self.is_dyn_type(ty) {
            return None;
        }
        let w = self.width_of(ty);
        let step = usize::try_from(step)
            .ok()
            .filter(|s| *s < slots.len() / w)?;
        let inner = match inner {
            Some(k) => usize::try_from(k).ok().filter(|k| *k < w && w > 1)?,
            None => 0,
        };
        let start = step * w + inner;
        if start + width > slots.len() {
            return None;
        }
        if block.lent != 0
            && self
                .loans
                .iter()
                .any(|l| l.overlaps(r, start, start + width))
        {
            return None;
        }
        loan.containers = self.loan_containers.len();
        loan.ncontainers = 1;
        loan.loc = Some((r, start));
        loan.width = width;
        loan.place = Some(place);
        loan.kind = SLOT_PLAIN;
        self.loan_containers.push(LoanContainer {
            block: Some(r),
            ty,
            marked: false,
        });
        self.loans.push(loan);
        self.mark_containers(loan.containers);
        Some(self.loans.len() - 1)
    }

    /// Whether the flat place a loan lends from slot `start` of the block `r` is where its path
    /// (the element `step`, then `inner` in it when given) leads in the dynamic array `r` again, so
    /// that `Unlend` has nothing to move (the fast path).
    pub fn place_back_ready(&self, r: StrRef, start: usize, step: i64, inner: Option<i64>) -> bool {
        let Some(index) = self.plain_index(r) else {
            return false;
        };
        let Some(Body::Agg { ty, .. }) = &self.blocks[index].body else {
            return false;
        };
        if !self.is_dyn_type(*ty) {
            return false;
        }
        let w = self.width_of(*ty) as i64;
        step * w + inner.unwrap_or(0) == start as i64
    }

    /// Slot `i` of the flat place loan `j` lends (`RefFieldGet` / `RefFieldSet` through a reference
    /// slot of kind 4 on the fast path), when `j` is the newest loan (no later one can hold a slot
    /// of the place) and the place's block is a plain record or array block.
    #[inline(always)]
    pub fn place_slot_fast(&mut self, j: usize, i: i64) -> Option<&mut i64> {
        if j + 1 != self.loans.len() {
            return None;
        }
        let loan = &self.loans[j];
        if loan.place.is_none() || loan.boxed {
            return None;
        }
        let (r, start) = loan.loc?;
        let i = usize::try_from(i).ok().filter(|i| *i < loan.width)?;
        let index = self.plain_index(r)?;
        match self.blocks[index].body.as_mut()? {
            Body::Agg { slots, .. } => slots.get_mut(start + i),
            _ => None,
        }
    }

    /// Whether slot `i` of the block `r` takes back a lent value as `Unlend` would without
    /// following the path again (the fast path): `r` is a live record or array block, unshared
    /// unless it is a dynamic array (so `Unlend` would not copy it), and the slot is still empty.
    pub fn give_back_ready(&self, r: StrRef, i: usize) -> bool {
        let Some(index) = self.plain_index(r) else {
            return false;
        };
        let block = &self.blocks[index];
        match &block.body {
            Some(Body::Agg { ty, slots }) => {
                slots.get(i) == Some(&0) && (block.rc == 1 || self.is_dyn_type(*ty))
            }
            _ => false,
        }
    }

    /// Ends the loans from position `from` of the table on, oldest first, each of one slot whose
    /// block [`Heap::give_back_ready`] accepted: unmarks their containers and writes back the
    /// value `take` gives for each (what [`Heap::take_loans`] and `Unlend` do for them).
    pub fn give_back_fast(&mut self, from: usize, mut take: impl FnMut(&Loan) -> i64) {
        self.unmark_containers(self.loans[from].containers);
        for k in from..self.loans.len() {
            let loan = self.loans[k];
            let (r, i) = loan.loc.expect("give_back_ready accepted the lent slot");
            let value = take(&loan);
            // A flat place was written where it is
            if loan.place.is_some() {
                continue;
            }
            let index = self
                .plain_index(r)
                .expect("give_back_ready accepted the block");
            let Some(Body::Agg { slots, .. }) = &mut self.blocks[index].body else {
                unreachable!("give_back_ready accepted the block");
            };
            slots[i] = value;
        }
        self.loans.truncate(from);
    }

    /// The position in the table of the first loan made at frame depth `depth` or deeper (the
    /// table's length when there is none).
    pub fn loans_from(&self, depth: usize) -> usize {
        self.loans
            .iter()
            .rposition(|l| l.depth < depth)
            .map_or(0, |i| i + 1)
    }

    /// Removes the loans made at frame depth `depth` or deeper (the newest ones), unmarking their
    /// containers, and returns them oldest first in `out` (cleared first) with their containers
    /// in `containers`.
    pub fn take_loans(
        &mut self,
        depth: usize,
        out: &mut Vec<Loan>,
        containers: &mut Vec<LoanContainer>,
    ) {
        out.clear();
        containers.clear();
        let from = self.loans_from(depth);
        if from == self.loans.len() {
            return;
        }
        let first = self.loans[from].containers;
        containers.extend_from_slice(&self.loan_containers[first..]);
        self.unmark_containers(first);
        out.extend(self.loans.drain(from..).map(|mut l| {
            l.containers -= first;
            l
        }));
    }

    /// Forgets a freed block in the lending table, so that its index can be reused. Its slots
    /// are `body`'s: the slots of a flat place lent from it move to a block of their own
    /// ([`Heap::forget_lent_slots`]).
    fn forget_loaned_block(&mut self, r: StrRef, body: &mut Body) {
        for c in &mut self.loan_containers {
            if c.block == Some(r) {
                c.block = None;
            }
        }
        let slots: &mut [i64] = match body {
            Body::Agg { slots, .. } => slots,
            Body::Obj { slots, .. } => slots,
            _ => &mut [],
        };
        let mut moved = std::mem::take(&mut self.evacuating);
        // Newest first: a place lent through another one takes its values out before the outer
        // place moves out (leaving 0 where the inner one was)
        for (k, loan) in self.loans.iter().enumerate().rev() {
            if let Some((b, start)) = loan.loc
                && b == r
            {
                moved.push((k, start));
            }
        }
        for &(k, start) in &moved {
            self.evacuate(k, slots, start);
        }
        moved.clear();
        self.evacuating = moved;
    }

    /// Forgets the slots from `from` on of the block `r`, which `dropped` holds (the slots
    /// `SetLength` dropped from a dynamic array resized in place): a value lent from one is lent
    /// no more, and a flat place lent from them moves to a block of its own.
    fn forget_lent_slots(&mut self, r: StrRef, from: usize, dropped: &mut [i64]) {
        // Newest first, as in `forget_loaned_block`
        for k in (0..self.loans.len()).rev() {
            let loan = self.loans[k];
            match loan.loc {
                Some((b, start)) if b == r && start >= from => {
                    if loan.place.is_some() && !loan.boxed {
                        self.evacuate(k, dropped, start - from);
                    } else {
                        self.loans[k].loc = None;
                    }
                }
                _ => {}
            }
        }
    }

    /// Moves the slots of the flat place lent by loan `k` from `slots` (where they start at
    /// `start`) to a new block of the place's type, which the loan holds from then on: its
    /// container goes away, and `Unlend` gives the values back from there
    /// (`docs/design/records.md` 3.7 B). A value loan is only forgotten. The block is made beyond
    /// the usage limit, as moving out cannot fail (it still counts toward the usage).
    fn evacuate(&mut self, k: usize, slots: &mut [i64], start: usize) {
        let loan = self.loans[k];
        let Some(ty) = loan.place.filter(|_| !loan.boxed) else {
            self.loans[k].loc = None;
            return;
        };
        let mut own = vec![0; loan.width];
        if let Some(range) = slots.get_mut(start..start + loan.width) {
            own.copy_from_slice(range);
            range.fill(0);
        }
        let b = self.place(Body::Agg { ty, slots: own });
        let loan = &mut self.loans[k];
        loan.loc = Some((b, 0));
        loan.evacuated = true;
    }

    /// The loan of slot `i` of the block `r`, if it is lent.
    pub fn loan_of(&self, r: StrRef, i: usize) -> Option<&Loan> {
        self.loan_after(r, i, None).map(|(_, l)| l)
    }

    /// The newest loan of slot `i` of the block `r` (with its position in the table) among those
    /// after position `after` (all with `None`). Reached through the flat place of loan `j`,
    /// the loans up to `j` holding the slot are `j` and the places it was lent from, so only
    /// the loans after `j` refuse it.
    pub fn loan_after(&self, r: StrRef, i: usize, after: Option<usize>) -> Option<(usize, &Loan)> {
        if self.loans.is_empty() || r == 0 {
            return None;
        }
        if self.block(r, "read").ok()?.lent == 0 && after.is_none() {
            return None;
        }
        let first = after.map_or(0, |j| j + 1);
        self.loans
            .iter()
            .enumerate()
            .skip(first)
            .rev()
            .find(|(_, l)| l.covers(r, i))
    }

    /// Refuses reading or writing slot `i` of the container `r` while it is lent.
    pub fn check_slot(&self, r: StrRef, i: i64) -> Result<(), Aliased> {
        if self.loans.is_empty() {
            return Ok(());
        }
        match usize::try_from(i).ok().and_then(|i| self.loan_of(r, i)) {
            Some(_) => Err(self.aliased_slot(r, i, false)),
            None => Ok(()),
        }
    }

    /// [`Heap::check_slot`] for each slot `lo..hi` of the block `r` (nothing to look at when no
    /// element is lent).
    pub fn check_slots(&self, r: StrRef, lo: i64, hi: i64) -> Result<(), Aliased> {
        if self.loans.is_empty() {
            return Ok(());
        }
        (lo..hi).try_for_each(|i| self.check_slot(r, i))
    }

    /// The refusal for slot `i` of the block `r` (an element of a dynamic or static array, or a
    /// field).
    pub fn aliased_slot(&self, r: StrRef, i: i64, twice: bool) -> Aliased {
        if let Ok(Block {
            body: Some(Body::Obj { .. }),
            ..
        }) = self.block(r, "read")
        {
            return Aliased::ObjField { twice };
        }
        let entry = self.agg_type(r).ok().and_then(|ty| self.type_entry(ty));
        match entry.map(|t| t.kind) {
            // The element holding the slot (an element of a flat record takes several)
            Some(TYPE_DYNARRAY) => Aliased::Elem {
                index: Some(i / entry.map_or(1, |t| t.elem_width() as i64)),
                twice,
            },
            Some(TYPE_ARRAY) => Aliased::Elem { index: None, twice },
            _ => Aliased::Field { twice },
        }
    }

    /// Refuses counting one more reference of a record or static array that contains a lent
    /// location (a copy would not see the value given back; `AssignRef` and the like). A dynamic
    /// array is shared, so counting it is not copying.
    pub fn check_share(&self, r: StrRef) -> Result<(), Aliased> {
        if self.loans.is_empty() || !is_block_ref(r) {
            return Ok(());
        }
        match self.block(r, "add_ref") {
            Ok(Block {
                lent: 1..,
                body: Some(Body::Agg { ty, .. }),
                ..
            }) if !self.is_dyn_type(*ty) => Err(Aliased::Copy),
            _ => Ok(()),
        }
    }

    /// Refuses copying or moving the elements `start..end` of the array `r` when one of them is
    /// lent or contains a lent location (`Copy`, `+`, `Insert`, `SetLength` of a shared array).
    pub fn check_copy(&self, r: StrRef, start: usize, end: usize) -> Result<(), Aliased> {
        if self.loans.is_empty() || r == 0 || start >= end {
            return Ok(());
        }
        let Ok(Block {
            body: Some(Body::Agg { ty, .. } | Body::Bytes { ty, .. }),
            ..
        }) = self.block(r, "copy")
        else {
            return Ok(());
        };
        let w = self.width_of(*ty);
        self.check_copy_slots(r, start * w, end * w)
    }

    /// Refuses copying the slots `start..end` of the record or array block `r` when one of them
    /// is lent or holds a value containing a lent location (`AggLoad`, `check_copy`).
    pub fn check_copy_slots(&self, r: StrRef, start: usize, end: usize) -> Result<(), Aliased> {
        if self.loans.is_empty() || r == 0 || start >= end {
            return Ok(());
        }
        let Ok(block) = self.block(r, "copy") else {
            return Ok(());
        };
        if block.lent != 0 && self.loans.iter().any(|l| l.overlaps(r, start, end)) {
            return Err(Aliased::Copy);
        }
        let ty = match &block.body {
            Some(Body::Agg { ty, .. } | Body::Obj { ty, .. }) => *ty,
            _ => return Ok(()),
        };
        let slots = match &block.body {
            Some(Body::Agg { slots, .. }) => &slots[..],
            Some(Body::Obj { slots, .. }) => &slots[..],
            _ => return Ok(()),
        };
        for (i, v) in slots
            .iter()
            .enumerate()
            .take(end.min(slots.len()))
            .skip(start)
        {
            if self.slot_kind(ty, i) == SLOT_REF {
                self.check_share(*v)?;
            }
        }
        Ok(())
    }

    /// Refuses copying the whole block `r` (`Unique` of a shared block) while it contains a lent
    /// location.
    pub fn check_copy_block(&self, r: StrRef) -> Result<(), Aliased> {
        if self.loans.is_empty() || r == 0 {
            return Ok(());
        }
        let ty = self.agg_type(r).unwrap_or(0);
        if self.is_dyn_type(ty) {
            let n = self.len(r).unwrap_or(0);
            self.check_copy(r, 0, n)
        } else {
            self.check_share(r)
        }
    }

    /// The reference count of a block (0 for the default value).
    pub fn ref_count(&self, r: StrRef) -> Result<u32, BadRef> {
        if r == 0 {
            return Ok(0);
        }
        Ok(self.block(r, "read")?.rc)
    }

    /// Adds one reference (0 is the default value and holds nothing). A handle of an instance of a
    /// reference counted class counts like a heap reference (its lower 32 bits name the block);
    /// one of a tombstone counts the tombstone.
    pub fn add_ref(&mut self, r: StrRef) -> Result<(), BadRef> {
        if r == 0 {
            return Ok(());
        }
        let Some(block) = slab_index(r).and_then(|i| self.blocks.get_mut(i)) else {
            return Err(BadRef {
                message: format!("add_ref of an invalid heap reference {r}"),
            });
        };
        if block.body.is_none() && block.tag & TAG_KIND != TAG_TOMB {
            return Err(BadRef {
                message: format!("add_ref of a freed heap block {r}"),
            });
        }
        block.rc += 1;
        Ok(())
    }

    /// Whether `h` is a handle of an instance of a reference counted class (live or a tombstone)
    /// that its generation still matches: the values `AddRefObj` / `ReleaseObj` count
    /// (`docs/spec/objects.md` 4.2). A handle of an instance managed by hand, or of one already
    /// freed, is not counted.
    fn counted_by_contents(&self, h: i64) -> bool {
        self.counted_index(h).is_some()
    }

    /// The slab index of `h` when it is counted by its contents (see
    /// [`Heap::counted_by_contents`]).
    #[inline(always)]
    fn counted_index(&self, h: i64) -> Option<usize> {
        let index = slab_index(h)?;
        let block = self.blocks.get(index)?;
        let kind = block.tag & TAG_KIND;
        (block.tag >> 8 == ((h as u64) >> 32) as u32
            && (kind == TAG_TOMB || (kind == TAG_OBJ_COUNTED && block.body.is_some())))
        .then_some(index)
    }

    /// `AssignObj`'s counting of the new value `new` and the old value `old` when it frees
    /// nothing and no job is open (the fast path): `false`, with nothing changed, when letting go
    /// of `old` may free it (its count is below 2), so the body does the whole work.
    #[inline(always)]
    pub fn assign_obj_fast(&mut self, new: i64, old: i64) -> bool {
        if self.open {
            return false;
        }
        if new == old {
            // Counted once and let go once: no change
            return true;
        }
        let old_index = self.counted_index(old);
        if let Some(i) = old_index
            && self.blocks[i].rc < 2
        {
            return false;
        }
        if let Some(i) = self.counted_index(new) {
            self.blocks[i].rc += 1;
        }
        if let Some(i) = old_index {
            self.blocks[i].rc -= 1;
        }
        true
    }

    /// `ReleaseObj` of `h` when it frees nothing and no job is open (the fast path): `false`, with
    /// nothing changed, otherwise.
    #[inline(always)]
    pub fn release_obj_fast(&mut self, h: i64) -> bool {
        if self.open {
            return false;
        }
        match self.counted_index(h) {
            None => true,
            Some(i) if self.blocks[i].rc >= 2 => {
                self.blocks[i].rc -= 1;
                true
            }
            Some(_) => false,
        }
    }

    /// [`Heap::release`] of `r` when it frees nothing and no job is open (the fast path): `false`,
    /// with nothing changed, otherwise.
    #[inline(always)]
    pub fn release_fast(&mut self, r: StrRef) -> bool {
        !self.open && (r == 0 || self.decrement_kept(r))
    }

    /// `AddRefObj` of `h` (the fast path; counting never fails).
    #[inline(always)]
    pub fn add_ref_obj_fast(&mut self, h: i64) {
        if let Some(i) = self.counted_index(h) {
            self.blocks[i].rc += 1;
        }
    }

    /// The slab index of `r` when it is a heap reference (no kind or generation in its upper bits)
    /// to a live block that is not an instance: a string, a record or an array, which the fast
    /// paths of the counting instructions count like `add_ref` / `release` do. Anything else
    /// (0, an instance's handle, a tombstone, a freed block) is `None`.
    #[inline(always)]
    fn plain_index(&self, r: i64) -> Option<usize> {
        if (r as u64) >> 32 != 0 {
            return None;
        }
        let index = usize::try_from(r - 1).ok()?;
        let block = self.blocks.get(index)?;
        (block.tag & TAG_KIND == TAG_PLAIN && block.body.is_some()).then_some(index)
    }

    /// The slab index of a value a fast path counts: `Some(None)` for 0 (nothing to count),
    /// `Some(Some(i))` for a plain block ([`Heap::plain_index`]) or a handle counted by its
    /// contents ([`Heap::counted_index`]; `add_ref` / `release` count it the same way), `None` for
    /// anything else.
    #[inline(always)]
    fn count_index(&self, r: i64) -> Option<Option<usize>> {
        if r == 0 {
            Some(None)
        } else {
            self.plain_index(r)
                .or_else(|| self.counted_index(r))
                .map(Some)
        }
    }

    /// The counting of `AssignRef` and the other instructions that put the managed value `new` in
    /// place of `old` (`add_ref` of `new`, then `release` of `old`), when that frees nothing,
    /// `new` holds no lent location (so `check_share` passes) and no job is open (the fast path;
    /// `docs/design/vm.md` 3.14 (a′)): `false`, with nothing changed, when either value is not
    /// one [`Heap::count_index`] takes or when letting go of `old` may free it (its count is below
    /// 2), so the body does the whole work.
    #[inline(always)]
    pub fn assign_ref_fast(&mut self, new: i64, old: i64) -> bool {
        if self.open {
            return false;
        }
        let Some(new_index) = self.count_index(new) else {
            return false;
        };
        if new_index.is_some_and(|i| self.blocks[i].lent != 0) {
            return false;
        }
        if new == old {
            // Counted once and let go once: no change
            return true;
        }
        let Some(old_index) = self.count_index(old) else {
            return false;
        };
        if let Some(i) = old_index {
            if self.blocks[i].rc < 2 {
                return false;
            }
            self.blocks[i].rc -= 1;
        }
        if let Some(i) = new_index {
            self.blocks[i].rc += 1;
        }
        true
    }

    /// `AddRefObj` (`docs/spec/objects.md` 14.3): counts `h` if it is an instance of a reference
    /// counted class.
    pub fn add_ref_obj(&mut self, h: i64) -> Result<(), BadRef> {
        if self.counted_by_contents(h) {
            self.add_ref(h)?;
        }
        Ok(())
    }

    /// `ReleaseObj` (`docs/spec/objects.md` 14.3): lets go of `h` if it is an instance of a
    /// reference counted class.
    pub fn release_obj(&mut self, h: i64) -> Result<(), BadRef> {
        if self.counted_by_contents(h) {
            self.release(h)?;
        }
        Ok(())
    }

    /// Drops one reference and frees the block when it was the last one (0 is the default value
    /// and holds nothing). The references held by a freed record or array are dropped through a
    /// work list, not by recursion, so deep nesting does not use up the Rust stack
    /// (`docs/design/lifetime.md` 3.2). An instance of a reference counted class that loses its
    /// last reference is not destroyed here: an open job takes it and the rest of the work (its
    /// destructor is Pascal code the VM runs; `docs/design/objects.md` 3.8), and while a job is
    /// open the releases that follow go behind it.
    pub fn release(&mut self, r: StrRef) -> Result<(), BadRef> {
        if r == 0 {
            return Ok(());
        }
        // Most releases only decrement a count of 2 or more, with one look at the block
        // (`docs/design/vm.md` 3.14 (a′))
        if !self.open && self.decrement_kept(r) {
            return Ok(());
        }
        if self.open {
            let job = self.jobs.last_mut().expect("an open job is the last");
            let before = job.work.capacity();
            job.work.push_front(Work::Ref(r));
            let after = job.work.capacity();
            self.count_capacity::<Work>(before, after);
            return Ok(());
        }
        // Most releases only decrement the count: no work list then
        // `decrement_kept` was tried above
        match self.decrement_rest(r)? {
            Dec::Kept => Ok(()),
            // A string, a callable value and a cell of a plain value hold no references: nothing
            // to cascade
            Dec::Freed(
                Body::Str(_)
                | Body::U8(_)
                | Body::Bytes { .. }
                | Body::Callable(_)
                | Body::Cell {
                    kind: SLOT_PLAIN, ..
                },
            ) => Ok(()),
            Dec::Freed(Body::Closure { caps, .. }) => match self.let_go_caps(&caps)? {
                None => Ok(()),
                Some((dec, from)) => self.cascade_listed(dec, |_, work| {
                    work.extend(caps[from..].iter().rev().map(|v| Work::Ref(*v)));
                }),
            },
            Dec::Freed(body @ Body::Obj { .. }) => {
                let result = match self.let_go_fields(&body)? {
                    None => Ok(()),
                    Some((dec, from)) => self.cascade_listed(dec, |heap, work| {
                        heap.push_children_from(&body, from, work);
                    }),
                };
                self.keep_slots(body);
                result
            }
            dec @ Dec::Freed(_) => self.cascade_listed(dec, |_, _| {}),
            Dec::Dying(h) => {
                self.open_job(VecDeque::from([Work::Destroy(h)]));
                Ok(())
            }
        }
    }

    /// The end of [`Heap::release`] that needs the work list: [`Heap::cascade`] from `first`,
    /// then the references `rest` pushes onto the work list (the captured cells or the fields
    /// [`Heap::let_go_caps`] / [`Heap::let_go_fields`] stopped at, pushed so that they pop in the
    /// order [`Heap::push_children`] gives).
    fn cascade_listed(
        &mut self,
        first: Dec,
        rest: impl FnOnce(&Self, &mut Vec<Work>),
    ) -> Result<(), BadRef> {
        // The cascade's work list is kept on the heap and reused, so that releasing does not
        // allocate per call
        let mut work = std::mem::take(&mut self.release_work);
        let kept = work.capacity();
        // `rest` pops after the references `first` pushes
        rest(self, &mut work);
        let result = self.cascade(first, &mut work);
        if result.is_ok() && !work.is_empty() {
            // Stopped at an instance: the rest goes into the job it opened, after its destruction
            let job = self.jobs.last_mut().expect("the cascade opened a job");
            let before = job.work.capacity();
            for w in work.drain(..).rev() {
                job.work.push_front(w);
            }
            let after = job.work.capacity();
            self.count_capacity::<Work>(before, after);
        }
        work.clear();
        // Releasing a large array grows the list to its element count: do not keep that alive
        work.shrink_to(RELEASE_WORK_KEEP);
        self.count_capacity::<Work>(kept, work.capacity());
        self.release_work = work;
        result
    }

    /// Counts a change in the capacity of a release work list or of the job stack in `used`, in
    /// elements of `T` (`docs/spec/objects.md` 14.8). The limit is not checked: letting go always
    /// runs to the end (stopping would leak the rest), so `used` may be over the limit until the
    /// work is done, and the allocations of Pascal code meanwhile raise `EOutOfMemory`. Each place
    /// that changes a capacity counts its own change, so the total is never recomputed.
    fn count_capacity<T>(&mut self, before: usize, after: usize) {
        let size = std::mem::size_of::<T>();
        self.change_used(before * size, after * size);
    }

    /// Opens a job with the given work (`docs/design/objects.md` 3.8).
    fn open_job(&mut self, work: VecDeque<Work>) {
        self.add_job(Job {
            work,
            exc: 0,
            origin: JobOrigin::Open,
            depth: 0,
        });
        self.open = true;
    }

    /// Puts a job on the stack, counting its work list and the growth of the stack.
    fn add_job(&mut self, job: Job) {
        let before = self.jobs.capacity();
        self.count_capacity::<Work>(0, job.work.capacity());
        self.jobs.push(job);
        let after = self.jobs.capacity();
        self.count_capacity::<Job>(before, after);
    }

    /// Release the references held by a freed block (`first`, the result of letting go of a
    /// value) and, in turn, by the blocks they free, then the rest of `work`. Stops at an instance
    /// of a reference counted class that loses its last reference: it opens a job for its
    /// destruction, and what is left in `work` goes after it.
    fn cascade(&mut self, first: Dec, work: &mut Vec<Work>) -> Result<(), BadRef> {
        match first {
            Dec::Kept => {}
            Dec::Freed(body) => self.push_children(&body, work),
            Dec::Dying(h) => {
                self.open_job(VecDeque::from([Work::Destroy(h)]));
                return Ok(());
            }
        }
        while let Some(w) = work.pop() {
            match self.let_go(w)? {
                Dec::Kept => {}
                Dec::Freed(body) => self.push_children(&body, work),
                Dec::Dying(h) => {
                    self.open_job(VecDeque::from([Work::Destroy(h)]));
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Lets go of the captured cells of a freed closure in order, without the work list, while
    /// each only drops a count or frees a cell of a plain value (the common case of a closure
    /// replaced in a loop): `None` once all are let go, or the result for the cell that needs
    /// more (a freed cell holding a reference, or an instance losing its last reference) and the
    /// position of the next cell, for the work list to go on from.
    #[inline]
    fn let_go_caps(&mut self, caps: &[i64]) -> Result<Option<(Dec, usize)>, BadRef> {
        for (i, &cell) in caps.iter().enumerate() {
            if self.decrement_kept(cell) {
                continue;
            }
            match self.decrement_rest(cell)? {
                Dec::Kept
                | Dec::Freed(Body::Cell {
                    kind: SLOT_PLAIN, ..
                }) => {}
                dec => return Ok(Some((dec, i + 1))),
            }
        }
        Ok(None)
    }

    /// Lets go of the fields of a freed instance (and an exception's `Suppressed`) in order,
    /// without the work list, while each only drops a count or frees a block that holds no
    /// references (the common case of an instance with string fields replaced in a loop): `None`
    /// once all are let go, or the result for the value that needs more (a freed block holding
    /// references, or an instance losing its last reference) and the position of the next value
    /// (as [`Heap::push_children_from`] counts them), for the work list to go on from.
    #[inline]
    fn let_go_fields(&mut self, body: &Body) -> Result<Option<(Dec, usize)>, BadRef> {
        let Body::Obj {
            ty, slots, extra, ..
        } = body
        else {
            return Ok(None);
        };
        let suppressed: &[i64] = extra.as_ref().map_or(&[], |x| &x.suppressed);
        let n = slots.len();
        for i in 0..n + suppressed.len() {
            let (v, kind) = match slots.get(i) {
                Some(v) => (*v, self.slot_kind(*ty, i)),
                None => (suppressed[i - n], SLOT_REF),
            };
            let dec = match kind {
                _ if v == 0 => continue,
                SLOT_REF => {
                    if self.decrement_kept(v) {
                        continue;
                    }
                    self.decrement_rest(v)?
                }
                SLOT_OBJ if self.counted_by_contents(v) => self.decrement(v)?,
                _ => continue,
            };
            match dec {
                Dec::Kept
                | Dec::Freed(
                    Body::Str(_)
                    | Body::U8(_)
                    | Body::Bytes { .. }
                    | Body::Callable(_)
                    | Body::Cell {
                        kind: SLOT_PLAIN, ..
                    },
                ) => {}
                dec => return Ok(Some((dec, i + 1))),
            }
        }
        Ok(None)
    }

    /// Keeps the slot box of a freed instance in [`Heap::slot_pool`] when there is room for it
    /// (the rest of the body is dropped).
    fn keep_slots(&mut self, body: Body) {
        if let Body::Obj { slots, .. } = body
            && let Some(pool) = self.slot_pool_of(slots.len())
            && pool.len() < SLOT_POOL_KEEP
        {
            pool.push(slots);
        }
    }

    /// The boxes of [`Heap::slot_pool`] of `n` slots (`None` for 0 or more than
    /// [`SLOT_POOL_LENS`]).
    fn slot_pool_of(&mut self, n: usize) -> Option<&mut Vec<Box<[i64]>>> {
        self.slot_pool.get_mut(n.checked_sub(1)?)
    }

    /// Lets go of one value of a work list (a [`Work::Destroy`] is taken as already let go).
    fn let_go(&mut self, w: Work) -> Result<Dec, BadRef> {
        match w {
            Work::Ref(v) => self.decrement(v),
            Work::Obj(v) if self.counted_by_contents(v) => self.decrement(v),
            Work::Obj(_) => Ok(Dec::Kept),
            Work::Destroy(h) => Ok(Dec::Dying(h)),
        }
    }

    /// Decrement the count of `r` (nothing for 0); the body of the block when that frees it, or
    /// the handle of an instance of a reference counted class that lost its last reference. A
    /// tombstone's index is reused once its last reference goes. An instance managed by hand is
    /// never counted: a release reaching it (only forged bytecode does that) stops there
    /// (`docs/design/objects.md` 3.2).
    fn decrement(&mut self, r: StrRef) -> Result<Dec, BadRef> {
        if self.decrement_kept(r) {
            return Ok(Dec::Kept);
        }
        self.decrement_rest(r)
    }

    /// Decrements the count of `r` when that keeps the block (its count is 2 or more and it is
    /// live or a tombstone; [`Heap::decrement`]'s common case); `false`, with nothing changed,
    /// otherwise.
    #[inline(always)]
    fn decrement_kept(&mut self, r: StrRef) -> bool {
        if let Some(block) = slab_index(r).and_then(|i| self.blocks.get_mut(i))
            && block.rc >= 2
            && (block.body.is_some() || block.tag & TAG_KIND == TAG_TOMB)
        {
            block.rc -= 1;
            return true;
        }
        false
    }

    /// [`Heap::decrement`] when [`Heap::decrement_kept`] did not apply.
    fn decrement_rest(&mut self, r: StrRef) -> Result<Dec, BadRef> {
        if r == 0 {
            return Ok(Dec::Kept);
        }
        let index = slab_index(r)
            .filter(|i| *i < self.blocks.len())
            .ok_or_else(|| BadRef {
                message: format!("release of an invalid heap reference {r}"),
            })?;
        let block = &mut self.blocks[index];
        let kind = block.tag & TAG_KIND;
        if block.body.is_none() {
            if kind != TAG_TOMB || block.rc == 0 {
                return Err(BadRef {
                    message: format!("release of a freed heap block {r}"),
                });
            }
            block.rc -= 1;
            if block.rc == 0 {
                self.retire(index);
            }
            return Ok(Dec::Kept);
        }
        if block.rc == 0 {
            // An instance being destroyed whose references were all let go again: its destruction
            // frees it
            return Ok(Dec::Kept);
        }
        block.rc -= 1;
        if block.rc > 0 {
            return Ok(Dec::Kept);
        }
        match kind {
            TAG_OBJ_MANUAL => {
                block.rc = 1;
                return Ok(Dec::Kept);
            }
            TAG_OBJ_COUNTED => {
                let ty = match block.body {
                    Some(Body::Obj { dying: true, .. }) => return Ok(Dec::Kept),
                    Some(Body::Obj { ty, .. }) => ty,
                    _ => unreachable!("a counted block is an instance"),
                };
                let generation = block.tag >> 8;
                if self.destruction_runs_code(ty) {
                    return Ok(Dec::Dying(make_handle(index, generation)));
                }
                // Nothing can observe the instance any more (no reference is left and its
                // destruction runs no code), so it is freed here like a record, and its fields
                // are let go next: the order is the one a job would give (`docs/spec/objects.md`
                // 14.8). Built-in exceptions go this way.
            }
            _ => {}
        }
        let block = &mut self.blocks[index];
        let lent = std::mem::take(&mut block.lent);
        let mut body = block.body.take().expect("a live block");
        self.used -= body.size();
        if lent != 0 {
            self.forget_loaned_block(index as StrRef + 1, &mut body);
        }
        self.retire(index);
        Ok(Dec::Freed(body))
    }

    /// Whether the destruction of an instance of class `ty` runs Pascal code (a destructor or a
    /// `BeforeDestruction` of its own; `docs/spec/objects.md` 14.8), so it needs a frame.
    #[inline]
    pub fn destruction_runs_code(&self, ty: u32) -> bool {
        self.runs_code.get(ty as usize).is_none_or(|r| *r)
    }

    /// Push the references a freed block holds onto the work list, so that they pop in slot
    /// order (`docs/design/lifetime.md` 3.2).
    fn push_children(&self, body: &Body, work: &mut Vec<Work>) {
        self.push_children_from(body, 0, work);
    }

    /// [`Heap::push_children`] of the references from position `from` on: of a closure's
    /// captured cells, or of an instance's fields followed by an exception's `Suppressed` (the
    /// positions [`Heap::let_go_caps`] and [`Heap::let_go_fields`] give). `from` is 0 for any
    /// other block.
    fn push_children_from(&self, body: &Body, from: usize, work: &mut Vec<Work>) {
        let (ty, slots): (u32, &[i64]) = match body {
            Body::Agg { ty, slots } => (*ty, slots),
            Body::Obj {
                ty, slots, extra, ..
            } => {
                // `Suppressed` goes after the fields
                if let Some(x) = extra {
                    let skip = from.saturating_sub(slots.len());
                    for v in x.suppressed.iter().skip(skip).rev() {
                        work.push(Work::Ref(*v));
                    }
                }
                (*ty, slots)
            }
            Body::Closure { caps, .. } => {
                for v in caps.iter().skip(from).rev() {
                    work.push(Work::Ref(*v));
                }
                return;
            }
            Body::Cell { kind, value, .. } => {
                match *kind {
                    SLOT_REF => work.push(Work::Ref(*value)),
                    SLOT_OBJ => work.push(Work::Obj(*value)),
                    _ => {}
                }
                return;
            }
            _ => return,
        };
        for (i, v) in slots.iter().enumerate().skip(from).rev() {
            if *v == 0 {
                continue;
            }
            match self.slot_kind(ty, i) {
                SLOT_REF => work.push(Work::Ref(*v)),
                SLOT_OBJ => work.push(Work::Obj(*v)),
                _ => {}
            }
        }
    }

    // ---- Release jobs (`docs/design/objects.md` 3.8) ----

    /// Whether there is a job (stamped or open).
    pub fn has_jobs(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// Whether the last job is open (made by the instruction that just ran).
    pub fn job_open(&self) -> bool {
        self.open
    }

    /// The origin and the depth of the last job.
    pub fn top_job(&self) -> Option<(JobOrigin, u32)> {
        self.jobs.last().map(|j| (j.origin, j.depth))
    }

    /// Stamps the open job with its origin and the number of frames it runs at.
    pub fn stamp_job(&mut self, origin: JobOrigin, depth: u32) {
        if let Some(job) = self.jobs.last_mut().filter(|_| self.open) {
            job.origin = origin;
            job.depth = depth;
        }
        self.open = false;
    }

    /// Gives the last job (stamped) another origin and depth.
    pub fn restamp_job(&mut self, origin: JobOrigin, depth: u32) {
        if let Some(job) = self.jobs.last_mut() {
            job.origin = origin;
            job.depth = depth;
        }
    }

    /// Pushes a stamped job (the fields of an instance destroyed by an instruction, the global
    /// variables of a unit, …) and returns whether it has anything to do.
    pub fn push_job(&mut self, work: VecDeque<Work>, exc: i64, origin: JobOrigin, depth: u32) {
        debug_assert!(!self.open, "a job is pushed between instructions");
        self.add_job(Job {
            work,
            exc,
            origin,
            depth,
        });
    }

    /// Records an exception in the last job (holding its reference): the one recorded before goes
    /// to its `Suppressed` (`docs/spec/objects.md` 8.5). Returns the previous exception, which
    /// the VM attaches.
    pub fn job_record(&mut self, x: i64) -> i64 {
        let job = self.jobs.last_mut().expect("a job records the exception");
        std::mem::replace(&mut job.exc, x)
    }

    /// Puts the values of a destroyed instance's slots (and an exception's `Suppressed`) at the
    /// back of the last job (they are let go next, in slot order; `docs/spec/objects.md` 3.3,
    /// step 4).
    pub fn job_push_fields(&mut self, remains: &Remains) {
        self.fields_into_job(remains, None);
    }

    /// A new job (stamped with `origin` and `depth`) to let go of the slots of a destroyed
    /// instance, as [`Heap::push_job`] and [`Heap::job_push_fields`] make it; none when no slot
    /// holds a reference (the job would end at once with nothing to do).
    pub fn push_fields_job(&mut self, remains: &Remains, origin: JobOrigin, depth: u32) {
        self.fields_into_job(remains, Some((origin, depth)));
    }

    /// Puts the references the slots of a destroyed instance hold at the back of the last job,
    /// or of a new job stamped with `new_job` (made only when there is a reference).
    fn fields_into_job(&mut self, remains: &Remains, new_job: Option<(JobOrigin, u32)>) {
        let mut work = std::mem::take(&mut self.release_work);
        let kept = work.capacity();
        self.push_children(&remains.0, &mut work);
        self.count_capacity::<Work>(kept, work.capacity());
        if !work.is_empty() {
            if let Some((origin, depth)) = new_job {
                self.push_job(VecDeque::new(), 0, origin, depth);
            }
            self.job_extend(&mut work);
        }
        self.release_work = work;
    }

    /// Moves the values of `work` to the back of the last job's work, counting its growth.
    fn job_extend(&mut self, work: &mut Vec<Work>) {
        let job = self.jobs.last_mut().expect("a job takes the work");
        let before = job.work.capacity();
        job.work.extend(work.drain(..));
        let after = job.work.capacity();
        self.count_capacity::<Work>(before, after);
    }

    /// Takes the last job off (its work done) and returns its exception and origin.
    pub fn pop_job(&mut self) -> (i64, JobOrigin) {
        let job = self.jobs.pop().expect("a job to take off");
        self.count_capacity::<Work>(job.work.capacity(), 0);
        (job.exc, job.origin)
    }

    /// Runs the last job in Rust until it is done or meets an instance whose destruction needs a
    /// frame (`docs/design/objects.md` 3.8). An instance whose class has neither a destructor nor
    /// a `BeforeDestruction` written in Pascal is destroyed here without a frame: it is freed at
    /// once (nothing can observe it any more: it has no reference left and its destruction runs
    /// no code) and its fields are let go next (`docs/spec/objects.md` 14.8).
    pub fn run_job(&mut self) -> Result<JobStep, BadRef> {
        let mut work = std::mem::take(&mut self.release_work);
        let kept = work.capacity();
        let result = self.run_job_with(&mut work);
        work.clear();
        work.shrink_to(RELEASE_WORK_KEEP);
        self.count_capacity::<Work>(kept, work.capacity());
        self.release_work = work;
        result
    }

    fn run_job_with(&mut self, work: &mut Vec<Work>) -> Result<JobStep, BadRef> {
        loop {
            let job = self.jobs.last_mut().expect("a job to run");
            let Some(w) = job.work.pop_back() else {
                return Ok(JobStep::Done);
            };
            let h = match self.let_go(w)? {
                Dec::Kept => continue,
                Dec::Freed(body) => {
                    self.push_children(&body, work);
                    self.job_extend(work);
                    continue;
                }
                Dec::Dying(h) => h,
            };
            let index = self.obj_index(h).map_err(|e| BadRef {
                message: format!("destruction of a bad instance {h}: {e:?}"),
            })?;
            let (ty, _) = self.obj_at(index);
            if self.destruction_runs_code(ty) {
                if let Some(Body::Obj { dying, .. }) = &mut self.blocks[index].body {
                    *dying = true;
                }
                return Ok(JobStep::Destroy(h));
            }
            let block = &mut self.blocks[index];
            // Nothing counts the instance between its last reference going and its turn here
            // (no code runs in between), so no tombstone is needed
            debug_assert_eq!(
                block.rc, 0,
                "a frameless destruction of a referenced instance"
            );
            let lent = std::mem::take(&mut block.lent);
            let mut body = block.body.take().expect("a live instance");
            self.used -= body.size();
            if lent != 0 {
                self.forget_loaned_block(index as StrRef + 1, &mut body);
            }
            self.retire(index);
            self.push_children(&body, work);
            self.job_extend(work);
        }
    }

    /// One reported type name per live heap block, in slab order (`docs/spec/diagnostics.md` 8.1).
    /// Uses `func_name` for callable blocks and `cell_name` for captured variables' cells.
    pub fn live_type_names<'h>(
        &'h self,
        func_name: impl Fn(u32) -> String + 'h,
        cell_name: impl Fn(u32, u16) -> String + 'h,
    ) -> impl Iterator<Item = Cow<'h, str>> + 'h {
        self.live_blocks(func_name, cell_name).map(|(name, _)| name)
    }

    /// The bodies of the live blocks, in slab order.
    fn live_bodies(&self) -> impl Iterator<Item = &Body> {
        self.blocks.iter().filter_map(|block| block.body.as_ref())
    }

    pub(super) fn live_block_count(&self) -> usize {
        self.live_bodies().count()
    }

    pub(super) fn live_block_bytes(&self) -> usize {
        self.live_bodies().map(Body::size).sum()
    }

    /// Names and accounted sizes of the blocks still on the heap. The leak report and memory
    /// snapshot share these names (`docs/spec/diagnostics.md` 8.1).
    pub(super) fn live_blocks<'h>(
        &'h self,
        func_name: impl Fn(u32) -> String + 'h,
        cell_name: impl Fn(u32, u16) -> String + 'h,
    ) -> impl Iterator<Item = (Cow<'h, str>, usize)> + 'h {
        self.live_bodies().map(move |body| {
            let name = match body {
                Body::Str(_) => Cow::Borrowed("string"),
                Body::U8(_) => Cow::Borrowed("UTF8String"),
                Body::Agg { ty, .. } | Body::Bytes { ty, .. } | Body::Obj { ty, .. } => {
                    Cow::Borrowed(
                        self.types
                            .get(*ty as usize)
                            .map_or("<unknown>", |t| &t.name),
                    )
                }
                Body::Callable(c) => Cow::Owned(match c.kind {
                    CallableKind::Routine => format!("<closure {}>", func_name(c.func)),
                    CallableKind::Method | CallableKind::ClassMethod => {
                        format!("<method-ref {}>", func_name(c.func))
                    }
                    CallableKind::Closure => unreachable!("a closure has a block of its own"),
                }),
                Body::Closure { func, .. } => Cow::Owned(format!("<closure {}>", func_name(*func))),
                Body::Cell { func, reg, .. } => {
                    Cow::Owned(format!("<cell {}>", cell_name(*func, *reg)))
                }
            };
            (name, body.size())
        })
    }

    /// A new closure (`docs/spec/objects.md` 14.9) of the function `func` (or [`NO_FUNC`])
    /// capturing `caps` (cells it now holds references to; the caller counted them), holding one
    /// reference.
    pub fn alloc_closure(&mut self, func: u32, caps: Caps) -> Result<StrRef, OutOfMemory> {
        self.alloc(Body::Closure { func, caps })
    }

    /// The captured cells of the closure `r`.
    pub fn closure_caps(&self, r: StrRef) -> Result<&[i64], BadRef> {
        match &self.block(r, "capture")?.body {
            Some(Body::Closure { caps, .. }) => Ok(caps),
            _ => Err(BadRef {
                message: format!("{r} is not a closure"),
            }),
        }
    }

    /// The live block a reference points to, or `None` (the fast path's checks; the slow path
    /// reports why with [`Heap::block`]).
    #[inline(always)]
    fn live_body(&self, r: StrRef) -> Option<&Body> {
        self.blocks.get(slab_index(r)?)?.body.as_ref()
    }

    /// Captured cell `i` of the closure `r` (`CapGet`), or `None` when `r` is not a live closure
    /// or `i` is out of its list (the fast path; [`Heap::closure_caps`] reports why).
    #[inline(always)]
    pub fn closure_cap(&self, r: StrRef, i: usize) -> Option<i64> {
        match self.live_body(r)? {
            Body::Closure { caps, .. } => caps.get(i).copied(),
            _ => None,
        }
    }

    /// The function a call of the callable value `r` enters with the arguments where `Call` has
    /// them, for the variants that need no `Self` (`CallClosure`; `docs/spec/objects.md` 14.9):
    /// `(function, true)` for a closure (the block is the hidden first argument), `(function,
    /// false)` for a routine. `None` for the other variants and anything not a callable value
    /// (the fast path; [`Heap::callable`] reports why).
    #[inline(always)]
    pub fn plain_callee(&self, r: StrRef) -> Option<(u32, bool)> {
        match self.live_body(r)? {
            Body::Closure { func, .. } => Some((*func, true)),
            Body::Callable(Callable {
                kind: CallableKind::Routine,
                func,
                ..
            }) => Some((*func, false)),
            _ => None,
        }
    }

    /// A new cell (`docs/spec/objects.md` 14.9) with a value of kind `kind` (0, 1 or 2), at its
    /// default value, made by the register `reg` of the function `func`; holding one reference.
    pub fn alloc_cell(&mut self, kind: u8, func: u32, reg: u16) -> Result<StrRef, OutOfMemory> {
        self.alloc(Body::Cell {
            kind,
            value: 0,
            func,
            reg,
        })
    }

    /// The value slot of the cell `r` (`docs/spec/objects.md` 14.9).
    pub fn cell_mut(&mut self, r: StrRef) -> Result<&mut i64, BadRef> {
        if self.cell_value_mut(r).is_none() {
            return Err(self.not_a_cell(r));
        }
        Ok(self.cell_value_mut(r).expect("checked just above"))
    }

    /// The value of the cell `r`.
    pub fn cell_get(&self, r: StrRef) -> Result<i64, BadRef> {
        self.cell_value(r).ok_or_else(|| self.not_a_cell(r))
    }

    /// The value of the cell `r`, or `None` when `r` is not a live cell (the fast path).
    #[inline(always)]
    pub fn cell_value(&self, r: StrRef) -> Option<i64> {
        match self.live_body(r)? {
            Body::Cell { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// The value slot of the cell `r`, or `None` when `r` is not a live cell (the fast path).
    #[inline(always)]
    pub fn cell_value_mut(&mut self, r: StrRef) -> Option<&mut i64> {
        match self.blocks.get_mut(slab_index(r)?)?.body.as_mut()? {
            Body::Cell { value, .. } => Some(value),
            _ => None,
        }
    }

    /// Why `r` is not a live cell.
    #[cold]
    fn not_a_cell(&self, r: StrRef) -> BadRef {
        match self.block(r, "cell") {
            Err(e) => e,
            Ok(_) => BadRef {
                message: format!("{r} is not a cell"),
            },
        }
    }

    /// A new callable value block (`docs/spec/objects.md` 14.9), holding one reference.
    pub fn alloc_callable(&mut self, c: Callable) -> Result<StrRef, OutOfMemory> {
        self.alloc(Body::Callable(c))
    }

    /// The contents of the callable value block `r`.
    pub fn callable(&self, r: StrRef) -> Result<Callable, BadRef> {
        match &self.block(r, "call")?.body {
            Some(Body::Callable(c)) => Ok(*c),
            Some(Body::Closure { func, .. }) => Ok(Callable {
                kind: CallableKind::Closure,
                func: *func,
                target: 0,
            }),
            _ => Err(BadRef {
                message: format!("{r} is not a callable value"),
            }),
        }
    }

    /// A new exception object of class `ty` (an instance of a reference counted class; 14.8)
    /// with `Message` set to `msg` (a string the object now owns) and the other slots at their
    /// default values, holding one reference, as its handle. `force` skips the usage limit
    /// (`EOutOfMemory`, `EStackOverflow`; `docs/spec/exceptions.md` 5.5).
    pub fn alloc_exc(&mut self, ty: u32, msg: i64, force: bool) -> Result<i64, OutOfMemory> {
        let n = self.types.get(ty as usize).map_or(2, |t| t.slots as usize);
        let mut slots = vec![0; n.max(1)];
        slots[crate::bytecode::EXC_SLOT_MESSAGE as usize] = msg;
        let body = Body::Obj {
            ty,
            slots: slots.into_boxed_slice(),
            dying: false,
            dtor: None,
            extra: None,
        };
        if !force && !self.fits(body.size()) {
            return Err(OutOfMemory);
        }
        let index = self.place_tagged(body, TAG_OBJ_COUNTED);
        Ok(make_handle(index, self.blocks[index].tag >> 8))
    }

    /// A string block, skipping the usage limit when `force` (the message of a VM exception).
    pub fn alloc_str_force(&mut self, s: PStr) -> StrRef {
        if s.is_empty() {
            return 0;
        }
        self.place(Body::Str(s))
    }

    /// The live exception object a handle refers to (an instance; the VM checked the class when
    /// it was raised).
    fn exc(&self, h: i64) -> Result<(u32, &[i64], Option<&ExcExtra>), BadRef> {
        let index = self.obj_index(h).map_err(|e| BadRef {
            message: format!("exception {h} is not a live instance: {e:?}"),
        })?;
        match &self.blocks[index].body {
            Some(Body::Obj {
                ty, slots, extra, ..
            }) => Ok((*ty, slots, extra.as_deref())),
            _ => unreachable!("obj_index returns instances"),
        }
    }

    /// The live exception object a handle refers to, for writing (with its body's size counted
    /// again by the caller).
    fn exc_mut(&mut self, h: i64) -> Result<&mut Body, BadRef> {
        let index = self.obj_index(h).map_err(|e| BadRef {
            message: format!("exception {h} is not a live instance: {e:?}"),
        })?;
        Ok(self.blocks[index]
            .body
            .as_mut()
            .expect("obj_index returns live blocks"))
    }

    /// The class (type index) of an exception object.
    pub fn exc_type(&self, h: i64) -> Result<u32, BadRef> {
        Ok(self.exc(h)?.0)
    }

    /// Slot `i` of an exception object (`None` outside the object).
    pub fn exc_slot(&self, h: i64, i: u32) -> Result<Option<i64>, BadRef> {
        Ok(self.exc(h)?.1.get(i as usize).copied())
    }

    /// Replaces slot `i` of an exception object (the object is a reference type, written while
    /// shared) and returns the old value (`None` outside the object).
    pub fn exc_replace(&mut self, h: i64, i: u32, v: i64) -> Result<Option<i64>, BadRef> {
        match self.exc_mut(h)? {
            Body::Obj { slots, .. } => {
                Ok(slots.get_mut(i as usize).map(|s| std::mem::replace(s, v)))
            }
            _ => unreachable!("exc_mut returns instances"),
        }
    }

    /// The stack trace of an exception object (`None` before it is raised).
    pub fn exc_trace(&self, h: i64) -> Result<Option<&Trace>, BadRef> {
        Ok(self.exc(h)?.2.and_then(|x| x.trace.as_ref()))
    }

    /// Records the stack trace of an exception object (counted toward usage without the limit,
    /// like the objects of VM exceptions).
    pub fn set_exc_trace(&mut self, h: i64, t: Trace) -> Result<(), BadRef> {
        self.change_used(0, t.frames.len() * 8);
        match self.exc_mut(h)? {
            Body::Obj { extra, .. } => extra.get_or_insert_with(Box::default).trace = Some(t),
            _ => unreachable!("exc_mut returns instances"),
        }
        Ok(())
    }

    /// The subexpressions and values a failed `Assert` recorded on an exception object
    /// (`EAssertionFailed.Values`).
    pub fn exc_values(&self, h: i64) -> Result<&[(String, String)], BadRef> {
        Ok(self.exc(h)?.2.map_or(&[][..], |x| &x.values))
    }

    /// Appends a subexpression and its value to `EAssertionFailed.Values` of an exception object
    /// (counted toward usage without the limit, like its stack trace).
    pub fn exc_push_value(&mut self, h: i64, expr: String, value: String) -> Result<(), BadRef> {
        let item = (expr, value);
        self.change_used(0, values_size(std::slice::from_ref(&item)));
        match self.exc_mut(h)? {
            Body::Obj { extra, .. } => extra.get_or_insert_with(Box::default).values.push(item),
            _ => unreachable!("exc_mut returns instances"),
        }
        Ok(())
    }

    /// The exceptions in `Suppressed` of an exception object.
    pub fn exc_suppressed(&self, h: i64) -> Result<&[i64], BadRef> {
        Ok(self.exc(h)?.2.map_or(&[][..], |x| &x.suppressed))
    }

    /// Appends `v` (whose reference the object takes) to `Suppressed` of an exception object
    /// (counted toward usage without the limit).
    pub fn exc_push_suppressed(&mut self, h: i64, v: i64) -> Result<(), BadRef> {
        self.change_used(0, 8);
        match self.exc_mut(h)? {
            Body::Obj { extra, .. } => extra.get_or_insert_with(Box::default).suppressed.push(v),
            _ => unreachable!("exc_mut returns instances"),
        }
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

    /// A class and its ancestors, the class first (by type index); empty for a type that is not
    /// a class.
    pub fn class_chain(&self, ty: u32) -> Vec<u32> {
        self.ancestors
            .get(ty as usize)
            .map_or_else(Vec::new, |l| l.iter().rev().copied().collect())
    }

    /// The name of a type (the class name of an exception, by its type entry).
    pub fn type_name(&self, ty: u32) -> &str {
        self.types
            .get(ty as usize)
            .map_or("<unknown>", |t| t.name.as_str())
    }

    /// Counts growth of `regs` and the like toward usage.
    pub fn account(&mut self, bytes: usize) -> Result<(), OutOfMemory> {
        if !self.fits(bytes) {
            return Err(OutOfMemory);
        }
        self.change_used(0, bytes);
        Ok(())
    }

    /// A string block with a reference count of 1 (tests that build blocks directly).
    #[cfg(test)]
    pub(crate) fn alloc_test_str(&mut self, s: &str) -> StrRef {
        let s = PStr::from_text(s).expect("test strings are small");
        self.alloc_str(s).expect("test strings fit the limit")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A heap that reuses indexes, with the built-in types only (`TObject` is type 19).
    fn heap() -> Heap {
        Heap::new(None, true, &[])
    }

    // The generation word and the remembered hash of a text sit in the block without making it
    // larger than before: the exception object's trace and `Suppressed` moved into a box
    // (`docs/design/objects.md` 3.2 B), and the hash fills the padding (`docs/design/vm.md` 3.18).
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn block_stays_small() {
        assert!(
            std::mem::size_of::<Block>() <= 56,
            "{}",
            std::mem::size_of::<Block>()
        );
    }

    // A string block remembers its hash once worked out, and forgets it when its content is
    // rewritten in place, so that the next `HashStr` sees the new content (`docs/spec/bytecode.md`
    // 2.21).
    #[test]
    fn a_string_block_remembers_its_hash_until_rewritten() {
        let keys = HashKeys::new(0);
        let mut h = heap();
        let s = h.alloc_str(PStr::Utf16(vec![0x61, 0x62])).unwrap();
        let x = h.alloc_str(PStr::Utf16(vec![0x63])).unwrap();
        let remembered = |h: &Heap| h.blocks[(s - 1) as usize].hash;
        assert_eq!(remembered(&h), 0);
        let ab = h.str_hash(s, &keys).unwrap();
        assert_eq!(remembered(&h), ab);
        assert_eq!(h.str_hash(s, &keys).unwrap(), ab);
        h.append_str(s, x).unwrap().unwrap();
        assert_eq!(remembered(&h), 0);
        assert_eq!(
            h.str_hash(s, &keys).unwrap(),
            keys.units([0x61, 0x62, 0x63].into_iter())
        );
        assert!(h.unique_str_mut(s).unwrap().is_some());
        assert_eq!(remembered(&h), 0);
    }

    // A short string kept in place makes the string value no larger than one holding a buffer, so
    // that blocks do not grow for it (`docs/design/vm.md` 3.3).
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn short_string_fits_in_the_value() {
        assert_eq!(std::mem::size_of::<PStr>(), 32);
    }

    // The longest one-byte string kept in place is SHORT_LEN characters, and it counts no buffer
    // toward usage; one more character goes to a buffer of its own.
    #[test]
    fn short_string_boundary() {
        let at = PStr::ascii(&[b'a'; SHORT_LEN]).expect("small");
        let over = PStr::ascii(&[b'a'; SHORT_LEN + 1]).expect("small");
        assert!(matches!(at, PStr::Short(_)), "{at:?}");
        assert!(matches!(over, PStr::Ascii(_)), "{over:?}");
        assert_eq!((at.heap_size(), over.heap_size()), (0, SHORT_LEN + 1));
    }

    // A handle stops matching once its instance is freed, also after a new instance reuses the
    // slab index: the generation in the handle differs (`docs/spec/objects.md` 14.4).
    #[test]
    fn freed_handle_never_matches_a_new_instance() {
        let mut h = heap();
        let a = h.alloc_obj(crate::bytecode::TOBJECT_TYPE).unwrap().unwrap();
        assert!(h.end_destroy(a).is_ok());
        assert_eq!(h.obj_index(a), Err(BadHandle::Freed));
        let b = h.alloc_obj(crate::bytecode::TOBJECT_TYPE).unwrap().unwrap();
        assert_eq!(a & 0xFFFF_FFFF, b & 0xFFFF_FFFF);
        assert_ne!(a, b);
        assert_eq!(h.obj_index(a), Err(BadHandle::Freed));
        assert!(h.obj_index(b).is_ok());
        assert_eq!(h.end_destroy(a).err(), Some(BadHandle::Freed));
    }

    // An index whose generation is used up is retired: no later block takes it, so an old handle
    // cannot come to match again when the generation would wrap.
    #[test]
    fn exhausted_generation_retires_the_index() {
        let mut h = heap();
        let a = h.alloc_obj(crate::bytecode::TOBJECT_TYPE).unwrap().unwrap();
        let index = (a & 0xFFFF_FFFF) as usize - 1;
        h.blocks[index].tag = ((MAX_GENERATION - 1) << 8) | TAG_OBJ_MANUAL;
        let last = make_handle(index, MAX_GENERATION - 1);
        assert!(h.end_destroy(last).is_ok());
        let b = h.alloc_obj(crate::bytecode::TOBJECT_TYPE).unwrap().unwrap();
        assert_ne!((b & 0xFFFF_FFFF) as usize - 1, index);
    }

    // A handle whose index is out of the slab, or whose block is not an instance, is an internal
    // error (forged bytecode), not `EAccessViolation`.
    #[test]
    fn forged_handles_are_fatal() {
        let mut h = heap();
        let s = h.alloc_test_str("x");
        assert!(matches!(h.obj_index(s), Err(BadHandle::Fatal(_))));
        assert!(matches!(h.obj_index(99), Err(BadHandle::Fatal(_))));
        assert_eq!(h.obj_index(0), Err(BadHandle::Nil));
    }
}
