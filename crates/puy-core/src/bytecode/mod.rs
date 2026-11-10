//! Bytecode (`docs/spec/bytecode.md`): instruction format, reading, writing and verifying modules (`.puyc`), and disassembly.

pub mod op;

use std::fmt;

/// Format version (3.1).
pub const FORMAT_VERSION: u32 = 2;

/// Builds an instruction word.
pub fn encode(op: u8, a: u32, b: u32, c: u32) -> u32 {
    op as u32 | ((a & 0xFF) << 8) | ((b & 0xFF) << 16) | ((c & 0xFF) << 24)
}

/// Extension word (upper 8 bits of `A` / `B` / `C`).
pub fn encode_ext(a: u32, b: u32, c: u32) -> u32 {
    (((a >> 8) & 0xFF) << 8) | (((b >> 8) & 0xFF) << 16) | (((c >> 8) & 0xFF) << 24)
}

/// Splits a word: `(op, A, B, C)` (8 bits each).
pub fn decode(word: u32) -> (u8, u32, u32, u32) {
    (
        (word & 0xFF) as u8,
        (word >> 8) & 0xFF,
        (word >> 16) & 0xFF,
        (word >> 24) & 0xFF,
    )
}

/// `Bx` joins `B/C` of the main word as the lower 16 bits and `B/C` of the extension word as the upper 16 bits.
/// Do not overlap `B` and `C` widened to 16 bits by shifting one of them by 8 bits.
pub(crate) fn decode_bx(word: u32, extension: Option<u32>) -> u32 {
    (word >> 16) | extension.map_or(0, |ext| ext & 0xFFFF_0000)
}

/// A constant pool entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstEntry {
    Int(i64),
    /// Kind 3: an `f64`, as its bits (`docs/spec/types.md` 8.4; a `Single` constant too).
    Float(u64),
    /// Kind 2: a UTF-8 string.
    Str(String),
    /// Kind 7: a `UTF8String` (`docs/spec/types.md` 8.5): UTF-8, encoded like kind 2.
    U8(String),
    /// Kind 4: a sequence of UTF-16 code units (may contain lone surrogates).
    Utf16(Vec<u16>),
    /// Kind 5: a record or static array constant (3.2): the type index (64 and up) and the
    /// constant index of each slot.
    Agg {
        ty: u32,
        items: Vec<u32>,
    },
    /// Kind 6: a big set constant (`docs/spec/types.md` 8.3): the type index of the set (kind 5)
    /// and its bit words (bit `i` of word `i / 64` is the ordinal `i`; trailing zero words dropped).
    Set {
        ty: u32,
        words: Vec<u64>,
    },
}

/// The first type index of the type section; 0–63 are the VM's built-in types (3.2).
pub const FIRST_TYPE_INDEX: u32 = 64;

/// Kind of a type section entry (3.2).
pub const TYPE_RECORD: u8 = 1;
pub const TYPE_ARRAY: u8 = 2;
pub const TYPE_DYNARRAY: u8 = 3;
pub const TYPE_EXCEPTION: u8 = 4;
/// A big set: its number of bits (`docs/spec/types.md` 8.3).
pub const TYPE_SET: u8 = 5;
/// An enumeration: the table of its value names (`docs/spec/types.md` 8.1).
pub const TYPE_ENUM: u8 = 6;

/// A type section entry (3.2): records, static arrays, dynamic arrays and exception classes in M2a.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeEntry {
    /// [`TYPE_RECORD`], [`TYPE_ARRAY`], [`TYPE_DYNARRAY`] or [`TYPE_EXCEPTION`] (0 for an unused
    /// built-in type index).
    pub kind: u8,
    /// Fields of a record, elements of a static array (0 for a dynamic array), slots of an
    /// exception class (`docs/spec/exceptions.md` 6.1).
    pub slots: u32,
    /// Whether each slot holds a heap reference: one per field for a record or slot of an
    /// exception class, one for all the elements of an array.
    pub refs: Vec<bool>,
    /// The fully qualified name (`Hello.TPoint`), or the written form of an anonymous type.
    pub name: String,
    /// The parent's type index of an exception class (`None` for the root `Exception` and for
    /// the other kinds).
    pub parent: Option<u32>,
    /// The values of an enumeration in declaration order: (ordinal, name as declared). Empty for
    /// the other kinds.
    pub values: Vec<(i64, String)>,
}

impl TypeEntry {
    /// The name of an ordinal of an enumeration: the first value declared with it, or the
    /// ordinal in decimal (`docs/spec/types.md` 8.1).
    pub fn enum_name(&self, ord: i64) -> String {
        self.values
            .iter()
            .find(|(o, _)| *o == ord)
            .map_or_else(|| ord.to_string(), |(_, n)| n.clone())
    }

    /// Whether slot `i` holds a heap reference.
    pub fn slot_is_ref(&self, i: usize) -> bool {
        match self.kind {
            TYPE_RECORD | TYPE_EXCEPTION => self.refs.get(i).copied().unwrap_or(false),
            _ => self.refs.first().copied().unwrap_or(false),
        }
    }
}

/// Built-in exception classes (`docs/spec/exceptions.md` chapter 2): the type index, the name,
/// the parent and whether the class adds a slot (a string or not). Index 12 is
/// `TArray<Exception>` ([`EXCEPTION_ARRAY_TYPE`]), so the M2b classes start at 13.
pub const BUILTIN_EXCEPTIONS: &[(u32, &str, Option<u32>, Option<bool>)] = &[
    (0, "Exception", None, None),
    (1, "EIntError", Some(0), None),
    (2, "EDivByZero", Some(1), None),
    (3, "ERangeError", Some(1), None),
    (4, "EIntOverflow", Some(1), None),
    (5, "EConvertError", Some(0), Some(true)),
    (6, "EInOutError", Some(0), Some(false)),
    (7, "EAssertionFailed", Some(0), None),
    (8, "EStackOverflow", Some(0), None),
    (9, "EOutOfMemory", Some(0), None),
    (10, "ENotSupportedException", Some(0), None),
    (11, "EAccessViolation", Some(0), None),
    (13, "EMathError", Some(0), None),
    (14, "EOverflow", Some(13), None),
    (15, "EInvalidOp", Some(13), None),
    (16, "EEncodingError", Some(0), None),
    (17, "EZeroDivide", Some(13), None),
];

/// One more than the largest type index of a built-in type.
pub const BUILTIN_TYPE_END: u32 = 18;

/// Type indexes of the built-in exception classes the VM raises.
pub mod exc {
    pub const EXCEPTION: u32 = 0;
    pub const DIV_BY_ZERO: u32 = 2;
    pub const RANGE_ERROR: u32 = 3;
    pub const INT_OVERFLOW: u32 = 4;
    pub const CONVERT_ERROR: u32 = 5;
    pub const IN_OUT_ERROR: u32 = 6;
    pub const ASSERTION_FAILED: u32 = 7;
    pub const STACK_OVERFLOW: u32 = 8;
    pub const OUT_OF_MEMORY: u32 = 9;
    pub const NOT_SUPPORTED: u32 = 10;
    pub const ACCESS_VIOLATION: u32 = 11;
    pub const OVERFLOW: u32 = 14;
    pub const INVALID_OP: u32 = 15;
    pub const ENCODING_ERROR: u32 = 16;
    pub const ZERO_DIVIDE: u32 = 17;
}

/// The type index of `TArray<Exception>`, the array `Suppressed` returns (`docs/spec/exceptions.md` 6.1).
pub const EXCEPTION_ARRAY_TYPE: u32 = 12;

/// Slot numbers of an exception object (`docs/spec/exceptions.md` 6.1).
pub const EXC_SLOT_MESSAGE: u32 = 0;
pub const EXC_SLOT_INNER: u32 = 1;
/// The first slot a class adds (`EConvertError.Value`, `EInOutError.ErrorCode`).
pub const EXC_SLOT_EXTRA: u32 = 2;

/// Information `ExcInfo` reads (`docs/spec/exceptions.md` 6.2).
pub const EXC_INFO_STACK_TRACE: u32 = 0;
pub const EXC_INFO_SUPPRESSED: u32 = 1;
pub const EXC_INFO_CLASS_NAME: u32 = 2;

/// The kinds of the elements of `Format`'s `array of const` (`docs/spec/bytecode.md` 2.16).
/// Integers are `0..=7`, their width code of the `Narrow` family (2.2).
pub const FORMAT_KIND_INT_LAST: i64 = 7;
pub const FORMAT_KIND_CHAR: i64 = 8;
pub const FORMAT_KIND_BOOLEAN: i64 = 9;
pub const FORMAT_KIND_STRING: i64 = 10;
pub const FORMAT_KIND_UTF8: i64 = 11;
pub const FORMAT_KIND_SINGLE: i64 = 12;
pub const FORMAT_KIND_DOUBLE: i64 = 13;
pub const FORMAT_KIND_CURRENCY: i64 = 14;

/// The ordinals of `TReplaceFlag` (`docs/spec/semantics.md` 5.3): `StrReplace` reads them as the
/// bits of a `TReplaceFlags` (2.12).
pub const REPLACE_FLAG_ALL: u32 = 0;
pub const REPLACE_FLAG_IGNORE_CASE: u32 = 1;

/// The ordinals of `TCaseMapping` and `TCaseFold` (`docs/spec/semantics.md` 5.3): the last
/// operand of `StrUpper` / `StrLower` and `AnsiSameText` / `AnsiCompareText` (2.16). Any other
/// value reads as the first ordinal (`cmSimple`, `cfLower`).
pub const CASE_MAPPING_FULL: i64 = 1;
pub const CASE_FOLD_UPPER: i64 = 1;

/// The kind of real that `StrToFloatIO` reads (its `C` operand; 2.17).
pub const FLOAT_IO_DOUBLE: i64 = 0;
pub const FLOAT_IO_SINGLE: i64 = 1;
pub const FLOAT_IO_CURRENCY: i64 = 2;

/// The VM's built-in types, type indexes 0 to 63 (`docs/spec/exceptions.md` 6.1). Unused
/// indexes have kind 0.
pub fn builtin_types() -> &'static [TypeEntry] {
    static TYPES: std::sync::OnceLock<Vec<TypeEntry>> = std::sync::OnceLock::new();
    TYPES.get_or_init(|| {
        let empty = || TypeEntry {
            kind: 0,
            slots: 0,
            refs: Vec::new(),
            name: String::new(),
            parent: None,
            values: Vec::new(),
        };
        let mut v: Vec<TypeEntry> = (0..BUILTIN_TYPE_END).map(|_| empty()).collect();
        for (index, name, parent, extra) in BUILTIN_EXCEPTIONS {
            let mut refs = vec![true, true];
            if let Some(r) = extra {
                refs.push(*r);
            }
            v[*index as usize] = TypeEntry {
                kind: TYPE_EXCEPTION,
                slots: refs.len() as u32,
                refs,
                name: name.to_string(),
                parent: *parent,
                values: Vec::new(),
            };
        }
        v[EXCEPTION_ARRAY_TYPE as usize] = TypeEntry {
            kind: TYPE_DYNARRAY,
            slots: 0,
            refs: vec![true],
            name: "TArray<Exception>".into(),
            parent: None,
            values: Vec::new(),
        };
        while v.len() < FIRST_TYPE_INDEX as usize {
            v.push(TypeEntry {
                kind: 0,
                slots: 0,
                refs: Vec::new(),
                name: String::new(),
                parent: None,
                values: Vec::new(),
            });
        }
        v
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    /// Fully qualified name (`Hello.Square`).
    pub name: String,
    pub params: u16,
    pub regs: u16,
    pub code: Vec<u32>,
    /// Line table: (word position, line), in ascending order of position.
    pub lines: Vec<(u32, u32)>,
    /// Local variable names: (register, name).
    pub locals: Vec<(u16, String)>,
    /// Registers holding managed values, in release order (the implicit `finally`; 3.2,
    /// `docs/design/lifetime.md` 3.8). Every entry has the kind [`MANAGED_HEAP_REF`].
    pub managed: Vec<u16>,
    /// Source file (index into the unit section).
    pub file: u32,
    /// The handler table, inner entries first (`docs/spec/exceptions.md` 6.3).
    pub handlers: Vec<HandlerEntry>,
    /// For a nested routine taking the static link as its first argument, the function it is
    /// nested in, whose frame the link points to (`docs/spec/semantics.md` 4.9).
    pub outer: Option<u32>,
}

/// `outer` of a function that is not nested (in the module file).
const NO_OUTER: u32 = u32::MAX;

/// `HandlerEntry::kind` of a `try..except`.
pub const HANDLER_EXCEPT: u8 = 1;
/// `HandlerEntry::kind` of a `try..finally`.
pub const HANDLER_FINALLY: u8 = 2;

/// An entry of a function's handler table (`docs/spec/exceptions.md` 6.3). Positions are word
/// positions; the ranges do not include their ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandlerEntry {
    /// [`HANDLER_EXCEPT`] or [`HANDLER_FINALLY`].
    pub kind: u8,
    pub body_start: u32,
    pub body_end: u32,
    pub handler_start: u32,
    /// For a `finally`, the position of its `EndFinally`.
    pub handler_end: u32,
    /// `except`: the register holding the exception. `finally`: the state register `rA`; `r(A+1)`
    /// holds the exception being propagated.
    pub reg: u16,
}

impl HandlerEntry {
    /// Whether `pc` is in the protected body.
    pub fn in_body(&self, pc: u32) -> bool {
        self.body_start <= pc && pc < self.body_end
    }

    /// Whether `pc` is in the handler (the `except` block or the `finally` block).
    pub fn in_handler(&self, pc: u32) -> bool {
        self.handler_start <= pc && pc < self.handler_end
    }
}

impl Function {
    /// The line of a word position.
    pub fn line_at(&self, pos: u32) -> u32 {
        let mut line = self.lines.first().map(|l| l.1).unwrap_or(1);
        for &(p, l) in &self.lines {
            if p <= pos {
                line = l;
            } else {
                break;
            }
        }
        line
    }
}

/// `GlobalEntry::kind` of an integer global variable (including `Boolean` / `Char`).
pub const GLOBAL_INT: u32 = 0;
/// `GlobalEntry::kind` of a global variable holding a heap reference (a string; released when the isolate finishes).
pub const GLOBAL_REF: u32 = 1;
/// `GlobalEntry::kind` of `System.ExitCode` (an `Integer`; at most one; `docs/spec/bytecode.md` 3.2).
/// The VM reads it for the exit code and `Halt` writes it.
pub const GLOBAL_EXIT_CODE: u32 = 2;

/// Kind of a managed register entry (3.2): a heap reference.
pub const MANAGED_HEAP_REF: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalEntry {
    pub name: String,
    /// [`GLOBAL_INT`], [`GLOBAL_REF`] or [`GLOBAL_EXIT_CODE`].
    pub kind: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitEntry {
    pub name: String,
    pub path: String,
    pub init: Option<u32>,
    pub fini: Option<u32>,
    /// The unit's global variables: the first index and the count (`docs/spec/units.md` 6.3).
    pub first_global: u32,
    pub global_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    pub name: String,
    pub params: u16,
    pub ret: bool,
}

/// A verified module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub consts: Vec<ConstEntry>,
    /// The type section (index 0 here is type index [`FIRST_TYPE_INDEX`]).
    pub types: Vec<TypeEntry>,
    pub funcs: Vec<Function>,
    pub globals: Vec<GlobalEntry>,
    pub entry: u32,
    pub units: Vec<UnitEntry>,
    pub hosts: Vec<HostEntry>,
}

/// A load failure (F0004).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// `invalid bytecode module: …`
    Invalid(String),
    /// `bytecode was built with puy …, this is puy …`
    Version { built: String, this: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Invalid(m) => write!(f, "invalid bytecode module: {m}"),
            LoadError::Version { built, this } => {
                write!(f, "bytecode was built with puy {built}, this is puy {this}")
            }
        }
    }
}

const NONE_INDEX: u32 = 0xFFFF_FFFF;

// ---- Writing ----

struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn pad4(&mut self) {
        while !self.buf.len().is_multiple_of(4) {
            self.buf.push(0);
        }
    }
    fn lp_utf8(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self.pad4();
    }
}

impl Module {
    /// Encodes as `.puyc` bytes.
    pub fn encode(&self, puy_version: &str) -> Vec<u8> {
        let mut sections: Vec<(u32, Vec<u8>)> = vec![
            (1, self.encode_consts()),
            (2, self.encode_funcs()),
            (3, self.encode_globals()),
            (4, self.encode_units()),
            (5, self.encode_hosts()),
        ];
        // The type section is written only when it has entries (3.1)
        if !self.types.is_empty() {
            sections.push((6, self.encode_types()));
        }
        let mut w = Writer { buf: Vec::new() };
        w.buf.extend_from_slice(b"PUYC");
        w.u32(FORMAT_VERSION);
        w.lp_utf8(puy_version);
        w.u32(sections.len() as u32);
        let table_at = w.buf.len();
        for _ in &sections {
            w.u32(0);
            w.u32(0);
            w.u32(0);
        }
        let mut offsets = Vec::new();
        for (kind, body) in &sections {
            w.pad4();
            offsets.push((*kind, w.buf.len() as u32, body.len() as u32));
            w.buf.extend_from_slice(body);
        }
        w.pad4();
        for (i, (kind, off, len)) in offsets.iter().enumerate() {
            let at = table_at + i * 12;
            w.buf[at..at + 4].copy_from_slice(&kind.to_le_bytes());
            w.buf[at + 4..at + 8].copy_from_slice(&off.to_le_bytes());
            w.buf[at + 8..at + 12].copy_from_slice(&len.to_le_bytes());
        }
        w.buf
    }

    fn encode_consts(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.consts.len() as u32);
        for c in &self.consts {
            match c {
                ConstEntry::Int(v) => {
                    w.u8(1);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.i64(*v);
                }
                ConstEntry::Float(b) => {
                    w.u8(3);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.i64(*b as i64);
                }
                ConstEntry::Str(s) => {
                    w.u8(2);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.lp_utf8(s);
                }
                ConstEntry::U8(s) => {
                    w.u8(7);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.lp_utf8(s);
                }
                ConstEntry::Utf16(u) => {
                    w.u8(4);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.u32(u.len() as u32);
                    for x in u {
                        w.u16(*x);
                    }
                    w.pad4();
                }
                ConstEntry::Agg { ty, items } => {
                    w.u8(5);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.u32(*ty);
                    w.u32(items.len() as u32);
                    for k in items {
                        w.u32(*k);
                    }
                }
                ConstEntry::Set { ty, words } => {
                    w.u8(6);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.u32(*ty);
                    w.u32(words.len() as u32);
                    for x in words {
                        w.i64(*x as i64);
                    }
                }
            }
        }
        w.buf
    }

    fn encode_types(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.types.len() as u32);
        for t in &self.types {
            w.u8(t.kind);
            w.u8(0);
            w.u8(0);
            w.u8(0);
            w.u32(t.slots);
            for r in &t.refs {
                w.u8(*r as u8);
            }
            w.pad4();
            w.lp_utf8(&t.name);
            if t.kind == TYPE_EXCEPTION {
                w.u32(t.parent.unwrap_or(0));
            }
            if t.kind == TYPE_ENUM {
                w.u32(t.values.len() as u32);
                for (ord, name) in &t.values {
                    w.i64(*ord);
                    w.lp_utf8(name);
                }
            }
        }
        w.buf
    }

    fn encode_funcs(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.funcs.len() as u32);
        for f in &self.funcs {
            w.lp_utf8(&f.name);
            w.u16(f.params);
            w.u16(f.regs);
            w.u32(f.code.len() as u32);
            for x in &f.code {
                w.u32(*x);
            }
            w.u32(f.handlers.len() as u32);
            for h in &f.handlers {
                w.u8(h.kind);
                w.u8(0);
                w.u16(0);
                w.u32(h.body_start);
                w.u32(h.body_end);
                w.u32(h.handler_start);
                w.u32(h.handler_end);
                w.u16(h.reg);
                w.u16(0);
            }
            w.u32(f.managed.len() as u32);
            for r in &f.managed {
                w.u16(*r);
                w.u16(MANAGED_HEAP_REF);
            }
            w.u32(f.lines.len() as u32);
            for (p, l) in &f.lines {
                w.u32(*p);
                w.u32(*l);
            }
            w.u32(f.locals.len() as u32);
            for (r, n) in &f.locals {
                w.u16(*r);
                w.u16(0);
                w.lp_utf8(n);
            }
            w.u32(f.file);
            w.u32(f.outer.unwrap_or(NO_OUTER));
        }
        w.buf
    }

    fn encode_globals(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.globals.len() as u32);
        for g in &self.globals {
            w.lp_utf8(&g.name);
            w.u32(g.kind);
        }
        w.buf
    }

    fn encode_units(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.entry);
        w.u32(self.units.len() as u32);
        for u in &self.units {
            w.lp_utf8(&u.name);
            w.lp_utf8(&u.path);
            w.u32(u.init.unwrap_or(NONE_INDEX));
            w.u32(u.fini.unwrap_or(NONE_INDEX));
            w.u32(u.first_global);
            w.u32(u.global_count);
        }
        w.buf
    }

    fn encode_hosts(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.hosts.len() as u32);
        for h in &self.hosts {
            w.lp_utf8(&h.name);
            w.u16(h.params);
            w.u16(h.ret as u16);
        }
        w.buf
    }
}

// ---- Reading ----

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn need(&self, n: usize) -> Result<(), LoadError> {
        if self.pos + n > self.buf.len() {
            Err(LoadError::Invalid("unexpected end of section".into()))
        } else {
            Ok(())
        }
    }
    fn u8(&mut self) -> Result<u8, LoadError> {
        self.need(1)?;
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, LoadError> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, LoadError> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }
    fn i64(&mut self) -> Result<i64, LoadError> {
        self.need(8)?;
        let v = i64::from_le_bytes(self.buf[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        Ok(v)
    }
    fn pad4(&mut self) -> Result<(), LoadError> {
        while !self.pos.is_multiple_of(4) {
            self.u8()?;
        }
        Ok(())
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], LoadError> {
        self.need(n)?;
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn lp_utf8(&mut self) -> Result<String, LoadError> {
        let n = self.u32()? as usize;
        let b = self.bytes(n)?;
        let s = std::str::from_utf8(b)
            .map_err(|_| LoadError::Invalid("string is not valid UTF-8".into()))?
            .to_string();
        self.pad4()?;
        Ok(s)
    }
    fn count(&mut self, what: &str) -> Result<usize, LoadError> {
        let n = self.u32()? as usize;
        // A count larger than the remaining bytes is invalid
        if n > self.buf.len() - self.pos {
            return Err(LoadError::Invalid(format!("{what} count is too large")));
        }
        Ok(n)
    }
}

impl Module {
    /// Reads from bytes and verifies (3.3).
    pub fn decode(bytes: &[u8], puy_version: &str) -> Result<Module, LoadError> {
        let mut r = Reader { buf: bytes, pos: 0 };
        if r.bytes(4)? != b"PUYC" {
            return Err(LoadError::Invalid("bad magic".into()));
        }
        let fv = r.u32()?;
        if fv != FORMAT_VERSION {
            return Err(LoadError::Invalid(format!(
                "unsupported format version {fv}"
            )));
        }
        let built = r.lp_utf8()?;
        if built != puy_version {
            return Err(LoadError::Version {
                built,
                this: puy_version.to_string(),
            });
        }
        let k = r.u32()? as usize;
        if k > 64 {
            return Err(LoadError::Invalid("too many sections".into()));
        }
        let mut table: Vec<(u32, u32, u32)> = Vec::new();
        for _ in 0..k {
            table.push((r.u32()?, r.u32()?, r.u32()?));
        }
        let mut sections: [Option<&[u8]>; 7] = [None; 7];
        let mut last_end = 0u32;
        for (kind, off, len) in table {
            let end = off
                .checked_add(len)
                .ok_or_else(|| LoadError::Invalid("section out of range".into()))?;
            if end as usize > bytes.len() || off < last_end {
                return Err(LoadError::Invalid("section out of range".into()));
            }
            last_end = end;
            if !(1..=6).contains(&kind) {
                return Err(LoadError::Invalid(format!("unknown section {kind}")));
            }
            if sections[kind as usize].is_some() {
                return Err(LoadError::Invalid(format!("duplicate section {kind}")));
            }
            sections[kind as usize] = Some(&bytes[off as usize..end as usize]);
        }
        if let Some(kind) = (1..=5).find(|k| sections[*k].is_none()) {
            return Err(LoadError::Invalid(format!("missing section {kind}")));
        }
        let consts = Self::decode_consts(sections[1].unwrap())?;
        let funcs = Self::decode_funcs(sections[2].unwrap())?;
        let globals = Self::decode_globals(sections[3].unwrap())?;
        let (entry, units) = Self::decode_units(sections[4].unwrap())?;
        let hosts = Self::decode_hosts(sections[5].unwrap())?;
        let types = match sections[6] {
            Some(b) => Self::decode_types(b)?,
            None => Vec::new(),
        };
        let m = Module {
            consts,
            types,
            funcs,
            globals,
            entry,
            units,
            hosts,
        };
        m.verify()?;
        Ok(m)
    }

    fn decode_consts(buf: &[u8]) -> Result<Vec<ConstEntry>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("constant")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let kind = r.u8()?;
            r.u8()?;
            r.u8()?;
            r.u8()?;
            out.push(match kind {
                1 => ConstEntry::Int(r.i64()?),
                2 => ConstEntry::Str(r.lp_utf8()?),
                3 => ConstEntry::Float(r.i64()? as u64),
                7 => ConstEntry::U8(r.lp_utf8()?),
                4 => {
                    let c = r.u32()? as usize;
                    if c > buf.len() {
                        return Err(LoadError::Invalid("string constant is too long".into()));
                    }
                    let mut u = Vec::with_capacity(c);
                    for _ in 0..c {
                        u.push(r.u16()?);
                    }
                    r.pad4()?;
                    if !u.iter().any(|x| (0xD800..0xE000).contains(x))
                        || String::from_utf16(&u).is_ok()
                    {
                        return Err(LoadError::Invalid(
                            "UTF-16 constant without a lone surrogate".into(),
                        ));
                    }
                    ConstEntry::Utf16(u)
                }
                5 => {
                    let ty = r.u32()?;
                    let c = r.count("aggregate element")?;
                    let mut items = Vec::with_capacity(c);
                    for _ in 0..c {
                        items.push(r.u32()?);
                    }
                    ConstEntry::Agg { ty, items }
                }
                6 => {
                    let ty = r.u32()?;
                    let c = r.count("set word")?;
                    let mut words = Vec::with_capacity(c);
                    for _ in 0..c {
                        words.push(r.i64()? as u64);
                    }
                    ConstEntry::Set { ty, words }
                }
                k => return Err(LoadError::Invalid(format!("unknown constant kind {k}"))),
            });
        }
        Ok(out)
    }

    fn decode_types(buf: &[u8]) -> Result<Vec<TypeEntry>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("type")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let kind = r.u8()?;
            r.u8()?;
            r.u8()?;
            r.u8()?;
            let slots = r.u32()?;
            let nrefs = match kind {
                TYPE_RECORD | TYPE_EXCEPTION => slots as usize,
                TYPE_ARRAY | TYPE_DYNARRAY => 1,
                TYPE_ENUM | TYPE_SET => 0,
                k => return Err(LoadError::Invalid(format!("unknown type kind {k}"))),
            };
            r.need(nrefs)?;
            let mut refs = Vec::with_capacity(nrefs);
            for _ in 0..nrefs {
                refs.push(match r.u8()? {
                    0 => false,
                    1 => true,
                    k => return Err(LoadError::Invalid(format!("unknown slot kind {k}"))),
                });
            }
            r.pad4()?;
            let name = r.lp_utf8()?;
            let parent = if kind == TYPE_EXCEPTION {
                Some(r.u32()?)
            } else {
                None
            };
            let mut values = Vec::new();
            if kind == TYPE_ENUM {
                if slots != 0 {
                    return Err(LoadError::Invalid("enumeration type with slots".into()));
                }
                let n = r.count("enumeration value")?;
                for _ in 0..n {
                    let ord = r.i64()?;
                    let name = r.lp_utf8()?;
                    values.push((ord, name));
                }
            }
            if kind == TYPE_ARRAY && slots > i32::MAX as u32 {
                return Err(LoadError::Invalid("static array is too large".into()));
            }
            if kind == TYPE_SET && !(1..=65536).contains(&slots) {
                return Err(LoadError::Invalid(
                    "set type with a wrong number of bits".into(),
                ));
            }
            if kind == TYPE_DYNARRAY && slots != 0 {
                return Err(LoadError::Invalid("dynamic array type with slots".into()));
            }
            out.push(TypeEntry {
                kind,
                slots,
                refs,
                name,
                parent,
                values,
            });
        }
        Ok(out)
    }

    fn decode_funcs(buf: &[u8]) -> Result<Vec<Function>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("function")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.lp_utf8()?;
            let params = r.u16()?;
            let regs = r.u16()?;
            let nwords = r.count("code")?;
            let mut code = Vec::with_capacity(nwords);
            for _ in 0..nwords {
                code.push(r.u32()?);
            }
            let nhandlers = r.count("handler")?;
            let mut handlers = Vec::with_capacity(nhandlers);
            for _ in 0..nhandlers {
                let kind = r.u8()?;
                r.u8()?;
                r.u16()?;
                let body_start = r.u32()?;
                let body_end = r.u32()?;
                let handler_start = r.u32()?;
                let handler_end = r.u32()?;
                let reg = r.u16()?;
                r.u16()?;
                handlers.push(HandlerEntry {
                    kind,
                    body_start,
                    body_end,
                    handler_start,
                    handler_end,
                    reg,
                });
            }
            let nmanaged = r.count("managed register")?;
            let mut managed = Vec::with_capacity(nmanaged);
            for _ in 0..nmanaged {
                let reg = r.u16()?;
                let kind = r.u16()?;
                if kind != MANAGED_HEAP_REF {
                    return Err(LoadError::Invalid(format!(
                        "unknown managed register kind {kind}"
                    )));
                }
                managed.push(reg);
            }
            let nlines = r.count("line table")?;
            let mut lines = Vec::with_capacity(nlines);
            for _ in 0..nlines {
                lines.push((r.u32()?, r.u32()?));
            }
            let nlocals = r.count("local")?;
            let mut locals = Vec::with_capacity(nlocals);
            for _ in 0..nlocals {
                let reg = r.u16()?;
                r.u16()?;
                locals.push((reg, r.lp_utf8()?));
            }
            let file = r.u32()?;
            let outer = Some(r.u32()?).filter(|&o| o != NO_OUTER);
            out.push(Function {
                outer,
                name,
                params,
                regs,
                code,
                lines,
                locals,
                managed,
                file,
                handlers,
            });
        }
        Ok(out)
    }

    fn decode_globals(buf: &[u8]) -> Result<Vec<GlobalEntry>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("global")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.lp_utf8()?;
            let kind = r.u32()?;
            if kind != GLOBAL_INT && kind != GLOBAL_REF && kind != GLOBAL_EXIT_CODE {
                return Err(LoadError::Invalid(format!("unknown global kind {kind}")));
            }
            if kind == GLOBAL_EXIT_CODE && out.iter().any(|g: &GlobalEntry| g.kind == kind) {
                return Err(LoadError::Invalid("more than one ExitCode global".into()));
            }
            out.push(GlobalEntry { name, kind });
        }
        Ok(out)
    }

    fn decode_units(buf: &[u8]) -> Result<(u32, Vec<UnitEntry>), LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let entry = r.u32()?;
        let n = r.count("unit")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.lp_utf8()?;
            let path = r.lp_utf8()?;
            let init = r.u32()?;
            let fini = r.u32()?;
            let first_global = r.u32()?;
            let global_count = r.u32()?;
            out.push(UnitEntry {
                name,
                path,
                init: (init != NONE_INDEX).then_some(init),
                fini: (fini != NONE_INDEX).then_some(fini),
                first_global,
                global_count,
            });
        }
        Ok((entry, out))
    }

    fn decode_hosts(buf: &[u8]) -> Result<Vec<HostEntry>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("host function")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.lp_utf8()?;
            let params = r.u16()?;
            let ret = r.u16()?;
            if ret > 1 {
                return Err(LoadError::Invalid("bad host function return flag".into()));
            }
            out.push(HostEntry {
                name,
                params,
                ret: ret == 1,
            });
        }
        Ok(out)
    }

    // ---- Verification (3.3) ----

    pub fn verify(&self) -> Result<(), LoadError> {
        if self.units.is_empty() {
            return Err(LoadError::Invalid("no unit".into()));
        }
        let check_fn = |idx: u32, what: &str| -> Result<(), LoadError> {
            let f = self
                .funcs
                .get(idx as usize)
                .ok_or_else(|| LoadError::Invalid(format!("{what} index out of range")))?;
            if f.params != 0 {
                return Err(LoadError::Invalid(format!(
                    "{what} function takes parameters"
                )));
            }
            Ok(())
        };
        check_fn(self.entry, "entry")?;
        // The units' global variables follow one another and cover them all (`units.md` 6.3)
        let mut next_global: u64 = 0;
        for u in &self.units {
            if let Some(i) = u.init {
                check_fn(i, "initialization")?;
            }
            if let Some(i) = u.fini {
                check_fn(i, "finalization")?;
            }
            if u64::from(u.first_global) != next_global {
                return Err(LoadError::Invalid(format!(
                    "global variables of unit `{}` do not follow the previous unit",
                    u.name
                )));
            }
            next_global += u64::from(u.global_count);
        }
        if next_global != self.globals.len() as u64 {
            return Err(LoadError::Invalid(
                "the units do not cover the global variables".into(),
            ));
        }
        for (k, c) in self.consts.iter().enumerate() {
            if let ConstEntry::Set { ty, words } = c {
                let t = self
                    .type_entry(*ty)
                    .filter(|t| t.kind == TYPE_SET)
                    .ok_or_else(|| {
                        LoadError::Invalid(format!("constant {k}: type {ty} is not a set"))
                    })?;
                let bits = t.slots as usize;
                let beyond = words
                    .iter()
                    .enumerate()
                    .any(|(i, w)| (0..64).any(|b| w & (1 << b) != 0 && i * 64 + b >= bits));
                if beyond || words.last() == Some(&0) {
                    return Err(LoadError::Invalid(format!(
                        "constant {k}: set words do not fit the type"
                    )));
                }
            }
            if let ConstEntry::Agg { ty, items } = c {
                let t = self
                    .type_entry(*ty)
                    .filter(|t| matches!(t.kind, TYPE_RECORD | TYPE_ARRAY))
                    .ok_or_else(|| {
                        LoadError::Invalid(format!(
                            "constant {k}: type {ty} is not a record or static array"
                        ))
                    })?;
                if items.len() != t.slots as usize {
                    return Err(LoadError::Invalid(format!(
                        "constant {k}: {} elements for a type of {} slots",
                        items.len(),
                        t.slots
                    )));
                }
                // Elements come first, so nested constants cannot form a cycle
                if items.iter().any(|i| *i as usize >= k) {
                    return Err(LoadError::Invalid(format!(
                        "constant {k}: an element does not come before it"
                    )));
                }
            }
        }
        for (i, t) in self.types.iter().enumerate() {
            let index = FIRST_TYPE_INDEX + i as u32;
            if t.kind != TYPE_EXCEPTION {
                continue;
            }
            // The parent comes first, so following parents always reaches the root
            let parent = t.parent.unwrap_or(index);
            let ok = parent < index
                && self
                    .type_entry(parent)
                    .is_some_and(|p| p.kind == TYPE_EXCEPTION);
            if !ok {
                return Err(LoadError::Invalid(format!(
                    "type {index}: the parent is not an earlier exception class"
                )));
            }
            let base = self.type_entry(parent).map_or(0, |p| p.slots);
            if t.slots < base {
                return Err(LoadError::Invalid(format!(
                    "type {index}: fewer slots than the parent"
                )));
            }
        }
        for (fi, f) in self.funcs.iter().enumerate() {
            self.verify_function(fi, f)?;
        }
        Ok(())
    }

    /// The type entry of a type index: a built-in type below 64, the type section from 64 (3.2).
    pub fn type_entry(&self, index: u32) -> Option<&TypeEntry> {
        match index.checked_sub(FIRST_TYPE_INDEX) {
            Some(i) => self.types.get(i as usize),
            None => builtin_types().get(index as usize).filter(|t| t.kind != 0),
        }
    }

    /// The handler table (`docs/spec/exceptions.md` 6.3). `end_finally` lists the `EndFinally`
    /// instructions as (index into `boundaries`, `A`, `B`).
    fn verify_handlers(
        &self,
        f: &Function,
        boundaries: &[u32],
        end_finally: &[(usize, u32, u32)],
    ) -> Result<(), String> {
        let len = f.code.len() as u32;
        let at_boundary = |p: u32| boundaries.binary_search(&p).is_ok();
        for (i, h) in f.handlers.iter().enumerate() {
            let what = format!("handler {i}");
            if h.kind != HANDLER_EXCEPT && h.kind != HANDLER_FINALLY {
                return Err(format!("{what} has an unknown kind {}", h.kind));
            }
            let positions = [h.body_start, h.body_end, h.handler_start];
            if positions.iter().any(|&p| !at_boundary(p))
                || !(at_boundary(h.handler_end)
                    || (h.kind == HANDLER_EXCEPT && h.handler_end == len))
            {
                return Err(format!("{what} has a position off an instruction boundary"));
            }
            if !(h.body_start < h.body_end
                && h.body_end <= h.handler_start
                && h.handler_start < h.handler_end)
            {
                return Err(format!("{what} has its ranges out of order"));
            }
            let regs = f.regs as u32;
            let top = h.reg as u32 + u32::from(h.kind == HANDLER_FINALLY);
            if top >= regs {
                return Err(format!("{what} register out of range"));
            }
            if h.kind == HANDLER_FINALLY {
                let ends_here = end_finally
                    .iter()
                    .any(|&(b, a, _)| boundaries[b] == h.handler_end && a == h.reg as u32);
                if !ends_here {
                    return Err(format!(
                        "{what} does not end at an `EndFinally` of its register"
                    ));
                }
            }
            // Spans nest or are disjoint; inner entries come first
            let span = (h.body_start, h.handler_end);
            for (j, g) in f.handlers.iter().enumerate().take(i) {
                let other = (g.body_start, g.handler_end);
                let disjoint = other.1 <= span.0 || span.1 <= other.0;
                let inside = span.0 <= other.0 && other.1 <= span.1;
                if !disjoint && !inside {
                    return Err(format!(
                        "{what} overlaps handler {j} (or comes before an entry inside it)"
                    ));
                }
            }
        }
        Ok(())
    }

    fn verify_function(&self, fi: usize, f: &Function) -> Result<(), LoadError> {
        let err = |m: String| LoadError::Invalid(format!("function {fi} ({}): {m}", f.name));
        if (f.params as usize) > f.regs as usize && !(f.params == 0 && f.regs == 0) {
            return Err(err("params exceed regs".into()));
        }
        if f.file as usize >= self.units.len() {
            return Err(err("file index out of range".into()));
        }
        // A nested routine takes the static link as its first argument (`docs/spec/semantics.md` 4.9)
        if let Some(o) = f.outer
            && (o as usize >= self.funcs.len() || f.params == 0)
        {
            return Err(err("bad enclosing function".into()));
        }
        // Collect instruction boundaries
        let mut boundaries: Vec<u32> = Vec::new();
        let mut pc = 0usize;
        let code = &f.code;
        let mut last: Option<u8> = None;
        // `EndFinally`: (index into `boundaries`, `A`, number of `Jmp`s)
        let mut end_finally: Vec<(usize, u32, u32)> = Vec::new();
        while pc < code.len() {
            boundaries.push(pc as u32);
            let (op0, _, _, _) = decode(code[pc]);
            let (wide, start) = if op0 == op::WIDE {
                if pc + 2 >= code.len() {
                    return Err(err("`Wide` without a following instruction".into()));
                }
                (true, pc + 1)
            } else {
                (false, pc)
            };
            let (opc, a8, b8, c8) = decode(code[start]);
            if opc == op::WIDE && wide {
                return Err(err("`Wide` after `Wide`".into()));
            }
            let info =
                op::info(opc).ok_or_else(|| err(format!("unknown opcode {opc} at {pc:04x}")))?;
            let (a, b, c) = if wide {
                let (_, ea, eb, ec) = decode(code[start + 1]);
                (a8 | (ea << 8), b8 | (eb << 8), c8 | (ec << 8))
            } else {
                (a8, b8, c8)
            };
            let bx = decode_bx(code[start], wide.then(|| code[start + 1]));
            let regs = f.regs as u32;
            let reg = |r: u32, what: &str| -> Result<(), LoadError> {
                if r >= regs {
                    Err(err(format!("register {what}={r} out of range at {pc:04x}")))
                } else {
                    Ok(())
                }
            };
            use op::Shape::*;
            match info.shape {
                None => {}
                A => reg(a, "A")?,
                AB => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                }
                ABC => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(c, "C")?;
                }
                ABCImm => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    // `StrToFloatIO` takes the kind of real (2.17), the others a width (2.2)
                    let max = if opc == op::STR_TO_FLOAT_IO {
                        FLOAT_IO_CURRENCY as u32
                    } else {
                        8
                    };
                    if c > max {
                        return Err(err(format!("width {c} out of range at {pc:04x}")));
                    }
                }
                AOuter => {
                    reg(a, "A")?;
                    // The frame `C` levels out is that of a function known here (each level
                    // passes through a static link), and `B` is one of its registers
                    let mut g = fi;
                    for _ in 0..c {
                        g = match self.funcs[g].outer {
                            Some(o) => o as usize,
                            Option::None => {
                                return Err(err(format!(
                                    "`OuterRef` goes out of a function that is not nested at {pc:04x}"
                                )));
                            }
                        };
                    }
                    if b >= self.funcs[g].regs as u32 {
                        return Err(err(format!(
                            "register B={b} out of range of the enclosing function at {pc:04x}"
                        )));
                    }
                }
                ABRange2 => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 1, "B+1")?;
                }
                ABRange3 => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 2, "B+2")?;
                }
                ABRange4 => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 3, "B+3")?;
                }
                ABPairs => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 2 * c, "B+2C")?;
                }
                ABsC | ABsCImm => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                }
                ASBxImm => reg(a, "A")?,
                ABx => {
                    reg(a, "A")?;
                    match opc {
                        op::LOAD_CONST => {
                            if bx as usize >= self.consts.len() {
                                return Err(err(format!("constant {bx} out of range at {pc:04x}")));
                            }
                        }
                        op::LOAD_GLOBAL
                        | op::STORE_GLOBAL
                        | op::MAKE_GLOBAL_REF
                        | op::LOAD_GLOBAL_REF
                        | op::STORE_GLOBAL_REF => {
                            if bx as usize >= self.globals.len() {
                                return Err(err(format!("global {bx} out of range at {pc:04x}")));
                            }
                        }
                        op::CALL => {
                            let callee = self.funcs.get(bx as usize).ok_or_else(|| {
                                err(format!("function {bx} out of range at {pc:04x}"))
                            })?;
                            if a + callee.params as u32 > regs {
                                return Err(err(format!("call arguments exceed regs at {pc:04x}")));
                            }
                        }
                        op::CALL_HOST => {
                            let h = self.hosts.get(bx as usize).ok_or_else(|| {
                                err(format!("host function {bx} out of range at {pc:04x}"))
                            })?;
                            if a + h.params as u32 > regs {
                                return Err(err(format!(
                                    "host call arguments exceed regs at {pc:04x}"
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                ASBx => reg(a, "A")?,
                SBx => {}
                ABSlot => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    if opc == op::EXC_INFO && c > EXC_INFO_CLASS_NAME {
                        return Err(err(format!("information {c} out of range at {pc:04x}")));
                    }
                }
                ASlotC => {
                    reg(a, "A")?;
                    reg(c, "C")?;
                }
                ABk => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    // `ChkSet` / `ChkSetBig` have a type name and range in a string like `ChkRange`
                    if !matches!(self.consts.get(c as usize), Some(ConstEntry::Str(_))) {
                        return Err(err(format!("constant {c} is not a string at {pc:04x}")));
                    }
                }
                ABType => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    match self.type_entry(c) {
                        Some(t) if t.kind == TYPE_ENUM => {}
                        Some(_) => {
                            return Err(err(format!("type {c} has the wrong kind at {pc:04x}")));
                        }
                        Option::None => {
                            return Err(err(format!("type {c} out of range at {pc:04x}")));
                        }
                    }
                }
                ABPath => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    if c == 0 {
                        return Err(err(format!("empty cell path at {pc:04x}")));
                    }
                    reg(a + c, "A+C")?;
                }
                AJumps => {
                    reg(a, "A")?;
                    reg(a + 1, "A+1")?;
                    end_finally.push((boundaries.len() - 1, a, b));
                }
            }
            // Type operands (2.8, 2.9): the kinds each instruction takes
            let kinds: &[u8] = match opc {
                op::UNIQUE => &[TYPE_RECORD, TYPE_ARRAY, TYPE_DYNARRAY, TYPE_SET],
                op::MATERIALIZE => &[TYPE_RECORD, TYPE_ARRAY],
                op::SET_LEN
                | op::NEW_ARR
                | op::ARR_COPY
                | op::ARR_CONCAT
                | op::ARR_INSERT
                | op::ARR_INSERT_ARR
                | op::ARR_DELETE => &[TYPE_DYNARRAY],
                op::NEW_EXC | op::CLASS_IS => &[TYPE_EXCEPTION],
                _ => &[],
            };
            if !kinds.is_empty() {
                match self.type_entry(bx) {
                    Some(t) if kinds.contains(&t.kind) => {}
                    Some(_) => {
                        return Err(err(format!("type {bx} has the wrong kind at {pc:04x}")));
                    }
                    Option::None => {
                        return Err(err(format!("type {bx} out of range at {pc:04x}")));
                    }
                }
            }
            match opc {
                op::SET_LEN | op::ARR_CONCAT => reg(a + 1, "A+1")?,
                op::ARR_COPY | op::ARR_INSERT | op::ARR_INSERT_ARR | op::ARR_DELETE => {
                    reg(a + 2, "A+2")?
                }
                _ => {}
            }
            last = Some(opc);
            pc = if wide { start + 2 } else { start + 1 };
        }
        if pc != code.len() {
            return Err(err("instruction stream is truncated".into()));
        }
        if !matches!(
            last,
            Some(op::RET | op::RET_VOID | op::HALT | op::JMP | op::RAISE)
        ) {
            return Err(err(
                "last instruction must be Ret, RetVoid, Halt, Jmp or Raise".into(),
            ));
        }
        // `EndFinally` is followed by exactly `B` `Jmp`s and then another instruction (state 0 goes on
        // after the `Jmp`s)
        let opcode_at = |i: usize| -> Option<u8> {
            let start = *boundaries.get(i)? as usize;
            let (opc0, _, _, _) = decode(code[start]);
            let body = if opc0 == op::WIDE { start + 1 } else { start };
            Some(decode(code[body]).0)
        };
        for &(i, _, n) in &end_finally {
            for k in 1..=n as usize {
                if opcode_at(i + k) != Some(op::JMP) {
                    return Err(err(format!(
                        "`EndFinally` at {:04x} is not followed by {n} `Jmp`s",
                        boundaries[i]
                    )));
                }
            }
            if i + n as usize + 1 >= boundaries.len() {
                return Err(err(format!(
                    "`EndFinally` at {:04x} ends the code",
                    boundaries[i]
                )));
            }
        }
        self.verify_handlers(f, &boundaries, &end_finally)
            .map_err(err)?;
        // Jump targets
        for &start in &boundaries {
            let (opc0, _, _, _) = decode(code[start as usize]);
            let wide = opc0 == op::WIDE;
            let body = if wide { start + 1 } else { start };
            let (opc, _, b8, c8) = decode(code[body as usize]);
            let Some(info) = op::info(opc) else { continue };
            let next = if wide { start + 3 } else { start + 1 };
            let offset: i64 = match info.shape {
                op::Shape::SBx | op::Shape::ASBx => {
                    if wide {
                        let (_, _, eb, ec) = decode(code[body as usize + 1]);
                        let v = b8 | (c8 << 8) | (eb << 16) | (ec << 24);
                        v as i32 as i64
                    } else {
                        (b8 | (c8 << 8)) as u16 as i16 as i64
                    }
                }
                op::Shape::ABsC => {
                    if wide {
                        let (_, _, _, ec) = decode(code[body as usize + 1]);
                        ((c8 | (ec << 8)) as u16 as i16) as i64
                    } else {
                        (c8 as u8 as i8) as i64
                    }
                }
                _ => continue,
            };
            let target = next as i64 + offset;
            if target < 0
                || target as usize >= code.len()
                || boundaries.binary_search(&(target as u32)).is_err()
            {
                return Err(err(format!(
                    "jump target {target} is not an instruction boundary at {start:04x}"
                )));
            }
        }
        // Line table
        if f.lines.is_empty() || f.lines[0].0 != 0 {
            return Err(err("line table must start at 0".into()));
        }
        let mut prev = 0u32;
        for (i, (p, _)) in f.lines.iter().enumerate() {
            if (i > 0 && *p <= prev) || *p as usize >= code.len().max(1) {
                return Err(err(
                    "line table positions must be increasing and within the code".into(),
                ));
            }
            prev = *p;
        }
        for (r, _) in &f.locals {
            if *r as u32 >= f.regs as u32 {
                return Err(err("local name register out of range".into()));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for r in &f.managed {
            if *r as u32 >= f.regs as u32 {
                return Err(err("managed register out of range".into()));
            }
            if !seen.insert(*r) {
                return Err(err(format!("managed register r{r} is listed twice")));
            }
        }
        Ok(())
    }
}

// ---- Disassembly (chapter 5) ----

/// The elements of a set's bit words as ranges in ascending order (`0..9, 65`), for the dumps.
pub fn set_ranges_text(words: &[u64]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    let bits = words.len() * 64;
    for i in 0..=bits {
        let on = i < bits && words[i / 64] & (1 << (i % 64)) != 0;
        match (on, run) {
            (true, None) => run = Some((i, i)),
            (true, Some((lo, _))) => run = Some((lo, i)),
            (false, Some((lo, hi))) => {
                parts.push(if lo == hi {
                    lo.to_string()
                } else {
                    format!("{lo}..{hi}")
                });
                run = None;
            }
            (false, None) => {}
        }
    }
    parts.join(", ")
}

/// The `bytecode` dump.
pub fn disassemble(m: &Module) -> String {
    let mut out = String::new();
    out.push_str(&format!("; puyc format {FORMAT_VERSION}\n"));
    out.push_str(".const\n");
    for (i, c) in m.consts.iter().enumerate() {
        match c {
            ConstEntry::Int(v) => out.push_str(&format!("  {i}: int {v}\n")),
            ConstEntry::Float(b) => out.push_str(&format!(
                "  {i}: float {}\n",
                crate::numfmt::f64_to_str(f64::from_bits(*b))
            )),
            ConstEntry::Str(s) => out.push_str(&format!("  {i}: str {}\n", debug_str(s))),
            ConstEntry::Utf16(u) => out.push_str(&format!("  {i}: str16 {}\n", debug_utf16(u))),
            ConstEntry::U8(s) => out.push_str(&format!("  {i}: utf8 {}\n", debug_str(s))),
            ConstEntry::Agg { ty, items } => {
                let ks: Vec<String> = items.iter().map(|k| format!("k{k}")).collect();
                out.push_str(&format!("  {i}: agg ty{ty} ({})\n", ks.join(", ")));
            }
            ConstEntry::Set { ty, words } => {
                out.push_str(&format!("  {i}: set ty{ty} [{}]\n", set_ranges_text(words)));
            }
        }
    }
    if !m.types.is_empty() {
        out.push_str(".type\n");
        for (i, t) in m.types.iter().enumerate() {
            let index = FIRST_TYPE_INDEX as usize + i;
            let kind = match t.kind {
                TYPE_RECORD => "record",
                TYPE_DYNARRAY => "dynarray",
                TYPE_EXCEPTION => "exception",
                TYPE_ENUM => "enum",
                TYPE_SET => "set",
                _ => "array",
            };
            let mut line = format!("  {index}: {kind} {} slots={}", t.name, t.slots);
            match t.kind {
                TYPE_RECORD | TYPE_EXCEPTION => {
                    let refs: Vec<String> = t
                        .refs
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| **r)
                        .map(|(i, _)| i.to_string())
                        .collect();
                    if !refs.is_empty() {
                        line.push_str(&format!(" refs={}", refs.join(",")));
                    }
                    if let Some(p) = t.parent {
                        line.push_str(&format!(" parent=ty{p}"));
                    }
                }
                TYPE_ENUM => {
                    let values: Vec<String> =
                        t.values.iter().map(|(o, n)| format!("{o}:{n}")).collect();
                    line.push_str(&format!(" values={}", values.join(",")));
                }
                TYPE_SET => {}
                _ => line.push_str(if t.slot_is_ref(0) {
                    " elem=ref"
                } else {
                    " elem=val"
                }),
            }
            out.push_str(&line);
            out.push('\n');
        }
    }
    out.push_str(".global\n");
    for (i, g) in m.globals.iter().enumerate() {
        let kind = match g.kind {
            GLOBAL_INT => "int",
            GLOBAL_EXIT_CODE => "exitcode",
            _ => "ref",
        };
        out.push_str(&format!("  {i}: {kind} {}\n", g.name));
    }
    out.push_str(".host\n");
    for (i, h) in m.hosts.iter().enumerate() {
        out.push_str(&format!(
            "  {i}: {} params={} ret={}\n",
            h.name, h.params, h.ret as u8
        ));
    }
    for (i, u) in m.units.iter().enumerate() {
        let idx = |v: Option<u32>| v.map(|x| x.to_string()).unwrap_or_else(|| "-".into());
        // Only the program row (the last) has an entry (`docs/spec/units.md` 6.3)
        let entry = if i + 1 == m.units.len() {
            m.entry.to_string()
        } else {
            "-".into()
        };
        out.push_str(&format!(
            ".unit {} file={} init={} fini={} entry={}\n",
            u.name,
            crate::lexer::debug_text(u.path.as_bytes()),
            idx(u.init),
            idx(u.fini),
            entry
        ));
    }
    for (i, f) in m.funcs.iter().enumerate() {
        out.push_str(&format!(
            ".func {i} {} params={} regs={}",
            f.name, f.params, f.regs
        ));
        if let Some(o) = f.outer {
            out.push_str(&format!(" outer={o}"));
        }
        out.push('\n');
        let mut locals = f.locals.clone();
        locals.sort_by_key(|l| l.0);
        for (r, n) in &locals {
            out.push_str(&format!("  .local r{r} {n}\n"));
        }
        for h in &f.handlers {
            let kind = if h.kind == HANDLER_EXCEPT {
                "except"
            } else {
                "finally"
            };
            out.push_str(&format!(
                "  .try {:04x}..{:04x} {kind} {:04x}..{:04x} r{}\n",
                h.body_start, h.body_end, h.handler_start, h.handler_end, h.reg
            ));
        }
        let mut li = 0usize;
        let mut pc = 0usize;
        while pc < f.code.len() {
            while li < f.lines.len() && f.lines[li].0 as usize <= pc {
                out.push_str(&format!("  .line {}\n", f.lines[li].1));
                li += 1;
            }
            let (text, len) = disassemble_one(&f.code, pc);
            out.push_str(&format!("  {pc:04x}  {text}\n"));
            pc += len;
        }
        if !f.managed.is_empty() {
            let regs: Vec<String> = f.managed.iter().map(|r| format!("r{r}")).collect();
            out.push_str(&format!(".finally {}\n", regs.join(", ")));
        }
    }
    out
}

/// A `str` constant as the disassembly shows it (5).
fn debug_str(s: &str) -> String {
    crate::lexer::debug_text(s.as_bytes())
}

/// A string in UTF-16 code units as the disassembly shows it: `str` if it is valid UTF-16, otherwise `str16` (5).
pub(crate) fn debug_units(u: &[u16]) -> String {
    match String::from_utf16(u) {
        Ok(s) => debug_str(&s),
        Err(_) => debug_utf16(u),
    }
}

fn debug_utf16(u: &[u16]) -> String {
    let mut s = String::from("\"");
    for r in char::decode_utf16(u.iter().copied()) {
        match r {
            Ok(c) => {
                let d = format!("{:?}", c.to_string());
                s.push_str(&d[1..d.len() - 1]);
            }
            Err(e) => s.push_str(&format!("\\u{{{:x}}}", e.unpaired_surrogate())),
        }
    }
    s.push('"');
    s
}

/// A jump target position (4 hex digits; a negative value that fails verification is shown in decimal).
fn fmt_target(t: i64) -> String {
    if t < 0 {
        t.to_string()
    } else {
        format!("{t:04x}")
    }
}

/// Disassembles one instruction: (text, number of words).
pub fn disassemble_one(code: &[u32], pc: usize) -> (String, usize) {
    let (op0, _, _, _) = decode(code[pc]);
    let wide = op0 == op::WIDE;
    let body = if wide { pc + 1 } else { pc };
    let (opc, a8, b8, c8) = decode(code[body]);
    let (a, b, c) = if wide {
        let (_, ea, eb, ec) = decode(code[body + 1]);
        (a8 | (ea << 8), b8 | (eb << 8), c8 | (ec << 8))
    } else {
        (a8, b8, c8)
    };
    let len = if wide { 3 } else { 1 };
    let next = pc + len;
    let Some(info) = op::info(opc) else {
        return (format!("<unknown {opc}>"), len);
    };
    let name = if wide {
        format!("w.{}", info.name)
    } else {
        info.name.to_string()
    };
    let bx = decode_bx(code[body], wide.then(|| code[body + 1]));
    let sbx = if wide {
        bx as i32 as i64
    } else {
        (b8 | (c8 << 8)) as u16 as i16 as i64
    };
    let sc = if wide {
        ((c8 | ((c >> 8) << 8)) as u16 as i16) as i64
    } else {
        (c8 as u8 as i8) as i64
    };
    use op::Shape::*;
    let operands = match info.shape {
        None => String::new(),
        A => format!("r{a}"),
        AB | ABRange2 | ABRange3 | ABRange4 => format!("r{a}, r{b}"),
        ABPairs => format!("r{a}, r{b}, {c}"),
        ABC => format!("r{a}, r{b}, r{c}"),
        ABCImm => format!("r{a}, r{b}, {c}"),
        ASBxImm => format!("r{a}, {sbx}"),
        ABx => {
            let prefix = match opc {
                op::LOAD_CONST => "k",
                op::LOAD_GLOBAL
                | op::STORE_GLOBAL
                | op::MAKE_GLOBAL_REF
                | op::LOAD_GLOBAL_REF
                | op::STORE_GLOBAL_REF => "g",
                op::CALL => "f",
                op::CALL_HOST => "h",
                op::UNIQUE
                | op::SET_LEN
                | op::NEW_ARR
                | op::ARR_COPY
                | op::ARR_CONCAT
                | op::ARR_INSERT
                | op::ARR_INSERT_ARR
                | op::ARR_DELETE
                | op::MATERIALIZE
                | op::NEW_EXC
                | op::CLASS_IS => "ty",
                _ => "",
            };
            format!("r{a}, {prefix}{bx}")
        }
        ABSlot | ABPath => format!("r{a}, r{b}, {c}"),
        ASlotC => format!("r{a}, {b}, r{c}"),
        ABk => format!("r{a}, r{b}, k{c}"),
        ABType => format!("r{a}, r{b}, ty{c}"),
        ASBx => format!("r{a}, -> {}", fmt_target(next as i64 + sbx)),
        SBx => format!("-> {}", fmt_target(next as i64 + sbx)),
        ABsC => format!("r{a}, r{b}, -> {}", fmt_target(next as i64 + sc)),
        ABsCImm => format!("r{a}, r{b}, {sc}"),
        AJumps => format!("r{a}, {b}"),
        AOuter => format!("r{a}, {b}, {c}"),
    };
    if operands.is_empty() {
        (name, len)
    } else {
        (format!("{name:<12} {operands}"), len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Module {
        Module {
            types: Vec::new(),
            consts: vec![
                ConstEntry::Str("Hello, world\n".into()),
                ConstEntry::Int(42),
                ConstEntry::Utf16(vec![0xD800]),
            ],
            funcs: vec![Function {
                name: "Hello.<main>".into(),
                params: 0,
                regs: 3,
                code: vec![
                    encode(op::LOAD_INT, 1, 0, 0),
                    encode(op::LOAD_CONST, 0, 0, 0),
                    encode(op::CALL_HOST, 0, 0, 0),
                    encode(op::RET_VOID, 0, 0, 0),
                ],
                lines: vec![(0, 5), (2, 6)],
                locals: vec![(1, "I".into())],
                managed: vec![0],
                file: 0,
                handlers: Vec::new(),
                outer: None,
            }],
            globals: vec![GlobalEntry {
                name: "Total".into(),
                kind: 0,
            }],
            entry: 0,
            units: vec![UnitEntry {
                name: "Hello".into(),
                path: "Hello.pas".into(),
                init: None,
                fini: None,
                first_global: 0,
                global_count: 1,
            }],
            hosts: vec![HostEntry {
                name: "Console.Write".into(),
                params: 1,
                ret: false,
            }],
        }
    }

    #[test]
    fn roundtrip() {
        let m = sample();
        let bytes = m.encode("0.0.0");
        assert_eq!(&bytes[..4], b"PUYC");
        let back = Module::decode(&bytes, "0.0.0").unwrap();
        assert_eq!(back, m);
        assert!(matches!(
            Module::decode(&bytes, "0.0.1"),
            Err(LoadError::Version { .. })
        ));
        assert!(matches!(
            Module::decode(b"XXXX", "0.0.0"),
            Err(LoadError::Invalid(_))
        ));
    }

    #[test]
    fn disassembly_format() {
        let d = disassemble(&sample());
        let expected = "\
; puyc format 2
.const
  0: str \"Hello, world\\n\"
  1: int 42
  2: str16 \"\\u{d800}\"
.global
  0: int Total
.host
  0: Console.Write params=1 ret=0
.unit Hello file=\"Hello.pas\" init=- fini=- entry=0
.func 0 Hello.<main> params=0 regs=3
  .local r1 I
  .line 5
  0000  LoadInt      r1, 0
  0001  LoadConst    r0, k0
  .line 6
  0002  CallHost     r0, h0
  0003  RetVoid
.finally r0
";
        assert_eq!(d, expected);
    }

    /// A module with one record type (two slots, the second a reference) for the M2a checks.
    fn sample_with_type() -> Module {
        let mut m = sample();
        m.types.push(TypeEntry {
            kind: TYPE_RECORD,
            slots: 2,
            refs: vec![false, true],
            name: "Hello.TPoint".into(),
            parent: None,
            values: Vec::new(),
        });
        m
    }

    #[test]
    // Type section entries, aggregate constants and their round trip (3.2): the elements come
    // before the aggregate, and the entry has one constant per slot.
    fn type_section_and_aggregates_roundtrip() {
        let mut m = sample_with_type();
        m.consts.push(ConstEntry::Agg {
            ty: 64,
            items: vec![1, 0],
        });
        m.verify().expect("valid module");
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        assert!(d.contains("  3: agg ty64 (k1, k0)\n"), "{d}");
        assert!(
            d.contains(".type\n  64: record Hello.TPoint slots=2 refs=1\n.global\n"),
            "{d}"
        );
    }

    #[test]
    // The units' global variable ranges must follow one another in unit order and cover every
    // global variable exactly (`docs/spec/units.md` 6.3), since the finishing stage releases them
    // unit by unit.
    fn verify_checks_unit_global_ranges() {
        let unit = |name: &str, first_global, global_count| UnitEntry {
            name: name.into(),
            path: format!("{name}.pas"),
            init: None,
            fini: None,
            first_global,
            global_count,
        };
        let cases: [(&str, Vec<UnitEntry>, bool); 5] = [
            ("one unit covers all", vec![unit("Hello", 0, 1)], true),
            (
                "two units",
                vec![unit("A", 0, 0), unit("Hello", 0, 1)],
                true,
            ),
            ("gap", vec![unit("A", 0, 0), unit("Hello", 1, 0)], false),
            ("overlap", vec![unit("A", 0, 1), unit("Hello", 0, 1)], false),
            ("not covered", vec![unit("Hello", 0, 0)], false),
        ];
        for (what, units, ok) in cases {
            let mut m = sample();
            m.units = units;
            assert_eq!(m.verify().is_ok(), ok, "{what}");
        }
    }

    #[test]
    // Aggregate constants that would make `LoadConst` loop or read a slot that is not there, and
    // type operands outside the type section, are refused at load (3.3).
    fn verify_rejects_bad_aggregates_and_types() {
        type Change = fn(&mut Module);
        let cases: [(&str, Change); 5] = [
            ("element after the aggregate", |m| {
                m.consts.push(ConstEntry::Agg {
                    ty: 64,
                    items: vec![1, 3],
                })
            }),
            ("element count", |m| {
                m.consts.push(ConstEntry::Agg {
                    ty: 64,
                    items: vec![1],
                })
            }),
            ("built-in type index", |m| {
                m.consts.push(ConstEntry::Agg {
                    ty: 0,
                    items: vec![1, 0],
                })
            }),
            ("Unique of a type outside the section", |m| {
                m.funcs[0].code[0] = encode(op::UNIQUE, 1, 65, 0)
            }),
            ("ChkIndex with a constant that is not a string", |m| {
                m.funcs[0].code[0] = encode(op::CHK_INDEX, 1, 1, 1)
            }),
        ];
        for (what, change) in cases {
            let mut m = sample_with_type();
            change(&mut m);
            assert!(m.verify().is_err(), "{what}");
        }
        let mut m = sample_with_type();
        m.funcs[0].code[0] = encode(op::UNIQUE, 1, 64, 0);
        assert!(m.verify().is_ok());
    }

    #[test]
    // The array instructions take a type of the right kind (a dynamic array for `SetLen`,
    // `NewArr`, `ArrCopy`; a record or static array for `Materialize`), and `SetLen` / `ArrCopy`
    // read registers after `rA` (2.9, 3.3).
    fn verify_checks_array_instructions() {
        let with_dyn = || {
            let mut m = sample_with_type();
            m.types.push(TypeEntry {
                kind: TYPE_DYNARRAY,
                slots: 0,
                refs: vec![true],
                name: "array of string".into(),
                parent: None,
                values: Vec::new(),
            });
            m
        };
        let regs = with_dyn().funcs[0].regs as u32;
        let cases: [(&str, u32, bool); 8] = [
            (
                "SetLen of a dynamic array",
                encode(op::SET_LEN, 0, 65, 0),
                true,
            ),
            ("SetLen of a record", encode(op::SET_LEN, 0, 64, 0), false),
            (
                "SetLen reading past the registers",
                encode(op::SET_LEN, regs - 1, 65, 0),
                false,
            ),
            ("NewArr of a record", encode(op::NEW_ARR, 0, 64, 0), false),
            (
                "ArrCopy reading past the registers",
                encode(op::ARR_COPY, regs - 2, 65, 0),
                false,
            ),
            (
                "Materialize of a record",
                encode(op::MATERIALIZE, 0, 64, 0),
                true,
            ),
            (
                "Materialize of a dynamic array",
                encode(op::MATERIALIZE, 0, 65, 0),
                false,
            ),
            (
                "Unique of a dynamic array",
                encode(op::UNIQUE, 0, 65, 0),
                true,
            ),
        ];
        for (what, word, ok) in cases {
            let mut m = with_dyn();
            m.funcs[0].code[0] = word;
            assert_eq!(m.verify().is_ok(), ok, "{what}");
        }
        let m = with_dyn();
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        assert!(
            d.contains(
                "  65: dynarray array of string slots=0 elem=ref
"
            ),
            "{d}"
        );
    }

    /// `sample_with_type` plus an exception class (64 is the record) whose parent is `parent`.
    fn with_exception(parent: u32) -> Module {
        let mut m = sample_with_type();
        m.types.push(TypeEntry {
            kind: TYPE_EXCEPTION,
            slots: 2,
            refs: vec![true, true],
            name: "Hello.EParseError".into(),
            parent: Some(parent),
            values: Vec::new(),
        });
        m
    }

    #[test]
    // An exception class entry carries its parent after the name, round trips, and is shown with
    // `parent=tyN` (3.2, 5.2).
    fn exception_class_entry_roundtrip() {
        let m = with_exception(0);
        m.verify().expect("valid module");
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        assert!(
            d.contains("  65: exception Hello.EParseError slots=2 refs=0,1 parent=ty0\n"),
            "{d}"
        );
    }

    /// `sample_with_type` with an enumeration at type index 65 whose ordinal 5 has two names.
    fn with_enum() -> Module {
        let mut m = sample_with_type();
        m.types.push(TypeEntry {
            kind: TYPE_ENUM,
            slots: 0,
            refs: Vec::new(),
            name: "Hello.TCode".into(),
            parent: None,
            values: vec![(0, "Zero".into()), (5, "Five".into()), (5, "Again".into())],
        });
        m.funcs[0].code[0] = encode(op::ENUM_TO_STR, 0, 1, 65);
        m
    }

    #[test]
    // An enumeration entry carries its values after the name, round trips, and is shown with
    // `values=ord:name,...`; `EnumToStr` shows its type as `tyN` (3.2, 5.2).
    fn enum_entry_roundtrip() {
        let m = with_enum();
        m.verify().expect("valid module");
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        assert!(
            d.contains("  65: enum Hello.TCode slots=0 values=0:Zero,5:Five,5:Again\n"),
            "{d}"
        );
        assert!(d.contains("EnumToStr    r0, r1, ty65\n"), "{d}");
    }

    #[test]
    // The name of an ordinal is the first value declared with it, and an ordinal no value has
    // is written in decimal (`docs/spec/types.md` 8.1).
    fn enum_name_of_ordinal() {
        let m = with_enum();
        let t = m.type_entry(65).expect("the enumeration entry");
        for (ord, name) in [(0, "Zero"), (5, "Five"), (3, "3"), (-1, "-1")] {
            assert_eq!(t.enum_name(ord), name, "{ord}");
        }
    }

    #[test]
    // `EnumToStr` takes an enumeration type and two registers (6.3, 3.3).
    fn verify_checks_enum_to_str() {
        let cases: [(&str, u32); 3] = [
            ("a record type", encode(op::ENUM_TO_STR, 0, 1, 64)),
            (
                "a type outside the section",
                encode(op::ENUM_TO_STR, 0, 1, 66),
            ),
            (
                "a register outside the frame",
                encode(op::ENUM_TO_STR, 0, 9, 65),
            ),
        ];
        for (what, word) in cases {
            let mut m = with_enum();
            m.funcs[0].code[0] = word;
            assert!(m.verify().is_err(), "{what}");
        }
    }

    /// `sample_with_type` with a big set of 100 bits at type index 65 and a set constant of it.
    fn with_set(words: Vec<u64>) -> Module {
        let mut m = sample_with_type();
        m.types.push(TypeEntry {
            kind: TYPE_SET,
            slots: 100,
            refs: Vec::new(),
            name: "Hello.TSmallChars".into(),
            parent: None,
            values: Vec::new(),
        });
        m.consts.push(ConstEntry::Set { ty: 65, words });
        m
    }

    #[test]
    // A `UTF8String` constant (kind 7) keeps its text through encoding, stays distinct from a
    // `string` constant (kind 2) with the same text, and the dump marks it `utf8`.
    fn utf8_constant_roundtrip() {
        let mut m = sample();
        m.consts.push(ConstEntry::U8("aé日".into()));
        m.consts.push(ConstEntry::Str("aé日".into()));
        m.verify().expect("valid module");
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        for line in ["  3: utf8 \"aé日\"\n", "  4: str \"aé日\"\n"] {
            assert!(d.contains(line), "{line:?} in {d}");
        }
    }

    #[test]
    // A floating-point constant (kind 3) keeps its bits through encoding, including the sign of
    // a negative zero and a NaN, and the dump shows it in the default text of a `Double`.
    fn float_constant_roundtrip() {
        let mut m = sample();
        for v in [0.1f64, -0.0, f64::NAN, 1e20] {
            m.consts.push(ConstEntry::Float(v.to_bits()));
        }
        m.verify().expect("valid module");
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        for line in [
            "  3: float 0.1\n",
            "  4: float -0\n",
            "  5: float NAN\n",
            "  6: float 1E20\n",
        ] {
            assert!(d.contains(line), "{line:?} in {d}");
        }
    }

    #[test]
    // A big set type (kind 5, its number of bits) and a set constant (kind 6, its bit words)
    // round trip and are shown as `set … slots=100` and `set ty65 [ranges]` (3.2, 5.2).
    fn set_type_and_constant_roundtrip() {
        let m = with_set(vec![0b1110_0001, 1 << 35]);
        m.verify().expect("valid module");
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        assert!(d.contains("  3: set ty65 [0, 5..7, 99]\n"), "{d}");
        assert!(d.contains("  65: set Hello.TSmallChars slots=100\n"), "{d}");
    }

    #[test]
    // A set constant is checked against its type: a set type, no bit beyond the type's bits,
    // and no trailing zero word (3.3).
    fn verify_checks_set_constants() {
        let cases: [(&str, Vec<u64>, u32); 3] = [
            ("a bit beyond the type", vec![0, 1 << 36], 65),
            ("a trailing zero word", vec![1, 0], 65),
            ("a record type", vec![1], 64),
        ];
        for (what, words, ty) in cases {
            let mut m = with_set(vec![1]);
            m.consts[3] = ConstEntry::Set { ty, words };
            assert!(m.verify().is_err(), "{what}");
        }
    }

    #[test]
    // A big set type has between 1 and 65536 bits (3.2).
    fn decode_checks_set_bits() {
        for (bits, ok) in [(1, true), (65536, true), (0, false), (65537, false)] {
            let mut m = with_set(vec![1]);
            m.types[1].slots = bits;
            let bytes = m.encode("0.0.0");
            assert_eq!(Module::decode(&bytes, "0.0.0").is_ok(), ok, "{bits}");
        }
    }

    #[test]
    // The parent of an exception class is a built-in exception class or an earlier exception
    // class of the section (`docs/spec/exceptions.md` 6.1).
    fn verify_checks_exception_parents() {
        for (parent, ok) in [(0, true), (11, true), (12, false), (64, false), (65, false)] {
            assert_eq!(
                with_exception(parent).verify().is_ok(),
                ok,
                "parent {parent}"
            );
        }
    }

    #[test]
    // `NewExc` takes an exception class (built-in or declared), and `ExcInfo` reads one of the
    // three kinds of information (`docs/spec/exceptions.md` 6.2, 6.3).
    fn verify_checks_exception_instructions() {
        let cases: [(&str, u32, bool); 9] = [
            (
                "NewExc of a built-in class",
                encode(op::NEW_EXC, 0, 7, 0),
                true,
            ),
            (
                "NewExc of a declared class",
                encode(op::NEW_EXC, 0, 65, 0),
                true,
            ),
            ("NewExc of a record", encode(op::NEW_EXC, 0, 64, 0), false),
            (
                "NewExc of TArray<Exception>",
                encode(op::NEW_EXC, 0, 12, 0),
                false,
            ),
            (
                "NewExc of an unused built-in index",
                encode(op::NEW_EXC, 0, 20, 0),
                false,
            ),
            (
                "NewExc of EEncodingError",
                encode(op::NEW_EXC, 0, 16, 0),
                true,
            ),
            (
                "NewExc of an M2b class",
                encode(op::NEW_EXC, 0, 13, 0),
                true,
            ),
            ("ExcInfo ClassName", encode(op::EXC_INFO, 0, 1, 2), true),
            ("ExcInfo out of range", encode(op::EXC_INFO, 0, 1, 3), false),
        ];
        for (what, word, ok) in cases {
            let mut m = with_exception(0);
            m.funcs[0].code[0] = word;
            assert_eq!(m.verify().is_ok(), ok, "{what}");
        }
    }

    /// `sample` with a `try..finally` around its first instruction:
    /// `0000 Nop` (body), `0001 Nop` (the `finally` block), `0002 EndFinally r1, 1`, `0003 Jmp -> 0004`,
    /// `0004 RetVoid`.
    fn with_finally() -> Module {
        let mut m = sample();
        let f = &mut m.funcs[0];
        f.code = vec![
            encode(op::NOP, 0, 0, 0),
            encode(op::NOP, 0, 0, 0),
            encode(op::END_FINALLY, 1, 1, 0),
            encode(op::JMP, 0, 0, 0),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        f.lines = vec![(0, 1)];
        f.handlers = vec![HandlerEntry {
            kind: HANDLER_FINALLY,
            body_start: 0,
            body_end: 1,
            handler_start: 1,
            handler_end: 2,
            reg: 1,
        }];
        m
    }

    #[test]
    // The handler table survives encoding, and the disassembly shows it before the code
    // (`docs/spec/exceptions.md` 6.3, 5.2).
    fn handler_table_roundtrip() {
        let m = with_finally();
        assert!(m.verify().is_ok());
        let back = Module::decode(&m.encode("0.0.0"), "0.0.0").unwrap();
        assert_eq!(back, m);
        let d = disassemble(&m);
        assert!(
            d.contains("  .try 0000..0001 finally 0001..0002 r1\n"),
            "{d}"
        );
        assert!(d.contains("  0002  EndFinally   r1, 1\n"), "{d}");
    }

    #[test]
    // The verifier's rules for the handler table and `EndFinally` (`docs/spec/exceptions.md` 6.3).
    fn verify_checks_handler_table() {
        type Change = fn(&mut Module);
        let cases: [(&str, Change, bool); 9] = [
            ("valid", |_| {}, true),
            ("unknown kind", |m| m.funcs[0].handlers[0].kind = 3, false),
            (
                "an except block may end at the end of the code",
                |m| {
                    let h = &mut m.funcs[0].handlers[0];
                    h.kind = HANDLER_EXCEPT;
                    h.handler_end = 5;
                },
                true,
            ),
            (
                "finally not ending at its EndFinally",
                |m| m.funcs[0].handlers[0].handler_end = 3,
                false,
            ),
            (
                "EndFinally of another register",
                |m| m.funcs[0].handlers[0].reg = 0,
                false,
            ),
            (
                "the exception register past regs",
                |m| {
                    m.funcs[0].regs = 2;
                },
                false,
            ),
            (
                "ranges out of order",
                |m| m.funcs[0].handlers[0].body_end = 2,
                false,
            ),
            (
                "fewer Jmps than EndFinally names",
                |m| m.funcs[0].code[2] = encode(op::END_FINALLY, 1, 2, 0),
                false,
            ),
            (
                "crossing entries",
                |m| {
                    m.funcs[0].handlers.push(HandlerEntry {
                        kind: HANDLER_EXCEPT,
                        body_start: 1,
                        body_end: 3,
                        handler_start: 3,
                        handler_end: 4,
                        reg: 0,
                    })
                },
                false,
            ),
        ];
        for (what, change, ok) in cases {
            let mut m = with_finally();
            change(&mut m);
            assert_eq!(m.verify().is_ok(), ok, "{what}");
        }
    }

    #[test]
    // `EndFinally` and its `Jmp`s cannot end the code: state 0 goes on after them (6.3).
    fn verify_rejects_end_finally_at_the_end() {
        let mut m = with_finally();
        m.funcs[0].code[3] = encode(op::JMP, 0, 0xFC, 0xFF);
        m.funcs[0].code.pop();
        assert!(m.verify().is_err());
    }

    #[test]
    // `MakeCell A B C` reads its path from `A+1 ..= A+C`, which must be registers, and the path
    // is never empty (6.1).
    fn verify_checks_cell_path() {
        for (c, ok) in [(0, false), (1, true), (2, false)] {
            let mut m = sample();
            m.funcs[0].code[0] = encode(op::MAKE_CELL, 1, 0, c);
            assert_eq!(m.verify().is_ok(), ok, "path length {c}");
        }
    }

    /// `sample` with a second function nested in the first: it takes the static link and its
    /// code is `inst` followed by `RetVoid`.
    fn with_nested(inst: u32) -> Module {
        let mut m = sample();
        let mut f = m.funcs[0].clone();
        f.name = "Hello.<main>.Inner".into();
        f.params = 1;
        f.regs = 2;
        f.code = vec![inst, encode(op::RET_VOID, 0, 0, 0)];
        f.lines = vec![(0, 1)];
        f.locals = Vec::new();
        f.managed = Vec::new();
        f.outer = Some(0);
        m.funcs.push(f);
        m
    }

    #[test]
    // The enclosing function of a nested routine survives encoding (3.2).
    fn outer_function_roundtrip() {
        let m = with_nested(encode(op::OUTER_REF, 1, 2, 1));
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
    }

    #[test]
    // `OuterRef A B C` goes out `C` static links, so each function on the way takes one, and `B`
    // is a register of the function it reaches (`docs/spec/semantics.md` 4.9).
    fn verify_checks_outer_ref() {
        // `sample`'s function has 3 registers and no static link
        for (b, c, ok) in [
            (2, 1, true),
            (3, 1, false),
            (1, 0, true),
            (2, 0, false),
            (0, 2, false),
        ] {
            let m = with_nested(encode(op::OUTER_REF, 1, b, c));
            assert_eq!(m.verify().is_ok(), ok, "B={b} C={c}");
        }
        // A function without a static link cannot go out
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::OUTER_REF, 1, 0, 1);
        assert!(m.verify().is_err());
    }

    #[test]
    // A function with a static link takes it as an argument, and its enclosing function exists
    // (3.2, 3.3).
    fn verify_checks_enclosing_function() {
        assert!(with_nested(encode(op::NOP, 0, 0, 0)).verify().is_ok());
        let mut m = with_nested(encode(op::NOP, 0, 0, 0));
        m.funcs[1].params = 0;
        assert!(m.verify().is_err());
        let mut m = with_nested(encode(op::NOP, 0, 0, 0));
        m.funcs[1].outer = Some(2);
        assert!(m.verify().is_err());
    }

    #[test]
    fn verify_rejects() {
        let mut m = sample();
        m.funcs[0].code[3] = encode(op::NOP, 0, 0, 0);
        assert!(m.verify().is_err());
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::MOVE, 5, 0, 0);
        assert!(m.verify().is_err());
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::JMP, 0, 7, 0);
        assert!(m.verify().is_err());
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::JMP, 0, 1, 0);
        assert!(m.verify().is_ok());
    }

    // The managed register list must name registers of the function, each once (3.3).
    #[test]
    fn verify_rejects_bad_managed_registers() {
        for (idx, managed) in [vec![3], vec![0, 0], vec![1, 2, 1]].into_iter().enumerate() {
            let mut m = sample();
            m.funcs[0].managed = managed;
            assert!(m.verify().is_err(), "{idx}");
        }
        let mut m = sample();
        m.funcs[0].managed = vec![2, 0, 1];
        assert!(m.verify().is_ok());
    }

    // A managed register entry of an unknown kind is a load error (3.2).
    #[test]
    fn decode_rejects_unknown_managed_kind() {
        let bytes = sample().encode("0.0.0");
        // The sample's only entry is { r0, kind 1 }; find it right after the empty handler table
        let entry = [1u8, 0, 0, 0, 0, 0, 1, 0];
        let pos = bytes
            .windows(entry.len() + 4)
            .position(|w| w[..4] == [0, 0, 0, 0] && w[4..] == entry)
            .expect("the managed register list is encoded")
            + 4;
        let mut bad = bytes.clone();
        bad[pos + 6] = 2;
        assert_eq!(
            Module::decode(&bad, "0.0.0"),
            Err(LoadError::Invalid("unknown managed register kind 2".into()))
        );
    }

    #[test]
    fn wide_table_index() {
        let mut m = sample();
        m.consts.resize(65537, ConstEntry::Int(0));
        m.consts[65536] = ConstEntry::Int(42);
        m.funcs[0].code = vec![
            encode(op::WIDE, 0, 0, 0),
            encode(op::LOAD_CONST, 0, 0, 0),
            encode_ext(0, 256, 0),
            encode(op::RET, 0, 0, 0),
        ];
        m.funcs[0].lines = vec![(0, 1)];
        assert_eq!(
            disassemble_one(&m.funcs[0].code, 0).0,
            "w.LoadConst  r0, k65536"
        );
        assert_eq!(Module::decode(&m.encode("test"), "test").unwrap(), m);
        m.consts.truncate(65536);
        assert!(m.verify().is_err(), "verifies the whole 32-bit index");
    }

    #[test]
    fn wide_disassembly() {
        let code = vec![
            encode(op::WIDE, 0, 0, 0),
            encode(op::LOAD_CONST, 300, 0, 1),
            encode_ext(300, 0, 0),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        let (t, len) = disassemble_one(&code, 0);
        assert_eq!(len, 3);
        assert_eq!(t, "w.LoadConst  r300, k256");
        let code = vec![
            encode(op::JMP_IF_LT_INT, 1, 2, 0xFE),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        let (t, _) = disassemble_one(&code, 0);
        assert_eq!(t, "JmpIfLtInt   r1, r2, -> -1");
    }
}
