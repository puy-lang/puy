//! Heap: string storage and usage accounting (`docs/design/vm.md` 3.3, 3.11; requirements §5.5).
//!
//! M1 has no reference counting; strings are kept until the isolate ends.

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

/// A string reference (slab index + 1; 0 is the empty string).
pub type StrRef = i64;

/// The isolate's heap.
pub struct Heap {
    strings: Vec<PStr>,
    used: usize,
    limit: Option<usize>,
    empty: PStr,
}

/// Allocation failure (`EOutOfMemory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

impl Heap {
    pub fn new(limit: Option<usize>) -> Self {
        Heap {
            strings: Vec::new(),
            used: 0,
            limit,
            empty: PStr::Ascii(Box::new([])),
        }
    }

    pub fn used(&self) -> usize {
        self.used
    }

    /// Puts a string on the heap and returns a reference. Checks the length limit and the usage limit.
    pub fn alloc_str(&mut self, s: PStr) -> Result<StrRef, OutOfMemory> {
        if s.is_empty() {
            return Ok(0);
        }
        if s.len() > MAX_STRING_LEN {
            return Err(OutOfMemory);
        }
        let size = s.byte_size() + std::mem::size_of::<PStr>();
        if let Some(limit) = self.limit
            && self.used + size > limit
        {
            return Err(OutOfMemory);
        }
        self.used += size;
        self.strings.push(s);
        Ok(self.strings.len() as i64)
    }

    /// The string a reference points to (0 is empty).
    pub fn get(&self, r: StrRef) -> &PStr {
        if r <= 0 {
            return &self.empty;
        }
        self.strings.get(r as usize - 1).unwrap_or(&self.empty)
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
}
