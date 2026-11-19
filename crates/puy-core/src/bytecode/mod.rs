//! Bytecode (`docs/spec/bytecode.md`): instruction format, reading, writing and verifying modules (`.puyc`), and disassembly.

pub mod op;

use std::fmt;

/// Format version (3.1). Fixed at 0 before 1.0: modules built by a different
/// toolchain are rejected by the exact toolchain version match instead.
pub const FORMAT_VERSION: u32 = 0;

/// Builds the word of the instruction `op`. An instruction numbered past 255 keeps its number less
/// 256 in `op` and goes after the prefix `Ext` (1.2; [`needs_ext`]).
pub fn encode(op: impl Into<u16>, a: u32, b: u32, c: u32) -> u32 {
    u32::from(op.into() & 0xFF) | ((a & 0xFF) << 8) | ((b & 0xFF) << 16) | ((c & 0xFF) << 24)
}

/// Whether the instruction `op` is written after the prefix `Ext` (its number is 256 to 511; 1.2).
pub fn needs_ext(op: u16) -> bool {
    op >= op::EXT_BASE
}

/// Extension word (upper 8 bits of `A` / `B` / `C`).
pub fn encode_ext(a: u32, b: u32, c: u32) -> u32 {
    (((a >> 8) & 0xFF) << 8) | (((b >> 8) & 0xFF) << 16) | (((c >> 8) & 0xFF) << 24)
}

/// Splits a word: `(op, A, B, C)` (8 bits each). `op` is the field of the word, not the
/// instruction's number (which [`decode_at`] gives, adding 256 after `Ext`; 1.2): compare it with
/// [`op::word`].
pub fn decode(word: u32) -> (u8, u32, u32, u32) {
    (word_op(word), word_a(word), word_b(word), word_c(word))
}

/// The `op` of a word (one field of [`decode`]; the fast path of the execution loop reads each
/// field only where an instruction uses it). After `Ext` it is the instruction's number less 256;
/// compare it with [`op::word`].
#[inline(always)]
pub fn word_op(word: u32) -> u8 {
    (word & 0xFF) as u8
}

/// `A` of a word (8 bits; see [`word_op`]).
#[inline(always)]
pub fn word_a(word: u32) -> u32 {
    (word >> 8) & 0xFF
}

/// `B` of a word (8 bits; see [`word_op`]).
#[inline(always)]
pub fn word_b(word: u32) -> u32 {
    (word >> 16) & 0xFF
}

/// `C` of a word (8 bits; see [`word_op`]).
#[inline(always)]
pub fn word_c(word: u32) -> u32 {
    (word >> 24) & 0xFF
}

/// `Bx` joins `B/C` of the main word as the lower 16 bits and `B/C` of the extension word as the upper 16 bits.
/// Do not overlap `B` and `C` widened to 16 bits by shifting one of them by 8 bits.
pub(crate) fn decode_bx(word: u32, extension: Option<u32>) -> u32 {
    (word >> 16) | extension.map_or(0, |ext| ext & 0xFFFF_0000)
}

/// The prefixes of the instruction at the boundary `pos` (1.2): whether it has `Wide`, and the
/// position of its main word (the word holding `op`, after `Wide` and `Ext`). The prefixes must be
/// followed by a word (the verifier keeps them so).
fn prefixes_at(code: &[u32], pos: usize) -> (bool, bool, usize) {
    let wide = word_op(code[pos]) == op::word::WIDE;
    let at = pos + usize::from(wide);
    let ext = word_op(code[at]) == op::word::EXT;
    (wide, ext, at + usize::from(ext))
}

/// What is wrong with the prefixes of the instruction at the boundary `pos` (1.2; checked before
/// [`decode_at`] reads it), if anything: they come in the order `Wide`, `Ext`, each at most once,
/// and are followed by the instruction's word and, after `Wide`, the extension word.
fn prefix_error(code: &[u32], pos: usize) -> Option<&'static str> {
    let mut at = pos;
    let wide = word_op(code[at]) == op::word::WIDE;
    if wide {
        at += 1;
        match code.get(at).map(|&w| word_op(w)) {
            Option::None => return Some("`Wide` without a following instruction"),
            Some(op::word::WIDE) => return Some("`Wide` after `Wide`"),
            Some(_) => {}
        }
    }
    if word_op(code[at]) == op::word::EXT {
        at += 1;
        match code.get(at).map(|&w| word_op(w)) {
            Option::None => return Some("`Ext` without a following instruction"),
            Some(op::word::WIDE) => return Some("`Wide` after `Ext`"),
            Some(op::word::EXT) => return Some("`Ext` after `Ext`"),
            Some(_) => {}
        }
    }
    if wide && at + 1 >= code.len() {
        return Some("`Wide` without a following instruction");
    }
    None
}

/// The instruction at the boundary `pos`: its number (`op`, plus 256 after `Ext`), its `A`, `B`,
/// `C` (widened by the extension word after a `Wide`; 1.2) and its length in words (1, 1 more for
/// `Ext`, 2 more for `Wide` and its extension word, and 1 more for the type word of an instruction
/// that has one: [`has_type_word`]). A `Wide` at `pos` must be followed by its instruction and the
/// extension word, an `Ext` by its instruction; the type word is not read.
pub(crate) fn decode_at(code: &[u32], pos: usize) -> (u16, u32, u32, u32, usize) {
    let (wide, ext, at) = prefixes_at(code, pos);
    let (op8, a8, b8, c8) = decode(code[at]);
    let opc = u16::from(op8) + if ext { op::EXT_BASE } else { 0 };
    let tail = usize::from(has_type_word(opc));
    if !wide {
        return (opc, a8, b8, c8, at - pos + 1 + tail);
    }
    let (_, ea, eb, ec) = decode(code[at + 1]);
    let len = at - pos + 2 + tail;
    (opc, a8 | (ea << 8), b8 | (eb << 8), c8 | (ec << 8), len)
}

/// Whether the instruction at the boundary `pos` has the `Wide` prefix (1.2).
pub(crate) fn is_wide_at(code: &[u32], pos: usize) -> bool {
    word_op(code[pos]) == op::word::WIDE
}

/// Whether the instruction `opc` is followed by a type word (1.2): the last word of the
/// instruction, after the extension word of a `Wide`, holding a type index (`RefFieldSet`,
/// `RefFieldSetRef`, `AggLoad`, `AggStore`, `LendPlace`).
#[inline(always)]
pub(crate) fn has_type_word(opc: u16) -> bool {
    matches!(
        opc,
        op::REF_FIELD_SET | op::REF_FIELD_SET_REF | op::AGG_LOAD | op::AGG_STORE | op::LEND_PLACE
    )
}

/// The type index in the type word of the instruction at the boundary `pos` whose length is `len`
/// (as [`decode_at`] gives it; the instruction has a type word).
pub(crate) fn type_word_at(code: &[u32], pos: usize, len: usize) -> u32 {
    code[pos + len - 1]
}

/// `Bx` of the instruction at the boundary `pos` (16 bits, or 32 bits after a `Wide`; see
/// `decode_bx`). The prefixes at `pos` must be followed by their words ([`decode_at`]).
pub(crate) fn bx_at(code: &[u32], pos: usize) -> u32 {
    let (wide, _, at) = prefixes_at(code, pos);
    decode_bx(code[at], wide.then(|| code[at + 1]))
}

/// Writes `Bx` of the instruction at the boundary `pos`, keeping its width (the inverse of
/// [`bx_at`] and `decode_bx`): without `Wide`, `bx` must fit in 16 bits.
pub(crate) fn set_bx_at(code: &mut [u32], pos: usize, bx: u32) {
    let (wide, _, at) = prefixes_at(code, pos);
    if wide {
        code[at] = (code[at] & 0xFFFF) | (bx << 16);
        code[at + 1] = (code[at + 1] & 0xFFFF) | (bx & 0xFFFF_0000);
    } else {
        assert!(bx <= 0xFFFF, "a narrow `Bx` holds 16 bits");
        code[at] = (code[at] & 0xFFFF) | (bx << 16);
    }
}

/// `sBx`: `Bx` read as a signed 16-bit value, or 32-bit after a `Wide` (1.2).
pub(crate) fn signed_bx(bx: u32, wide: bool) -> i64 {
    if wide {
        bx as i32 as i64
    } else {
        bx as u16 as i16 as i64
    }
}

/// `sC`: `C` read as a signed 8-bit value, or 16-bit after a `Wide` (1.2).
pub(crate) fn signed_c(c: u32, wide: bool) -> i64 {
    if wide {
        c as u16 as i16 as i64
    } else {
        c as u8 as i8 as i64
    }
}

/// `sB`: `B` read as a signed value, the same widths as `sC` (the immediate of the
/// compare-and-branch instructions with an immediate; 2.3).
pub(crate) fn signed_b(b: u32, wide: bool) -> i64 {
    signed_c(b, wide)
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

/// The largest width of a flat record or static array (an array's element width, the type of
/// `AggLoad`, `AggStore` and `LendPlace`; `docs/design/records.md` 3.2 B).
pub const MAX_FLAT_WIDTH: u32 = 16;

/// The first type index of the type section; 0–63 are the VM's built-in types (3.2).
pub const FIRST_TYPE_INDEX: u32 = 64;

/// Kind of a type section entry (3.2).
pub const TYPE_RECORD: u8 = 1;
pub const TYPE_ARRAY: u8 = 2;
pub const TYPE_DYNARRAY: u8 = 3;
// Kind 4 (an exception class until M3-35) is unused: exception classes are classes (kind 7;
// `docs/spec/objects.md` 14.8).
/// A big set: its number of bits (`docs/spec/types.md` 8.3).
pub const TYPE_SET: u8 = 5;
/// An enumeration: the table of its value names (`docs/spec/types.md` 8.1).
pub const TYPE_ENUM: u8 = 6;
/// A class (`docs/spec/objects.md` 14.1): its parent, flags and virtual method table.
pub const TYPE_CLASS: u8 = 7;
/// An interface (`docs/spec/objects.md` 14.7): its parent, number of methods and GUID.
pub const TYPE_INTERFACE: u8 = 8;

/// `TypeEntry::flags` of a reference counted class (`TRefObject` and its descendants).
pub const CLASS_REF_COUNTED: u32 = 1;
/// `TypeEntry::flags` of a class that cannot be instantiated (`class abstract`, or an abstract
/// method left).
pub const CLASS_ABSTRACT: u32 = 2;
/// `TypeEntry::flags` of a `sealed` class.
pub const CLASS_SEALED: u32 = 4;

/// The first reserved virtual method table entry: `VMT_BUILTIN + k` is the built-in body `k`
/// (`docs/spec/objects.md` 14.1).
pub const VMT_BUILTIN: u32 = 0xFFFF_FF00;
/// The built-in body that does nothing (`Destroy`, `AfterConstruction`, `BeforeDestruction`).
pub const VMT_BUILTIN_NOTHING: u32 = VMT_BUILTIN;
/// The built-in body of `TObject.ToString` (returns `ClassName`).
pub const VMT_BUILTIN_CLASS_NAME: u32 = VMT_BUILTIN + 1;
/// The built-in body of `Exception.ToString` (returns `Message`; `docs/spec/objects.md` 14.8).
pub const VMT_BUILTIN_MESSAGE: u32 = VMT_BUILTIN + 2;
/// The built-in body of `TObject.Equals` (the same instance; `docs/spec/equality.md` 3.1, 8.2).
pub const VMT_BUILTIN_EQUALS: u32 = VMT_BUILTIN + 3;
/// The built-in body of `TObject.GetHashCode` (`HashInt` of the handle; `docs/spec/equality.md`
/// 3.1, 8.2).
pub const VMT_BUILTIN_HASH: u32 = VMT_BUILTIN + 4;
/// The fixed entries of every class's virtual method table (`docs/spec/objects.md` chapter 5).
pub const VMT_DESTROY: u32 = 0;
pub const VMT_AFTER_CONSTRUCTION: u32 = 1;
pub const VMT_BEFORE_DESTRUCTION: u32 = 2;
pub const VMT_EQUALS: u32 = 4;
pub const VMT_GET_HASH_CODE: u32 = 5;

/// The virtual method table entry of an abstract method.
pub const VMT_ABSTRACT: u32 = 0xFFFF_FFFF;

/// The virtual method table of `TObject` and `TRefObject` (`Destroy`, `AfterConstruction`,
/// `BeforeDestruction`, `ToString`, `Equals`, `GetHashCode`).
pub const TOBJECT_VMT: [u32; 6] = [
    VMT_BUILTIN_NOTHING,
    VMT_BUILTIN_NOTHING,
    VMT_BUILTIN_NOTHING,
    VMT_BUILTIN_CLASS_NAME,
    VMT_BUILTIN_EQUALS,
    VMT_BUILTIN_HASH,
];

/// The virtual method table of `Exception` and the built-in exception classes
/// (`docs/spec/objects.md` 14.8): `ToString` returns `Message`.
pub const EXCEPTION_VMT: [u32; 6] = [
    VMT_BUILTIN_NOTHING,
    VMT_BUILTIN_NOTHING,
    VMT_BUILTIN_NOTHING,
    VMT_BUILTIN_MESSAGE,
    VMT_BUILTIN_EQUALS,
    VMT_BUILTIN_HASH,
];

/// A type section entry (3.2): records, static arrays, dynamic arrays, sets, enumerations and
/// classes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct TypeEntry {
    /// [`TYPE_RECORD`], [`TYPE_ARRAY`], … [`TYPE_CLASS`] (0 for an unused built-in type index).
    pub kind: u8,
    /// Fields of a record, elements of a static array (0 for a dynamic array), slots of a class's
    /// instances.
    pub slots: u32,
    /// The kind of each slot ([`SLOT_PLAIN`], [`SLOT_REF`], [`SLOT_OBJ`]): one per slot for a
    /// record or a class (flat fields included), one per slot of an element for an array (`width`
    /// of them).
    pub refs: Vec<u8>,
    /// The width of an element of an array: the number of slots it takes, 1 to [`MAX_FLAT_WIDTH`]
    /// (more than 1 only for an element that is a flat record or static array; 3.2). 0 for the
    /// other kinds.
    pub width: u32,
    /// The fully qualified name (`Hello.TPoint`), or the written form of an anonymous type.
    pub name: String,
    /// The parent's type index of a class (`None` for the root `TObject` and for the other
    /// kinds).
    pub parent: Option<u32>,
    /// The values of an enumeration in declaration order: (ordinal, name as declared). Empty for
    /// the other kinds.
    pub values: Vec<(i64, String)>,
    /// The flags of a class ([`CLASS_REF_COUNTED`], [`CLASS_ABSTRACT`], [`CLASS_SEALED`]; 0 for
    /// the other kinds).
    pub flags: u32,
    /// The virtual method table of a class (`docs/spec/objects.md` 14.1): function indexes or the
    /// reserved values ([`VMT_BUILTIN`] + k, [`VMT_ABSTRACT`]), the parent's entries first. Empty
    /// for the other kinds.
    pub vmt: Vec<u32>,
    /// The names of the abstract entries of a class's table (`docs/spec/objects.md` 14.1): (entry,
    /// `TShape.Area`), in entry order, for the message of `EAbstractError`. Empty for the other
    /// kinds.
    pub abstract_names: Vec<(u32, String)>,
    /// The interface table of a class (`docs/spec/objects.md` 14.7): one entry per interface the
    /// class implements. Empty for the other kinds.
    pub intfs: Vec<IntfEntry>,
    /// The number of methods of an interface, its ancestors' included (0 for the other kinds).
    pub methods: u32,
    /// The GUID of an interface as written (all zeros when none is written; `docs/spec/objects.md`
    /// 14.7).
    pub guid: [u8; 16],
}

/// An entry of a class's interface table (`docs/spec/objects.md` 14.7): the interface, and the
/// function of each of its methods (a function index, [`VMT_BUILTIN`] + k or [`VMT_ABSTRACT`]),
/// in method number order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct IntfEntry {
    pub intf: u32,
    pub funcs: Vec<u32>,
}

impl TypeEntry {
    /// The entry of the interface `intf` in the interface table of this class (`None` if the
    /// class does not implement it, and for the other kinds; `docs/spec/objects.md` 7.3, 14.7).
    pub fn intf_entry(&self, intf: u32) -> Option<&IntfEntry> {
        self.intfs.iter().find(|e| e.intf == intf)
    }

    /// The name of an ordinal of an enumeration: the first value declared with it, or the
    /// ordinal in decimal (`docs/spec/types.md` 8.1).
    pub fn enum_name(&self, ord: i64) -> String {
        self.values
            .iter()
            .find(|(o, _)| *o == ord)
            .map_or_else(|| ord.to_string(), |(_, n)| n.clone())
    }

    /// The kind of slot `i` ([`SLOT_PLAIN`], [`SLOT_REF`] or [`SLOT_OBJ`]): of an array, that of
    /// the same place in its element (`i` modulo the width).
    pub fn slot_kind(&self, i: usize) -> u8 {
        match self.kind {
            TYPE_RECORD | TYPE_CLASS => self.refs.get(i).copied().unwrap_or(0),
            TYPE_ARRAY | TYPE_DYNARRAY if self.width > 1 => {
                self.refs.get(i % self.width as usize).copied().unwrap_or(0)
            }
            _ => self.refs.first().copied().unwrap_or(0),
        }
    }

    /// The number of slots of an element of an array (its width; 1 for the other kinds, whose
    /// slots are the elements themselves).
    pub fn elem_width(&self) -> usize {
        match self.kind {
            TYPE_ARRAY | TYPE_DYNARRAY => self.width.max(1) as usize,
            _ => 1,
        }
    }

    /// The number of slots a value of this record or static array type takes when it is laid out
    /// flat (3.2): the fields of a record, the elements times their width of a static array.
    /// `None` for the other kinds.
    pub fn flat_width(&self) -> Option<u64> {
        match self.kind {
            TYPE_RECORD => Some(u64::from(self.slots)),
            TYPE_ARRAY => Some(u64::from(self.slots) * self.elem_width() as u64),
            _ => None,
        }
    }
}

/// Built-in exception classes (`docs/spec/exceptions.md` chapter 2): the type index, the name,
/// the parent and whether the class adds a slot (a string or not). Index 12 is
/// `TArray<Exception>` ([`EXCEPTION_ARRAY_TYPE`]), so the M2b classes start at 13. They are
/// reference counted classes; the parent of `Exception` is `TRefObject` (`docs/spec/objects.md`
/// 8.1).
pub const BUILTIN_EXCEPTIONS: &[(u32, &str, Option<u32>, Option<bool>)] = &[
    (0, "Exception", Some(TREFOBJECT_TYPE), None),
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
    (18, "EAliasError", Some(0), None),
    (21, "EInvalidCast", Some(0), None),
    (22, "EAbstractError", Some(0), None),
    (23, "EInvalidOperation", Some(0), None),
];

/// The type index of the root class `TObject` (`docs/spec/objects.md` 14.1).
pub const TOBJECT_TYPE: u32 = 19;
/// The type index of `TRefObject`, the root of the reference counted classes.
pub const TREFOBJECT_TYPE: u32 = 20;

/// The type index of `IInterface`, the root of the interfaces (`docs/spec/objects.md` 14.7).
pub const IINTERFACE_TYPE: u32 = 24;

/// The type index of `TCustomAttribute`, the root of the attribute classes
/// (`docs/spec/attributes.md` 4.1): a reference counted class with no members of its own.
pub const TCUSTOMATTRIBUTE_TYPE: u32 = 25;

/// The type index of `TIndexRange`, the built-in record a range written as an index of a
/// default array property makes (`docs/spec/objects.md` 12.3, 14.10): the slots `First`,
/// `Last`, `HasFirst` and `HasLast`.
pub const TINDEXRANGE_TYPE: u32 = 26;

/// One more than the largest type index of a built-in type.
pub const BUILTIN_TYPE_END: u32 = 27;

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
    pub const ALIAS_ERROR: u32 = 18;
    pub const INVALID_CAST: u32 = 21;
    pub const ABSTRACT_ERROR: u32 = 22;
    pub const INVALID_OPERATION: u32 = 23;
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

/// How `ArraySortRange` (`I96`) compares the elements (its `r(B+2)`; `docs/spec/bytecode.md`
/// 2.22): the slots as signed or unsigned 64-bit integers, as `Single` (the low 32 bits) or
/// `Double` in the total preorder of `KeyCompare`, as strings (`CmpStr`) or as `UTF8String`s
/// (`CmpU8`).
pub const SORT_INT: i64 = 0;
pub const SORT_UINT: i64 = 1;
pub const SORT_SINGLE: i64 = 2;
pub const SORT_DOUBLE: i64 = 3;
pub const SORT_STR: i64 = 4;
pub const SORT_UTF8: i64 = 5;

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

/// The VM's built-in types, type indexes 0 to 63 (`docs/spec/exceptions.md` 6.1). Unused
/// indexes have kind 0.
pub fn builtin_types() -> &'static [TypeEntry] {
    static TYPES: std::sync::OnceLock<Vec<TypeEntry>> = std::sync::OnceLock::new();
    TYPES.get_or_init(|| {
        let empty = || TypeEntry {
            abstract_names: Vec::new(),
            intfs: Vec::new(),
            methods: 0,
            guid: [0; 16],
            kind: 0,
            slots: 0,
            refs: Vec::new(),
            width: 0,
            name: String::new(),
            parent: None,
            values: Vec::new(),
            flags: 0,
            vmt: Vec::new(),
        };
        let mut v: Vec<TypeEntry> = (0..BUILTIN_TYPE_END).map(|_| empty()).collect();
        for (index, name, parent, extra) in BUILTIN_EXCEPTIONS {
            let mut refs = vec![SLOT_REF, SLOT_REF];
            if let Some(r) = extra {
                refs.push(u8::from(*r));
            }
            v[*index as usize] = TypeEntry {
                kind: TYPE_CLASS,
                slots: refs.len() as u32,
                refs,
                width: 0,
                name: name.to_string(),
                parent: *parent,
                values: Vec::new(),
                flags: CLASS_REF_COUNTED,
                vmt: EXCEPTION_VMT.to_vec(),
                abstract_names: Vec::new(),
                intfs: Vec::new(),
                methods: 0,
                guid: [0; 16],
            };
        }
        v[EXCEPTION_ARRAY_TYPE as usize] = TypeEntry {
            kind: TYPE_DYNARRAY,
            slots: 0,
            refs: vec![SLOT_REF],
            width: 1,
            name: "TArray<Exception>".into(),
            parent: None,
            values: Vec::new(),
            flags: 0,
            vmt: Vec::new(),
            abstract_names: Vec::new(),
            intfs: Vec::new(),
            methods: 0,
            guid: [0; 16],
        };
        // The root classes (`docs/spec/objects.md` 14.1)
        v[TOBJECT_TYPE as usize] = TypeEntry {
            kind: TYPE_CLASS,
            name: "TObject".into(),
            vmt: TOBJECT_VMT.to_vec(),
            ..TypeEntry::default()
        };
        v[TREFOBJECT_TYPE as usize] = TypeEntry {
            kind: TYPE_CLASS,
            name: "TRefObject".into(),
            parent: Some(TOBJECT_TYPE),
            flags: CLASS_REF_COUNTED,
            vmt: TOBJECT_VMT.to_vec(),
            ..TypeEntry::default()
        };
        v[IINTERFACE_TYPE as usize] = TypeEntry {
            kind: TYPE_INTERFACE,
            name: "IInterface".into(),
            ..TypeEntry::default()
        };
        v[TCUSTOMATTRIBUTE_TYPE as usize] = TypeEntry {
            kind: TYPE_CLASS,
            name: "TCustomAttribute".into(),
            parent: Some(TREFOBJECT_TYPE),
            flags: CLASS_REF_COUNTED,
            vmt: TOBJECT_VMT.to_vec(),
            ..TypeEntry::default()
        };
        v[TINDEXRANGE_TYPE as usize] = TypeEntry {
            kind: TYPE_RECORD,
            slots: 4,
            refs: vec![SLOT_PLAIN; 4],
            name: "TIndexRange".into(),
            ..TypeEntry::default()
        };
        while v.len() < FIRST_TYPE_INDEX as usize {
            v.push(TypeEntry {
                kind: 0,
                slots: 0,
                refs: Vec::new(),
                width: 0,
                name: String::new(),
                parent: None,
                values: Vec::new(),
                flags: 0,
                vmt: Vec::new(),
                abstract_names: Vec::new(),
                intfs: Vec::new(),
                methods: 0,
                guid: [0; 16],
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
    /// `docs/design/lifetime.md` 3.8).
    pub managed: Vec<ManagedReg>,
    /// Source file (index into the unit section).
    pub file: u32,
    /// The handler table, inner entries first (`docs/spec/exceptions.md` 6.3).
    pub handlers: Vec<HandlerEntry>,
    /// For a nested routine taking the static link as its first argument, the function it is
    /// nested in, whose frame the link points to (`docs/spec/semantics.md` 4.9).
    pub outer: Option<u32>,
    /// For a function shared by several instantiations, their full names
    /// (`docs/spec/generics.md` 7, 8): shown by the disassembly only, not written in the file.
    pub shared_by: Vec<String>,
    /// The inline table (3.2): the calls inlined into the function, ordered by start, an entry
    /// before the entries inside it.
    pub inlines: Vec<InlineEntry>,
}

/// An entry of a function's inline table (3.2, `docs/design/inlining.md` 3.6): the code that
/// stands for the body of the function `func`, called at `line`. Positions are word positions;
/// the range does not include its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineEntry {
    pub start: u32,
    pub end: u32,
    pub func: u32,
    /// The line of the call (in the function holding the entry, or in the function of `outer`).
    pub line: u32,
    /// The entry whose code the call is in (an earlier entry of the table).
    pub outer: Option<u32>,
}

/// `InlineEntry::outer` of an entry not inside another (in the module file).
const NO_OUTER_INLINE: u32 = u32::MAX;

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
    /// The entries of the inline table whose range holds the word position `pos`, innermost
    /// first.
    pub fn inline_chain(&self, pos: u32) -> Vec<usize> {
        let after = self.inlines.partition_point(|e| e.start <= pos);
        let Some(inner) = (0..after).rev().find(|&k| pos < self.inlines[k].end) else {
            return Vec::new();
        };
        std::iter::successors(Some(inner), |&k| self.inlines[k].outer.map(|o| o as usize)).collect()
    }

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
/// `GlobalEntry::kind` of a global variable counted by its contents (a value whose static type is
/// exactly `TObject`; `docs/spec/objects.md` 14.1).
pub const GLOBAL_OBJ: u32 = 3;

/// Kind of a managed register entry (3.2): a heap reference.
pub const MANAGED_HEAP_REF: u16 = 1;
/// Kind of a managed register entry: a handle counted by its contents (a value whose static type
/// is exactly `TObject`; `docs/spec/objects.md` 14.1).
pub const MANAGED_OBJ: u16 = 2;

/// An entry of a function's managed register list (3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedReg {
    pub reg: u16,
    /// [`MANAGED_HEAP_REF`] or [`MANAGED_OBJ`].
    pub kind: u16,
}

/// The kinds of a slot of a type entry (3.2): not managed.
pub const SLOT_PLAIN: u8 = 0;
/// A slot holding a heap reference (a string, an array, a record, an instance of a reference
/// counted class, …).
pub const SLOT_REF: u8 = 1;
/// A slot counted by its contents (`docs/spec/objects.md` 14.1).
pub const SLOT_OBJ: u8 = 2;

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
    /// The interface call targets (`docs/spec/objects.md` 14.7): (interface type index, method
    /// number), the `Bx` of `CallIntf`.
    pub intf_calls: Vec<(u32, u32)>,
}

/// The kind of the interface call target section (3.1; `docs/spec/objects.md` 14.7).
const SECTION_INTF_CALLS: u32 = 9;

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
        // So is the interface call target section (kind 9; 7 and 8 are planned for M5;
        // `docs/spec/objects.md` 14.7)
        if !self.intf_calls.is_empty() {
            let mut w = Writer { buf: Vec::new() };
            w.u32(self.intf_calls.len() as u32);
            for (intf, k) in &self.intf_calls {
                w.u32(*intf);
                w.u32(*k);
            }
            sections.push((SECTION_INTF_CALLS, w.buf));
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
            if matches!(t.kind, TYPE_ARRAY | TYPE_DYNARRAY) {
                w.u32(t.width);
            }
            for r in &t.refs {
                w.u8(*r);
            }
            w.pad4();
            w.lp_utf8(&t.name);
            if t.kind == TYPE_CLASS {
                w.u32(t.parent.unwrap_or(0));
                w.u32(t.flags);
                w.u32(t.vmt.len() as u32);
                for e in &t.vmt {
                    w.u32(*e);
                }
                w.u32(t.abstract_names.len() as u32);
                for (k, name) in &t.abstract_names {
                    w.u32(*k);
                    w.lp_utf8(name);
                }
                w.u32(t.intfs.len() as u32);
                for e in &t.intfs {
                    w.u32(e.intf);
                    w.u32(e.funcs.len() as u32);
                    for f in &e.funcs {
                        w.u32(*f);
                    }
                }
            }
            if t.kind == TYPE_INTERFACE {
                w.u32(t.parent.unwrap_or(0));
                w.u32(t.methods);
                w.buf.extend_from_slice(&t.guid);
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
            for m in &f.managed {
                w.u16(m.reg);
                w.u16(m.kind);
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
            w.u32(f.inlines.len() as u32);
            for e in &f.inlines {
                w.u32(e.start);
                w.u32(e.end);
                w.u32(e.func);
                w.u32(e.line);
                w.u32(e.outer.unwrap_or(NO_OUTER_INLINE));
            }
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

/// A list of managed registers (the managed register list and an entry of the virtual call
/// arguments; 3.2): a count and { register `u16`, kind `u16` }.
fn decode_managed(r: &mut Reader) -> Result<Vec<ManagedReg>, LoadError> {
    let n = r.count("managed register")?;
    let mut managed = Vec::with_capacity(n);
    for _ in 0..n {
        let reg = r.u16()?;
        let kind = r.u16()?;
        if kind != MANAGED_HEAP_REF && kind != MANAGED_OBJ {
            return Err(LoadError::Invalid(format!(
                "unknown managed register kind {kind}"
            )));
        }
        managed.push(ManagedReg { reg, kind });
    }
    Ok(managed)
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
        let mut sections: [Option<&[u8]>; 10] = [None; 10];
        let mut last_end = 0u32;
        for (kind, off, len) in table {
            let end = off
                .checked_add(len)
                .ok_or_else(|| LoadError::Invalid("section out of range".into()))?;
            if end as usize > bytes.len() || off < last_end {
                return Err(LoadError::Invalid("section out of range".into()));
            }
            last_end = end;
            if !matches!(kind, 1..=6 | SECTION_INTF_CALLS) {
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
        let intf_calls = match sections[SECTION_INTF_CALLS as usize] {
            Some(b) => {
                let mut r = Reader { buf: b, pos: 0 };
                let n = r.count("interface call target")?;
                let mut out = Vec::with_capacity(n);
                for _ in 0..n {
                    out.push((r.u32()?, r.u32()?));
                }
                out
            }
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
            intf_calls,
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
            let width = if matches!(kind, TYPE_ARRAY | TYPE_DYNARRAY) {
                let w = r.u32()?;
                if !(1..=MAX_FLAT_WIDTH).contains(&w) {
                    return Err(LoadError::Invalid(format!(
                        "array element width {w} out of range"
                    )));
                }
                w
            } else {
                0
            };
            let nrefs = match kind {
                TYPE_RECORD | TYPE_CLASS => slots as usize,
                TYPE_ARRAY | TYPE_DYNARRAY => width as usize,
                TYPE_ENUM | TYPE_SET | TYPE_INTERFACE => 0,
                k => return Err(LoadError::Invalid(format!("unknown type kind {k}"))),
            };
            r.need(nrefs)?;
            let mut refs = Vec::with_capacity(nrefs);
            for _ in 0..nrefs {
                refs.push(match r.u8()? {
                    k @ (SLOT_PLAIN | SLOT_REF | SLOT_OBJ) => k,
                    k => return Err(LoadError::Invalid(format!("unknown slot kind {k}"))),
                });
            }
            r.pad4()?;
            let name = r.lp_utf8()?;
            let parent = if matches!(kind, TYPE_CLASS | TYPE_INTERFACE) {
                Some(r.u32()?)
            } else {
                None
            };
            let mut flags = 0;
            let mut vmt = Vec::new();
            let mut abstract_names = Vec::new();
            let mut intfs = Vec::new();
            if kind == TYPE_CLASS {
                flags = r.u32()?;
                let n = r.count("virtual method")?;
                for _ in 0..n {
                    vmt.push(r.u32()?);
                }
                let n = r.count("abstract method name")?;
                for _ in 0..n {
                    let k = r.u32()?;
                    abstract_names.push((k, r.lp_utf8()?));
                }
                let n = r.count("interface")?;
                for _ in 0..n {
                    let intf = r.u32()?;
                    let m = r.count("interface method")?;
                    let mut funcs = Vec::with_capacity(m);
                    for _ in 0..m {
                        funcs.push(r.u32()?);
                    }
                    intfs.push(IntfEntry { intf, funcs });
                }
            }
            let mut methods = 0;
            let mut guid = [0u8; 16];
            if kind == TYPE_INTERFACE {
                if slots != 0 {
                    return Err(LoadError::Invalid("interface type with slots".into()));
                }
                methods = r.u32()?;
                guid.copy_from_slice(r.bytes(16)?);
            }
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
            if kind == TYPE_ARRAY && u64::from(slots) * u64::from(width) > i32::MAX as u64 {
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
                width,
                name,
                parent,
                values,
                flags,
                vmt,
                abstract_names,
                intfs,
                methods,
                guid,
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
            let managed = decode_managed(&mut r)?;
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
            let ninlines = r.count("inline")?;
            let mut inlines = Vec::with_capacity(ninlines);
            for _ in 0..ninlines {
                inlines.push(InlineEntry {
                    start: r.u32()?,
                    end: r.u32()?,
                    func: r.u32()?,
                    line: r.u32()?,
                    outer: Some(r.u32()?).filter(|&o| o != NO_OUTER_INLINE),
                });
            }
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
                shared_by: Vec::new(),
                inlines,
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
            if !matches!(
                kind,
                GLOBAL_INT | GLOBAL_REF | GLOBAL_EXIT_CODE | GLOBAL_OBJ
            ) {
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
                // One item per slot: a static array whose elements are flat records has their
                // slots (3.2)
                let n = t.flat_width().unwrap_or(0);
                if items.len() as u64 != n {
                    return Err(LoadError::Invalid(format!(
                        "constant {k}: {} elements for a type of {n} slots",
                        items.len(),
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
            if t.kind == TYPE_CLASS {
                self.verify_class(index, t)?;
            }
            if t.kind == TYPE_INTERFACE {
                self.verify_interface(index, t)?;
            }
        }
        // The interface call targets (`docs/spec/objects.md` 14.7)
        for (i, (intf, k)) in self.intf_calls.iter().enumerate() {
            let ok = self
                .type_entry(*intf)
                .is_some_and(|t| t.kind == TYPE_INTERFACE && *k < t.methods);
            if !ok {
                return Err(LoadError::Invalid(format!(
                    "interface call target {i} is not a method of an interface"
                )));
            }
        }
        for (fi, f) in self.funcs.iter().enumerate() {
            self.verify_function(fi, f)?;
        }
        Ok(())
    }

    /// A class entry (`docs/spec/objects.md` 14.2): its parent is a built-in class or an earlier
    /// class, of the same kind (managed by hand or reference counted), its slots start with the
    /// parent's, its virtual method table is at least as long as the parent's and holds function
    /// indexes or reserved values, and only an abstract class has an abstract entry.
    fn verify_class(&self, index: u32, t: &TypeEntry) -> Result<(), LoadError> {
        let err = |m: &str| Err(LoadError::Invalid(format!("type {index}: {m}")));
        let parent = t.parent.unwrap_or(index);
        let Some(p) = self
            .type_entry(parent)
            .filter(|p| parent < index && p.kind == TYPE_CLASS)
        else {
            return err("the parent is not an earlier class");
        };
        if t.flags & !(CLASS_REF_COUNTED | CLASS_ABSTRACT | CLASS_SEALED) != 0 {
            return err("unknown class flags");
        }
        if (t.flags ^ p.flags) & CLASS_REF_COUNTED != 0 {
            return err("the parent is of another kind (managed by hand or reference counted)");
        }
        if p.flags & CLASS_SEALED != 0 {
            return err("the parent is sealed");
        }
        if t.slots < p.slots || t.refs[..p.slots as usize] != p.refs[..] {
            return err("the slots do not start with the parent's");
        }
        if t.vmt.len() < p.vmt.len() {
            return err("the virtual method table is shorter than the parent's");
        }
        // `Exception.ToString` reads `Message`, so only an exception class has it
        let exception = self.class_descends(parent, exc::EXCEPTION);
        if t.vmt
            .iter()
            .any(|e| !self.table_entry_ok(*e, exception, t.flags))
        {
            return err("a virtual method table entry is out of range");
        }
        // The abstract entries have one name each, in entry order (`EAbstractError`)
        let abstract_entries: Vec<u32> = (0..t.vmt.len() as u32)
            .filter(|&k| t.vmt[k as usize] == VMT_ABSTRACT)
            .collect();
        if t.abstract_names
            .iter()
            .map(|(k, _)| *k)
            .ne(abstract_entries)
        {
            return err("the abstract method names do not match the abstract entries");
        }
        // The interface table (`docs/spec/objects.md` 14.7): interfaces, each once, with one
        // callable entry per method; closed over the interfaces' ancestors, and holding the
        // parent's interfaces
        for (i, e) in t.intfs.iter().enumerate() {
            let Some(it) = self
                .type_entry(e.intf)
                .filter(|it| it.kind == TYPE_INTERFACE)
            else {
                return err("an interface table entry is not an interface");
            };
            if t.intfs[..i].iter().any(|o| o.intf == e.intf) {
                return err("an interface is in the interface table twice");
            }
            if e.funcs.len() != it.methods as usize {
                return err("an interface table entry has the wrong number of methods");
            }
            if e.funcs
                .iter()
                .any(|f| !self.table_entry_ok(*f, exception, t.flags))
            {
                return err("an interface table entry is out of range");
            }
            if let Some(ip) = it.parent
                && !t.intfs.iter().any(|o| o.intf == ip)
            {
                return err("an interface's parent is not in the interface table");
            }
        }
        if p.intfs
            .iter()
            .any(|pe| !t.intfs.iter().any(|e| e.intf == pe.intf))
        {
            return err("the interface table does not hold the parent's interfaces");
        }
        Ok(())
    }

    /// Whether `e` is a callable entry of a class's virtual method table or interface table
    /// (`docs/spec/objects.md` 14.1, 14.7): a function, a built-in body (`Message` only for an
    /// exception class), or abstract in an abstract class (`flags` are the class's).
    fn table_entry_ok(&self, e: u32, exception: bool, flags: u32) -> bool {
        (e as usize) < self.funcs.len()
            || (VMT_BUILTIN_NOTHING..=VMT_BUILTIN_CLASS_NAME).contains(&e)
            || (e == VMT_BUILTIN_MESSAGE && exception)
            || (VMT_BUILTIN_EQUALS..=VMT_BUILTIN_HASH).contains(&e)
            || (e == VMT_ABSTRACT && flags & CLASS_ABSTRACT != 0)
    }

    /// An interface entry (`docs/spec/objects.md` 14.7): its parent is `IInterface` or an earlier
    /// interface, and it has at least the parent's methods.
    fn verify_interface(&self, index: u32, t: &TypeEntry) -> Result<(), LoadError> {
        let err = |m: &str| Err(LoadError::Invalid(format!("type {index}: {m}")));
        let parent = t.parent.unwrap_or(index);
        let Some(p) = self
            .type_entry(parent)
            .filter(|p| parent < index && p.kind == TYPE_INTERFACE)
        else {
            return err("the parent is not an earlier interface");
        };
        if t.methods < p.methods {
            return err("the interface has fewer methods than its parent");
        }
        Ok(())
    }

    /// Whether the class `class` implements the interface `intf` (its interface table has it;
    /// `docs/spec/objects.md` 14.7).
    pub fn implements(&self, class: u32, intf: u32) -> Option<&IntfEntry> {
        self.type_entry(class)?.intf_entry(intf)
    }

    /// Whether the class `index` is `ancestor` or a descendant of it (the parents come first, so
    /// the walk ends; the verifier checks each class's parent before its own entry).
    pub fn class_descends(&self, index: u32, ancestor: u32) -> bool {
        let mut cur = Some(index);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self
                .type_entry(c)
                .filter(|t| t.kind == TYPE_CLASS)
                .and_then(|t| t.parent)
                .filter(|p| *p < c);
        }
        false
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
            // A `finally` block may be empty (`try … finally end`): it is just its `EndFinally`
            let handler_ordered = if h.kind == HANDLER_FINALLY {
                h.handler_start <= h.handler_end
            } else {
                h.handler_start < h.handler_end
            };
            if !(h.body_start < h.body_end && h.body_end <= h.handler_start && handler_ordered) {
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

    /// The inline table (3.3): each range is a non-empty run of whole instructions, inside the
    /// range of its outer entry (an earlier one), and the entries are ordered by start, each
    /// nested in or apart from the entries before it.
    fn verify_inlines(&self, f: &Function, boundaries: &[u32]) -> Result<(), String> {
        let len = f.code.len() as u32;
        let at_boundary = |p: u32| p == len || boundaries.binary_search(&p).is_ok();
        for (i, e) in f.inlines.iter().enumerate() {
            let what = format!("inline entry {i}");
            if !(e.start < e.end && e.end <= len && at_boundary(e.start) && at_boundary(e.end)) {
                return Err(format!("{what} has a range off the instructions"));
            }
            if e.func as usize >= self.funcs.len() {
                return Err(format!("{what} names function {} out of range", e.func));
            }
            if let Some(o) = e.outer {
                let Some(outer) = f.inlines[..i].get(o as usize) else {
                    return Err(format!(
                        "{what} has an outer entry that does not come before it"
                    ));
                };
                if !(outer.start <= e.start && e.end <= outer.end) {
                    return Err(format!("{what} is not inside its outer entry"));
                }
            }
            if let Some(prev) = i.checked_sub(1).map(|p| f.inlines[p])
                && prev.start > e.start
            {
                return Err(format!("{what} starts before the entry before it"));
            }
            // An earlier entry overlapping this one holds it: it is its outer entry or one
            // around that
            for (j, g) in f.inlines[..i].iter().enumerate() {
                if g.end <= e.start || e.end <= g.start {
                    continue;
                }
                let mut k = e.outer;
                while let Some(o) = k.filter(|&o| o as usize != j) {
                    k = f.inlines[o as usize].outer;
                }
                if k.is_none() {
                    return Err(format!(
                        "{what} overlaps inline entry {j} without being inside it"
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
        let mut last: Option<u16> = None;
        // `EndFinally`: (index into `boundaries`, `A`, number of `Jmp`s)
        let mut end_finally: Vec<(usize, u32, u32)> = Vec::new();
        while pc < code.len() {
            boundaries.push(pc as u32);
            if let Some(m) = prefix_error(code, pc) {
                return Err(err(m.into()));
            }
            let (opc, a, b, c, len) = decode_at(code, pc);
            let wide = is_wide_at(code, pc);
            if pc + len > code.len() {
                return Err(err(format!("missing type word at {pc:04x}")));
            }
            // An intrinsic is checked by its own shape (2.19)
            let info = op::resolve(opc, c).ok_or_else(|| {
                if op::info(opc).is_some() {
                    err(format!("unknown intrinsic {c} at {pc:04x}"))
                } else {
                    err(format!("unknown opcode {opc} at {pc:04x}"))
                }
            })?;
            let bx = bx_at(code, pc);
            let regs = f.regs as u32;
            let reg = |r: u32, what: &str| -> Result<(), LoadError> {
                if r >= regs {
                    Err(err(format!("register {what}={r} out of range at {pc:04x}")))
                } else {
                    Ok(())
                }
            };
            // The type word of `AggLoad`, `AggStore` and `LendPlace` (2.8): a record or static
            // array that fits a flat place
            let flat_type = |t: u32| -> Result<(), LoadError> {
                match self.type_entry(t) {
                    Some(e) => match e.flat_width() {
                        Some(w) if (1..=u64::from(MAX_FLAT_WIDTH)).contains(&w) => Ok(()),
                        Some(_) => Err(err(format!("type {t} is too wide at {pc:04x}"))),
                        Option::None => {
                            Err(err(format!("type {t} has the wrong kind at {pc:04x}")))
                        }
                    },
                    Option::None => Err(err(format!("type {t} out of range at {pc:04x}"))),
                }
            };
            // A cursor in the short form has both registers in 8 bits (1.2)
            if !wide
                && let Some(r) = info.shape.cursor_end(a, b)
                && r > 255
            {
                return Err(err(format!(
                    "cursor ending at r{r} without `Wide` at {pc:04x}"
                )));
            }
            use op::Shape::*;
            match info.shape {
                Intrinsic => unreachable!("`resolve` gives an intrinsic's own shape"),
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
                    // The width of `Narrow` and friends and `StrToIntIO` (2.2)
                    if c > 8 {
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
                // `docs/spec/bytecode.md` 2.23: the kinds, lengths and indices of the arrays are
                // checked when it runs
                ABRange5 => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 4, "B+4")?;
                    if c != 0 {
                        return Err(err(format!("C={c} is not 0 at {pc:04x}")));
                    }
                }
                // `C` is a slot of the instance `rB`, checked when it runs as `ObjGet`'s
                ABRange2Slot => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 1, "B+1")?;
                }
                ABPairs => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 2 * c, "B+2C")?;
                }
                ABList => {
                    reg(a, "A")?;
                    if c < 2 {
                        return Err(err(format!("fewer than 2 operands at {pc:04x}")));
                    }
                    reg(b + c - 1, "B+C-1")?;
                }
                ABsC | ABsCImm => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                }
                AsBsC => reg(a, "A")?,
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
                        op::CALL | op::CALL_METHOD => {
                            let callee = self.funcs.get(bx as usize).ok_or_else(|| {
                                err(format!("function {bx} out of range at {pc:04x}"))
                            })?;
                            if a + callee.params as u32 > regs {
                                return Err(err(format!("call arguments exceed regs at {pc:04x}")));
                            }
                        }
                        op::CALL_INTF => {
                            if bx as usize >= self.intf_calls.len() {
                                return Err(err(format!(
                                    "interface call target {bx} out of range at {pc:04x}"
                                )));
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
                    if opc == op::EXC_INFO && c > EXC_INFO_SUPPRESSED {
                        return Err(err(format!("information {c} out of range at {pc:04x}")));
                    }
                }
                ASlotC => {
                    reg(a, "A")?;
                    reg(c, "C")?;
                }
                ASlotCType => {
                    reg(a, "A")?;
                    reg(c, "C")?;
                    // The type word (1.2): the record or static array a reference slot points
                    // to (2.8)
                    let t = type_word_at(code, pc, len);
                    match self.type_entry(t) {
                        Some(e) if matches!(e.kind, TYPE_RECORD | TYPE_ARRAY) => {}
                        Some(_) => {
                            return Err(err(format!("type {t} has the wrong kind at {pc:04x}")));
                        }
                        Option::None => {
                            return Err(err(format!("type {t} out of range at {pc:04x}")));
                        }
                    }
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
                ABPath | ABPathType => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    if c == 0 {
                        return Err(err(format!("empty lending path at {pc:04x}")));
                    }
                    reg(a + c, "A+C")?;
                    if info.shape == ABPathType {
                        flat_type(type_word_at(code, pc, len))?;
                    }
                }
                // A cursor is two registers (2.8)
                ACursorBC => {
                    reg(a + 1, "A+1")?;
                    reg(b, "B")?;
                    reg(c, "C")?;
                }
                ABCursorSlot => {
                    reg(a, "A")?;
                    reg(b + 1, "B+1")?;
                }
                ACursorSlotC => {
                    reg(a + 1, "A+1")?;
                    reg(c, "C")?;
                }
                ABCursorType => {
                    reg(a, "A")?;
                    reg(b + 1, "B+1")?;
                    flat_type(type_word_at(code, pc, len))?;
                }
                ACursorBType => {
                    reg(a + 1, "A+1")?;
                    reg(b, "B")?;
                    flat_type(type_word_at(code, pc, len))?;
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
                op::NEW | op::IS_CLASS | op::AS_CLASS => &[TYPE_CLASS],
                _ => &[],
            };
            // `New` cannot make an instance of an abstract class (`docs/spec/objects.md` 14.2)
            if opc == op::NEW
                && self
                    .type_entry(bx)
                    .is_some_and(|t| t.flags & CLASS_ABSTRACT != 0)
            {
                return Err(err(format!("type {bx} is abstract at {pc:04x}")));
            }
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
            pc += len;
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
        let opcode_at =
            |i: usize| -> Option<u16> { Some(decode_at(code, *boundaries.get(i)? as usize).0) };
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
            let (opc, _, _, c, len) = decode_at(code, start as usize);
            let wide = is_wide_at(code, start as usize);
            let Some(info) = op::info(opc) else { continue };
            let next = start as usize + len;
            let offset: i64 = match info.shape {
                op::Shape::SBx | op::Shape::ASBx => signed_bx(bx_at(code, start as usize), wide),
                op::Shape::ABsC | op::Shape::AsBsC => signed_c(c, wide),
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
        self.verify_inlines(f, &boundaries).map_err(err)?;
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
        for m in &f.managed {
            let r = m.reg;
            if r as u32 >= f.regs as u32 {
                return Err(err("managed register out of range".into()));
            }
            if !seen.insert(r) {
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
                TYPE_ENUM => "enum",
                TYPE_SET => "set",
                TYPE_CLASS => "class",
                TYPE_INTERFACE => "interface",
                _ => "array",
            };
            let mut flags = String::new();
            for (bit, name) in [
                (CLASS_REF_COUNTED, " ref"),
                (CLASS_ABSTRACT, " abstract"),
                (CLASS_SEALED, " sealed"),
            ] {
                if t.flags & bit != 0 {
                    flags.push_str(name);
                }
            }
            let mut line = format!("  {index}: {kind} {}{flags} slots={}", t.name, t.slots);
            if t.kind == TYPE_INTERFACE {
                // `docs/spec/objects.md` 15.4: no slots
                line = format!(
                    "  {index}: {kind} {} parent=ty{} methods={}",
                    t.name,
                    t.parent.unwrap_or(0),
                    t.methods
                );
                if t.guid != [0; 16] {
                    line.push_str(&format!(" guid={}", guid_text(&t.guid)));
                }
            }
            match t.kind {
                TYPE_INTERFACE => {}
                TYPE_RECORD | TYPE_CLASS => {
                    for (kind, label) in [(SLOT_REF, "refs"), (SLOT_OBJ, "objs")] {
                        let slots: Vec<String> = t
                            .refs
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| **r == kind)
                            .map(|(i, _)| i.to_string())
                            .collect();
                        if !slots.is_empty() {
                            line.push_str(&format!(" {label}={}", slots.join(",")));
                        }
                    }
                    if let Some(p) = t.parent {
                        line.push_str(&format!(" parent=ty{p}"));
                    }
                    if t.kind == TYPE_CLASS {
                        let vmt: Vec<String> = t
                            .vmt
                            .iter()
                            .enumerate()
                            .map(|(k, e)| match *e {
                                VMT_ABSTRACT => {
                                    let name = t
                                        .abstract_names
                                        .iter()
                                        .find(|(i, _)| *i as usize == k)
                                        .map_or("", |(_, n)| n.as_str());
                                    format!("abstract:{name}")
                                }
                                e if e >= VMT_BUILTIN => format!("builtin{}", e - VMT_BUILTIN),
                                e => format!("f{e}"),
                            })
                            .collect();
                        line.push_str(&format!(" vmt={}", vmt.join(",")));
                        if !t.intfs.is_empty() {
                            let intfs: Vec<String> = t
                                .intfs
                                .iter()
                                .map(|e| {
                                    let funcs: Vec<String> =
                                        e.funcs.iter().map(|f| vmt_entry_text(*f)).collect();
                                    format!("ty{}:{}", e.intf, funcs.join(","))
                                })
                                .collect();
                            line.push_str(&format!(" intf={}", intfs.join(";")));
                        }
                    }
                }
                TYPE_ENUM => {
                    let values: Vec<String> =
                        t.values.iter().map(|(o, n)| format!("{o}:{n}")).collect();
                    line.push_str(&format!(" values={}", values.join(",")));
                }
                TYPE_SET => {}
                _ => {
                    // The kind of each slot of an element; its width only when an element takes
                    // more than one slot (a flat record or static array; 3.2)
                    let kinds: Vec<&str> = (0..t.elem_width())
                        .map(|i| match t.slot_kind(i) {
                            SLOT_REF => "ref",
                            SLOT_OBJ => "obj",
                            _ => "val",
                        })
                        .collect();
                    if t.elem_width() > 1 {
                        line.push_str(&format!(" width={}", t.elem_width()));
                    }
                    line.push_str(&format!(" elem={}", kinds.join(",")));
                }
            }
            out.push_str(&line);
            out.push('\n');
        }
    }
    if !m.intf_calls.is_empty() {
        out.push_str(".intfcall\n");
        for (i, (intf, k)) in m.intf_calls.iter().enumerate() {
            out.push_str(&format!("  {i}: ty{intf} #{k}\n"));
        }
    }
    out.push_str(".global\n");
    for (i, g) in m.globals.iter().enumerate() {
        let kind = match g.kind {
            GLOBAL_INT => "int",
            GLOBAL_EXIT_CODE => "exitcode",
            GLOBAL_OBJ => "obj",
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
        // `docs/spec/generics.md` 8
        if !f.shared_by.is_empty() {
            out.push_str(&format!("; shared by {}\n", f.shared_by.join(", ")));
        }
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
        // The entries of the inline table entered and not left yet, innermost last
        let mut open: Vec<usize> = Vec::new();
        let mut next_inline = 0usize;
        while pc < f.code.len() {
            inline_marks(m, f, pc, &mut open, &mut next_inline, &mut out);
            while li < f.lines.len() && f.lines[li].0 as usize <= pc {
                out.push_str(&format!("  .line {}\n", f.lines[li].1));
                li += 1;
            }
            let (text, len) = disassemble_one(&f.code, pc);
            out.push_str(&format!("  {pc:04x}  {text}\n"));
            pc += len;
        }
        inline_marks(m, f, pc, &mut open, &mut next_inline, &mut out);
        if !f.managed.is_empty() {
            out.push_str(&format!(".finally {}\n", managed_text(&f.managed)));
        }
    }
    out
}

/// The lines of the inline table (5) at `pc`: `.endinline` for the entries ending there
/// (innermost first), then `.inline` for those starting there (outermost first).
fn inline_marks(
    m: &Module,
    f: &Function,
    pc: usize,
    open: &mut Vec<usize>,
    next: &mut usize,
    out: &mut String,
) {
    while let Some(&k) = open.last() {
        if f.inlines[k].end as usize > pc {
            break;
        }
        open.pop();
        out.push_str("  .endinline\n");
    }
    while let Some(e) = f.inlines.get(*next).filter(|e| e.start as usize == pc) {
        let name = m
            .funcs
            .get(e.func as usize)
            .map_or("?", |g| g.name.as_str());
        out.push_str(&format!("  .inline f{} {name} line={}\n", e.func, e.line));
        open.push(*next);
        *next += 1;
    }
}

/// Managed registers as the disassembly shows them (`.finally`; 5): `r3, r4:obj`.
fn managed_text(regs: &[ManagedReg]) -> String {
    let regs: Vec<String> = regs
        .iter()
        .map(|m| {
            let obj = if m.kind == MANAGED_OBJ { ":obj" } else { "" };
            format!("r{}{obj}", m.reg)
        })
        .collect();
    regs.join(", ")
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
    let (opc, a, b, c, len) = decode_at(code, pc);
    let wide = is_wide_at(code, pc);
    let next = pc + len;
    // An intrinsic is shown by its own name and shape (2.19)
    let Some(info) = op::resolve(opc, c) else {
        return (format!("<unknown {opc} {c}>"), len);
    };
    let name = if wide {
        format!("w.{}", info.name)
    } else {
        info.name.to_string()
    };
    let bx = bx_at(code, pc);
    let sbx = signed_bx(bx, wide);
    let sc = signed_c(c, wide);
    use op::Shape::*;
    let operands = match info.shape {
        Intrinsic => unreachable!("`resolve` gives an intrinsic's own shape"),
        None => String::new(),
        A => format!("r{a}"),
        AB | ABRange2 | ABRange3 | ABRange4 | ABRange5 => format!("r{a}, r{b}"),
        ABRange2Slot => format!("r{a}, r{b}, {c}"),
        ABPairs | ABList => format!("r{a}, r{b}, {c}"),
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
                op::CALL | op::CALL_METHOD => "f",
                op::CALL_HOST => "h",
                op::CALL_INTF => "ic",
                op::UNIQUE
                | op::SET_LEN
                | op::NEW_ARR
                | op::ARR_COPY
                | op::ARR_CONCAT
                | op::ARR_INSERT
                | op::ARR_INSERT_ARR
                | op::ARR_DELETE
                | op::MATERIALIZE
                | op::NEW
                | op::IS_CLASS
                | op::AS_CLASS => "ty",
                _ => "",
            };
            format!("r{a}, {prefix}{bx}")
        }
        ABSlot | ABPath => format!("r{a}, r{b}, {c}"),
        ASlotC => format!("r{a}, {b}, r{c}"),
        ASlotCType => format!("r{a}, {b}, r{c}, ty{}", type_word_at(code, pc, len)),
        ABPathType => format!("r{a}, r{b}, {c}, ty{}", type_word_at(code, pc, len)),
        ACursorBC => format!("r{a}, r{b}, r{c}"),
        ABCursorSlot => format!("r{a}, r{b}, {c}"),
        ACursorSlotC => format!("r{a}, {b}, r{c}"),
        ABCursorType | ACursorBType => {
            format!("r{a}, r{b}, ty{}", type_word_at(code, pc, len))
        }
        ABk => format!("r{a}, r{b}, k{c}"),
        ABType => format!("r{a}, r{b}, ty{c}"),
        ASBx => format!("r{a}, -> {}", fmt_target(next as i64 + sbx)),
        SBx => format!("-> {}", fmt_target(next as i64 + sbx)),
        ABsC => format!("r{a}, r{b}, -> {}", fmt_target(next as i64 + sc)),
        ABsCImm => format!("r{a}, r{b}, {sc}"),
        AsBsC => format!(
            "r{a}, {}, -> {}",
            signed_b(b, wide),
            fmt_target(next as i64 + sc)
        ),
        AJumps => format!("r{a}, {b}"),
        AOuter => format!("r{a}, {b}, {c}"),
    };
    if operands.is_empty() {
        (name, len)
    } else {
        (format!("{name:<12} {operands}"), len)
    }
}

/// An entry of an interface table in the disassembly (`docs/spec/objects.md` 15.4): `f3`,
/// `builtin1` or `abstract`.
fn vmt_entry_text(e: u32) -> String {
    match e {
        VMT_ABSTRACT => "abstract".into(),
        e if e >= VMT_BUILTIN => format!("builtin{}", e - VMT_BUILTIN),
        e => format!("f{e}"),
    }
}

/// A GUID as written (`{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, uppercase; `docs/spec/objects.md`
/// 14.7): the bytes are the hexadecimal digits in pairs, in written order.
pub fn guid_text(g: &[u8; 16]) -> String {
    let hex: String = g.iter().map(|b| format!("{b:02X}")).collect();
    format!(
        "{{{}-{}-{}-{}-{}}}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Module {
        Module {
            types: Vec::new(),
            intf_calls: Vec::new(),
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
                managed: vec![ManagedReg {
                    reg: 0,
                    kind: MANAGED_HEAP_REF,
                }],
                file: 0,
                handlers: Vec::new(),
                shared_by: Vec::new(),
                inlines: Vec::new(),
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

    // `set_bx_at` writes what `bx_at` reads, in the narrow and the `Wide` form, and leaves the
    // opcode and `A` alone (sharing renumbers calls in place; generics.md 7).
    #[test]
    fn set_bx_at_inverts_bx_at() {
        for (idx, (code, bx)) in [
            (vec![encode(op::CALL, 7, 0x34, 0x12)], 0xBEEF),
            (
                vec![
                    encode(op::WIDE, 0, 0, 0),
                    encode(op::CALL, 7, 0x34, 0x12),
                    encode_ext(1, 0x5600, 0x7800),
                ],
                0x0102_0304,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let mut code = code;
            set_bx_at(&mut code, 0, bx);
            assert_eq!(bx_at(&code, 0), bx, "{idx}");
            let (opc, a, _, _, _) = decode_at(&code, 0);
            assert_eq!((opc, a & 0xFF), (op::CALL, 7), "{idx}");
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

    // Before 1.0 the header's format version field is fixed at 0 and any other
    // value is rejected (3.1, 3.3).
    #[test]
    fn format_version_fixed_at_zero() {
        let mut bytes = sample().encode("0.0.0");
        assert_eq!(&bytes[4..8], &[0, 0, 0, 0]);
        bytes[4] = 7;
        assert!(matches!(
            Module::decode(&bytes, "0.0.0"),
            Err(LoadError::Invalid(_))
        ));
    }

    #[test]
    fn disassembly_format() {
        let d = disassemble(&sample());
        let expected = "\
; puyc format 0
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
            refs: vec![SLOT_PLAIN, SLOT_REF],
            width: 0,
            name: "Hello.TPoint".into(),
            parent: None,
            values: Vec::new(),
            flags: 0,
            vmt: Vec::new(),
            abstract_names: Vec::new(),
            intfs: Vec::new(),
            methods: 0,
            guid: [0; 16],
        });
        m
    }

    /// A module with an interface (type 64, one method) implemented by a reference counted class
    /// (type 65, function 0), and a `CallIntf` of the method (`docs/spec/objects.md` 14.7).
    fn with_interface() -> Module {
        let mut m = sample();
        m.types.push(TypeEntry {
            kind: TYPE_INTERFACE,
            name: "Hello.IRun".into(),
            parent: Some(IINTERFACE_TYPE),
            methods: 1,
            guid: [0xAB; 16],
            ..TypeEntry::default()
        });
        m.types.push(TypeEntry {
            kind: TYPE_CLASS,
            name: "Hello.TRun".into(),
            parent: Some(TREFOBJECT_TYPE),
            flags: CLASS_REF_COUNTED,
            vmt: TOBJECT_VMT.to_vec(),
            intfs: vec![
                IntfEntry {
                    intf: IINTERFACE_TYPE,
                    funcs: Vec::new(),
                },
                IntfEntry {
                    intf: 64,
                    funcs: vec![0],
                },
            ],
            ..TypeEntry::default()
        });
        m.intf_calls.push((64, 0));
        m.funcs[0].code[0] = encode(op::CALL_INTF, 1, 0, 0);
        m
    }

    #[test]
    // An interface entry, a class's interface table and the interface call target section are
    // written, read back and shown (`docs/spec/objects.md` 14.7, 15.4).
    fn interface_entries_roundtrip() {
        let m = with_interface();
        m.verify().expect("valid module");
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        let guid = "{ABABABAB-ABAB-ABAB-ABAB-ABABABABABAB}";
        let types = format!(
            ".type\n  64: interface Hello.IRun parent=ty24 methods=1 guid={guid}\n  65: class Hello.TRun ref slots=0 parent=ty20 vmt=builtin0,builtin0,builtin0,builtin1,builtin3,builtin4 intf=ty24:;ty64:f0\n.intfcall\n  0: ty64 #0\n.global\n"
        );
        assert!(d.contains(&types), "{d}");
        assert!(d.contains("  0000  CallIntf     r1, ic0\n"), "{d}");
    }

    #[test]
    // What the verifier refuses in interfaces, interface tables and interface call targets
    // (`docs/spec/objects.md` 14.7): each change breaks one rule of the valid module.
    fn verify_checks_interfaces() {
        type Change = fn(&mut Module);
        let cases: [(&str, Change); 12] = [
            ("parent not an interface", |m| {
                m.types[0].parent = Some(TOBJECT_TYPE)
            }),
            ("parent not earlier", |m| m.types[0].parent = Some(64)),
            ("fewer methods than the parent", |m| {
                m.types.push(TypeEntry {
                    kind: TYPE_INTERFACE,
                    name: "Hello.IEmpty".into(),
                    parent: Some(64),
                    methods: 0,
                    ..TypeEntry::default()
                })
            }),
            ("table entry not an interface", |m| {
                m.types[1].intfs[1].intf = TOBJECT_TYPE
            }),
            ("interface twice", |m| {
                let e = m.types[1].intfs[1].clone();
                m.types[1].intfs.push(e)
            }),
            ("wrong number of methods", |m| {
                m.types[1].intfs[1].funcs.push(0)
            }),
            ("function out of range", |m| {
                m.types[1].intfs[1].funcs[0] = 7
            }),
            ("abstract entry in a class that is not abstract", |m| {
                m.types[1].intfs[1].funcs[0] = VMT_ABSTRACT
            }),
            ("interface's parent missing", |m| {
                m.types[1].intfs.remove(0);
            }),
            ("parent's interface missing", |m| {
                let mut child = m.types[1].clone();
                child.parent = Some(65);
                child.intfs.truncate(1);
                m.types.push(child)
            }),
            ("call target method out of range", |m| m.intf_calls[0].1 = 1),
            ("CallIntf target out of range", |m| {
                m.funcs[0].code[0] = encode(op::CALL_INTF, 1, 1, 0)
            }),
        ];
        for (what, change) in cases {
            let mut m = with_interface();
            change(&mut m);
            assert!(m.verify().is_err(), "{what}");
        }
    }

    #[test]
    // The reserved values a virtual method table may hold (`docs/spec/objects.md` 14.2,
    // `docs/spec/equality.md` 8.2): the built-in bodies 0 to 4 (2 only in an exception class),
    // and nothing above 4.
    fn vmt_builtin_bodies_range() {
        let cases: [(u32, bool); 6] = [
            (VMT_BUILTIN, true),
            (VMT_BUILTIN + 1, true),
            (VMT_BUILTIN + 2, false),
            (VMT_BUILTIN + 3, true),
            (VMT_BUILTIN + 4, true),
            (VMT_BUILTIN + 5, false),
        ];
        for (entry, ok) in cases {
            let mut m = with_interface();
            m.types[1].vmt[5] = entry;
            assert_eq!(m.verify().is_ok(), ok, "{entry:#x}");
        }
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
    // `IsClass` / `AsClass` take a class as `Bx` (`docs/spec/objects.md` 14.2): `TObject` and
    // `Exception` (a class since M3-35) pass, while a record, `TArray<Exception>` and an unused
    // index are refused.
    fn class_check_operand_is_a_class() {
        for opc in [op::IS_CLASS, op::AS_CLASS] {
            for (ty, ok) in [
                (TOBJECT_TYPE, true),
                (0, true),
                (64, false),
                (12, false),
                (63, false),
            ] {
                let mut m = sample_with_type();
                m.funcs[0].code[0] = encode(opc, 1, ty, 0);
                assert_eq!(m.verify().is_ok(), ok, "{opc} ty{ty}");
            }
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

    /// A module whose `<main>` has `regs` registers and the code `code`, with a record type (64).
    fn with_type_and_code(regs: u16, code: Vec<u32>) -> Module {
        let mut m = sample_with_type();
        m.funcs[0].regs = regs;
        m.funcs[0].code = code;
        m.funcs[0].lines = vec![(0, 5)];
        m
    }

    #[test]
    // A type word is shown as the last operand `tyN` on the line of its instruction, which is 2
    // words long, or 4 with `Wide` (1.2, 5).
    fn ref_field_writes_disassemble_with_the_type_word() {
        let ret = encode(op::RET_VOID, 0, 0, 0);
        let short = with_type_and_code(3, vec![encode(op::REF_FIELD_SET, 0, 1, 2), 64, ret]);
        assert_eq!(short.verify(), Ok(()));
        assert_eq!(
            disassemble_one(&short.funcs[0].code, 0),
            ("RefFieldSet  r0, 1, r2, ty64".to_string(), 2)
        );
        let wide = with_type_and_code(
            300,
            vec![
                encode(op::WIDE, 0, 0, 0),
                encode(op::REF_FIELD_SET_REF, 0, 1, 299 & 0xFF),
                encode_ext(0, 1, 299),
                64,
                ret,
            ],
        );
        assert_eq!(wide.verify(), Ok(()));
        assert_eq!(
            disassemble_one(&wide.funcs[0].code, 0),
            ("w.RefFieldSetRef r0, 1, r299, ty64".to_string(), 4)
        );
    }

    #[test]
    // The verifier refuses a `RefFieldSet` / `RefFieldSetRef` without its type word, with a type
    // word that does not name a record of the type section, and a jump to the type word, which
    // belongs to the instruction (1.2, 3.3).
    fn verify_checks_the_type_word() {
        let ret = encode(op::RET_VOID, 0, 0, 0);
        let bad = [
            (
                "no type word",
                vec![ret, encode(op::REF_FIELD_SET, 0, 1, 2)],
                "missing type word at 0001",
            ),
            (
                "a type that is not a record",
                vec![encode(op::REF_FIELD_SET, 0, 1, 2), 0, ret],
                "type 0 has the wrong kind at 0000",
            ),
            (
                "a type outside the section",
                vec![encode(op::REF_FIELD_SET_REF, 0, 1, 2), 65, ret],
                "type 65 out of range at 0000",
            ),
            (
                "a jump to the type word",
                vec![
                    encode(op::JMP, 0, 1, 0),
                    encode(op::REF_FIELD_SET, 0, 1, 2),
                    64,
                    ret,
                ],
                "jump target 2 is not an instruction boundary at 0000",
            ),
        ];
        for (what, code, message) in bad {
            assert_eq!(
                with_type_and_code(3, code).verify(),
                Err(LoadError::Invalid(format!(
                    "function 0 (Hello.<main>): {message}"
                ))),
                "{what}"
            );
        }
    }

    /// [`with_type_and_code`] with two more types: 65 a dynamic array of `Hello.TPoint` laid out
    /// flat (width 2) and 66 a record of 17 slots, wider than a flat place.
    fn with_flat_types(regs: u16, code: Vec<u32>) -> Module {
        let mut m = with_type_and_code(regs, code);
        m.types.push(TypeEntry {
            kind: TYPE_DYNARRAY,
            refs: vec![SLOT_PLAIN, SLOT_REF],
            width: 2,
            name: "array of Hello.TPoint".into(),
            ..TypeEntry::default()
        });
        m.types.push(TypeEntry {
            kind: TYPE_RECORD,
            slots: 17,
            refs: vec![SLOT_PLAIN; 17],
            name: "Hello.TWide".into(),
            ..TypeEntry::default()
        });
        m
    }

    #[test]
    // An array entry of the type section has the width of its elements, 1 to 16, and that many
    // slot kinds: the module reads back the same, the dump shows a width above 1, and a width
    // out of the range is refused when the module is read (3.2, 5.2).
    fn array_types_carry_their_element_width() {
        let ret = encode(op::RET_VOID, 0, 0, 0);
        let m = with_flat_types(4, vec![ret]);
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        assert!(
            d.contains("  65: dynarray array of Hello.TPoint slots=0 width=2 elem=val,ref\n"),
            "{d}"
        );
        for w in [0, 17] {
            let mut bad = m.clone();
            bad.types[1].width = w;
            bad.types[1].refs = vec![SLOT_PLAIN; w as usize];
            assert_eq!(
                Module::decode(&bad.encode("0.0.0"), "0.0.0"),
                Err(LoadError::Invalid(format!(
                    "array element width {w} out of range"
                ))),
                "{w}"
            );
        }
    }

    #[test]
    // The instructions of flat places disassemble with their cursors as the first register and
    // their type words as `tyN` (2.8, 5.2).
    fn flat_place_instructions_disassemble() {
        let ret = encode(op::RET_VOID, 0, 0, 0);
        let code = vec![
            encode(op::ELEM_AT, 2, 0, 1),
            encode(op::SLOT_GET, 1, 2, 0),
            encode(op::SLOT_SET_REF, 2, 1, 1),
            encode(op::AGG_LOAD, 0, 2, 0),
            64,
            encode(op::AGG_STORE, 2, 0, 0),
            64,
            encode(op::LEND_PLACE, 0, 3, 1),
            64,
            ret,
        ];
        let m = with_flat_types(5, code);
        assert_eq!(m.verify(), Ok(()));
        let mut lines = Vec::new();
        let mut pc = 0;
        while pc < m.funcs[0].code.len() {
            let (text, len) = disassemble_one(&m.funcs[0].code, pc);
            lines.push(text);
            pc += len;
        }
        assert_eq!(
            lines,
            [
                "ElemAt       r2, r0, r1",
                "SlotGet      r1, r2, 0",
                "SlotSetRef   r2, 1, r1",
                "AggLoad      r0, r2, ty64",
                "AggStore     r2, r0, ty64",
                "LendPlace    r0, r3, 1, ty64",
                "RetVoid",
            ]
        );
    }

    #[test]
    // The verifier checks both registers of a cursor, and that the type word of `AggLoad`,
    // `AggStore` and `LendPlace` names a record or static array of 16 slots at most (2.8, 3.3).
    fn verify_checks_flat_place_instructions() {
        let ret = encode(op::RET_VOID, 0, 0, 0);
        let bad = [
            (
                "ElemAt writing past the registers",
                vec![encode(op::ELEM_AT, 3, 0, 1), ret],
                "register A+1=4 out of range at 0000",
            ),
            (
                "SlotGet reading past the registers",
                vec![encode(op::SLOT_GET, 0, 3, 0), ret],
                "register B+1=4 out of range at 0000",
            ),
            (
                "SlotSet writing past the registers",
                vec![encode(op::SLOT_SET, 3, 0, 1), ret],
                "register A+1=4 out of range at 0000",
            ),
            (
                "AggLoad of a dynamic array",
                vec![encode(op::AGG_LOAD, 0, 1, 0), 65, ret],
                "type 65 has the wrong kind at 0000",
            ),
            (
                "AggStore of a record of 17 slots",
                vec![encode(op::AGG_STORE, 2, 0, 0), 66, ret],
                "type 66 is too wide at 0000",
            ),
            (
                "LendPlace without a path",
                vec![encode(op::LEND_PLACE, 0, 1, 0), 64, ret],
                "empty lending path at 0000",
            ),
            (
                "LendPlace without its type word",
                vec![encode(op::LEND_PLACE, 0, 1, 1)],
                "missing type word at 0000",
            ),
        ];
        for (what, code, message) in bad {
            assert_eq!(
                with_flat_types(4, code).verify(),
                Err(LoadError::Invalid(format!(
                    "function 0 (Hello.<main>): {message}"
                ))),
                "{what}"
            );
        }
    }

    #[test]
    // A cursor whose second register is past r255 takes `Wide` even when its first one is r255
    // (1.2): the verifier refuses the short form, which the fast path would read out of its window
    // of 256 registers, and takes the wide form of the same instruction.
    fn verify_wants_wide_for_cursor_ending_past_r255() {
        let ret = encode(op::RET_VOID, 0, 0, 0);
        // (instruction, A, B, C, whether a type word follows)
        let cases = [
            (op::ELEM_AT, 255, 0, 1, false),
            (op::SLOT_GET, 0, 255, 0, false),
            (op::SLOT_SET, 255, 0, 1, false),
            (op::AGG_LOAD, 0, 255, 0, true),
            (op::AGG_STORE, 255, 0, 0, true),
        ];
        for (opc, a, b, c, typed) in cases {
            let name = op::info(opc).unwrap().name;
            let mut short = vec![encode(opc, a, b, c)];
            let mut wide = vec![
                encode(op::WIDE, 0, 0, 0),
                encode(opc, a, b, c),
                encode_ext(a, b, c),
            ];
            if typed {
                short.push(64);
                wide.push(64);
            }
            short.push(ret);
            wide.push(ret);
            assert_eq!(
                with_flat_types(257, short).verify(),
                Err(LoadError::Invalid(
                    "function 0 (Hello.<main>): cursor ending at r256 without `Wide` at 0000"
                        .into()
                )),
                "{name}"
            );
            assert_eq!(with_flat_types(257, wide).verify(), Ok(()), "{name}");
        }
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
                refs: vec![SLOT_REF],
                width: 1,
                name: "array of string".into(),
                parent: None,
                values: Vec::new(),
                flags: 0,
                vmt: Vec::new(),
                abstract_names: Vec::new(),
                intfs: Vec::new(),
                methods: 0,
                guid: [0; 16],
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

    /// `sample_with_type` plus an exception class (64 is the record) whose parent is `parent`: a
    /// reference counted class whose `ToString` is the built-in body 2 (`docs/spec/objects.md`
    /// 14.8).
    fn with_exception(parent: u32) -> Module {
        let mut m = sample_with_type();
        m.types.push(TypeEntry {
            kind: TYPE_CLASS,
            slots: 2,
            refs: vec![SLOT_REF, SLOT_REF],
            width: 0,
            name: "Hello.EParseError".into(),
            parent: Some(parent),
            values: Vec::new(),
            flags: CLASS_REF_COUNTED,
            vmt: EXCEPTION_VMT.to_vec(),
            abstract_names: Vec::new(),
            intfs: Vec::new(),
            methods: 0,
            guid: [0; 16],
        });
        m
    }

    #[test]
    // A declared exception class is a class entry (kind 7) whose parent is a built-in exception
    // class; it round trips and is shown like any class (`docs/spec/objects.md` 14.8, 15.4).
    fn exception_class_entry_roundtrip() {
        let m = with_exception(0);
        m.verify().expect("valid module");
        let bytes = m.encode("0.0.0");
        assert_eq!(Module::decode(&bytes, "0.0.0").as_ref(), Ok(&m));
        let d = disassemble(&m);
        assert!(
            d.contains(
                "  65: class Hello.EParseError ref slots=2 refs=0,1 parent=ty0 vmt=builtin0,builtin0,builtin0,builtin2,builtin3,builtin4\n"
            ),
            "{d}"
        );
    }

    #[test]
    // Type kind 4 (the exception classes of M2) is gone: a module with it does not load
    // (`docs/spec/objects.md` 14.8).
    fn type_kind_4_is_refused() {
        let mut m = sample_with_type();
        m.types[0].kind = 4;
        let bytes = m.encode("0.0.0");
        assert!(
            matches!(Module::decode(&bytes, "0.0.0"), Err(LoadError::Invalid(ref e)) if e.contains("unknown type kind 4")),
            "{:?}",
            Module::decode(&bytes, "0.0.0")
        );
    }

    /// `sample_with_type` with an enumeration at type index 65 whose ordinal 5 has two names.
    fn with_enum() -> Module {
        let mut m = sample_with_type();
        m.types.push(TypeEntry {
            kind: TYPE_ENUM,
            slots: 0,
            refs: Vec::new(),
            width: 0,
            name: "Hello.TCode".into(),
            parent: None,
            values: vec![(0, "Zero".into()), (5, "Five".into()), (5, "Again".into())],
            flags: 0,
            vmt: Vec::new(),
            abstract_names: Vec::new(),
            intfs: Vec::new(),
            methods: 0,
            guid: [0; 16],
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
            width: 0,
            name: "Hello.TSmallChars".into(),
            parent: None,
            values: Vec::new(),
            flags: 0,
            vmt: Vec::new(),
            abstract_names: Vec::new(),
            intfs: Vec::new(),
            methods: 0,
            guid: [0; 16],
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
    // An exception class's parent is a built-in exception class or an earlier class; `TObject`
    // is of the other kind (managed by hand), and the built-in body 2 (`Exception.ToString`) is
    // refused in a class that is not an exception (`docs/spec/objects.md` 14.2, 14.8).
    fn verify_checks_exception_parents() {
        for (parent, ok) in [
            (0, true),
            (11, true),
            (23, true),
            (12, false),
            (19, false),
            (20, false),
            (64, false),
            (65, false),
        ] {
            assert_eq!(
                with_exception(parent).verify().is_ok(),
                ok,
                "parent {parent}"
            );
        }
    }

    #[test]
    // `ExcInfo` reads the stack trace or `Suppressed` (`C` 0 or 1; the class name moved to the
    // `ClassName` intrinsic), and `New` makes an instance of a built-in exception class
    // (`docs/spec/objects.md` 14.8).
    fn verify_checks_exception_instructions() {
        let cases: [(&str, u32, bool); 6] = [
            ("ExcInfo StackTrace", encode(op::EXC_INFO, 0, 1, 0), true),
            ("ExcInfo Suppressed", encode(op::EXC_INFO, 0, 1, 1), true),
            ("ExcInfo ClassName", encode(op::EXC_INFO, 0, 1, 2), false),
            (
                "New of a built-in exception",
                encode(op::NEW, 0, 7, 0),
                true,
            ),
            (
                "New of a declared exception",
                encode(op::NEW, 0, 65, 0),
                true,
            ),
            ("New of TArray<Exception>", encode(op::NEW, 0, 12, 0), false),
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
        let cases: [(&str, Change, bool); 11] = [
            ("valid", |_| {}, true),
            (
                "an empty finally block (its handler range ends where it starts)",
                |m| {
                    let h = &mut m.funcs[0].handlers[0];
                    h.body_end = 2;
                    h.handler_start = 2;
                },
                true,
            ),
            (
                "an empty except block",
                |m| {
                    let h = &mut m.funcs[0].handlers[0];
                    h.kind = HANDLER_EXCEPT;
                    h.handler_start = 2;
                    h.body_end = 2;
                },
                false,
            ),
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

    /// `sample` with an inline table: words 1..3 stand for function 0's body called at line 5,
    /// and word 2 for a call inlined into it at line 9.
    fn with_inlines() -> Module {
        let mut m = sample();
        m.funcs[0].inlines = vec![
            InlineEntry {
                start: 1,
                end: 3,
                func: 0,
                line: 5,
                outer: None,
            },
            InlineEntry {
                start: 2,
                end: 3,
                func: 0,
                line: 9,
                outer: Some(0),
            },
        ];
        m
    }

    #[test]
    // The inline table (3.3): ranges of whole instructions inside the code, each inside its outer
    // entry, in order of start, nested or apart, naming functions of the table.
    fn verify_checks_inline_table() {
        type Change = fn(&mut Module);
        let cases: [(&str, Change, bool); 9] = [
            ("valid", |_| {}, true),
            ("empty range", |m| m.funcs[0].inlines[1].end = 2, false),
            ("past the code", |m| m.funcs[0].inlines[0].end = 5, false),
            (
                "may end at the end of the code",
                |m| m.funcs[0].inlines[0].end = 4,
                true,
            ),
            (
                "unknown function",
                |m| m.funcs[0].inlines[1].func = 1,
                false,
            ),
            (
                "outer after it",
                |m| m.funcs[0].inlines[0].outer = Some(1),
                false,
            ),
            (
                "outside its outer entry",
                |m| m.funcs[0].inlines[1].end = 4,
                false,
            ),
            (
                "overlapping without an outer entry",
                |m| m.funcs[0].inlines[1].outer = None,
                false,
            ),
            (
                "out of order",
                |m| {
                    let e = &mut m.funcs[0].inlines;
                    e[1].outer = None;
                    e[1].start = 0;
                    e[1].end = 1;
                },
                false,
            ),
        ];
        for (what, change, ok) in cases {
            let mut m = with_inlines();
            change(&mut m);
            assert_eq!(m.verify().is_ok(), ok, "{what}");
        }
    }

    #[test]
    // The inline table goes through the module file as it is (3.2).
    fn inline_table_round_trips() {
        let m = with_inlines();
        assert_eq!(Module::decode(&m.encode("test"), "test").unwrap(), m);
    }

    #[test]
    // The disassembly marks an inlined range with `.inline` before its first instruction and
    // `.endinline` after its last, innermost last in and first out (5).
    fn disassembly_marks_inlined_ranges() {
        let d = disassemble(&with_inlines());
        let code = &d[d.find(".func 0").expect("the function")..];
        assert_eq!(
            code,
            ".func 0 Hello.<main> params=0 regs=3\n  .local r1 I\n  .line 5\n  \
             0000  LoadInt      r1, 0\n  .inline f0 Hello.<main> line=5\n  \
             0001  LoadConst    r0, k0\n  .inline f0 Hello.<main> line=9\n  .line 6\n  \
             0002  CallHost     r0, h0\n  .endinline\n  .endinline\n  0003  RetVoid\n\
             .finally r0\n"
        );
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
    // `Lend A B C` reads its path from `A+1 ..= A+C`, which must be registers, and the path
    // is never empty (6.1).
    fn verify_checks_lending_path() {
        for (c, ok) in [(0, false), (1, true), (2, false)] {
            let mut m = sample();
            m.funcs[0].code[0] = encode(op::LEND, 1, 0, c);
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

    // `ConcatStrN` reads `C` registers from `B`: at least 2, all within the frame (2.4, 3.3).
    #[test]
    fn concat_str_n_checks_its_register_list() {
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::CONCAT_STR_N, 0, 0, 3);
        assert!(m.verify().is_ok());
        assert_eq!(
            disassemble_one(&m.funcs[0].code, 0).0,
            "ConcatStrN   r0, r0, 3"
        );
        for (b, c) in [(1, 3), (0, 4), (0, 1), (0, 0)] {
            m.funcs[0].code[0] = encode(op::CONCAT_STR_N, 0, b, c);
            assert!(m.verify().is_err(), "B={b} C={c}");
        }
    }

    // The instructions with an immediate (2.2, 2.3) check their registers but not the immediate
    // in the place of a register (`sC` of `MulIntI`, `sB` of `JmpIfLtIntI`), and the
    // compare-and-branch forms and `ForLoopUp` check their jump target like the other jumps (3.3)
    #[test]
    fn immediate_forms_check_registers_and_targets() {
        let mut m = sample();
        let ok = [
            encode(op::MUL_INT_I, 1, 2, 0x80),
            encode(op::DIV_INT_I, 0, 1, 0xFF),
            encode(op::JMP_IF_LT_INT_I, 2, 0xFB, 1),
            encode(op::FOR_LOOP_UP, 1, 2, 1),
            encode(op::FOR_LOOP_DOWN, 1, 2, 0xFF),
        ];
        for (i, word) in ok.into_iter().enumerate() {
            m.funcs[0].code[1] = word;
            assert!(m.verify().is_ok(), "{i}: {:?}", m.verify());
        }
        let bad = [
            encode(op::MUL_INT_I, 3, 0, 1),
            encode(op::SHR_INT32_I, 0, 3, 1),
            encode(op::JMP_IF_EQ_INT_I, 3, 0, 0),
            encode(op::JMP_IF_GE_INT_I, 0, 5, 2),
            encode(op::FOR_LOOP_UP, 0, 3, 0),
            encode(op::FOR_LOOP_DOWN, 0, 1, 0xFD),
        ];
        for (i, word) in bad.into_iter().enumerate() {
            m.funcs[0].code[1] = word;
            assert!(m.verify().is_err(), "{i}");
        }
    }

    // The hash instructions (224-228; `docs/spec/equality.md` 8.1) take registers of the function
    // in each operand, and show by their names (3.3, 5).
    #[test]
    fn hash_instructions_check_their_registers() {
        let mut m = sample();
        let ok = [
            (encode(op::HASH_INT, 0, 2, 0), "HashInt      r0, r2"),
            (encode(op::HASH_F64, 2, 1, 0), "HashF64      r2, r1"),
            (encode(op::HASH_STR, 1, 0, 0), "HashStr      r1, r0"),
            (encode(op::HASH_MIX, 0, 1, 2), "HashMix      r0, r1, r2"),
            (encode(op::KEY_EQ_F64, 2, 0, 1), "KeyEqF64     r2, r0, r1"),
        ];
        for (word, text) in ok {
            m.funcs[0].code[1] = word;
            assert!(m.verify().is_ok(), "{text}: {:?}", m.verify());
            assert_eq!(disassemble_one(&m.funcs[0].code, 1).0, text);
        }
        let bad = [
            encode(op::HASH_INT, 3, 0, 0),
            encode(op::HASH_INT, 0, 3, 0),
            encode(op::HASH_F64, 0, 3, 0),
            encode(op::HASH_STR, 3, 0, 0),
            encode(op::HASH_MIX, 0, 0, 3),
            encode(op::HASH_MIX, 0, 3, 0),
            encode(op::KEY_EQ_F64, 3, 0, 0),
            encode(op::KEY_EQ_F64, 0, 0, 3),
        ];
        for (i, word) in bad.into_iter().enumerate() {
            m.funcs[0].code[1] = word;
            assert!(m.verify().is_err(), "{i}");
        }
    }

    // `Intrinsic` is checked by the intrinsic's own shape and number, and shows by the
    // intrinsic's name (2.19, 3.3, 5).
    #[test]
    fn intrinsic_checked_and_shown_by_its_own_shape() {
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::INTRINSIC, 1, 0, u32::from(op::intr::ARG_GET));
        assert!(m.verify().is_ok());
        assert_eq!(
            disassemble_one(&m.funcs[0].code, 0).0,
            "ArgGet       r1, r0"
        );
        m.funcs[0].code[0] = encode(op::INTRINSIC, 0, 0, u32::from(op::intr::READ_LN_END));
        assert!(m.verify().is_ok());
        assert_eq!(disassemble_one(&m.funcs[0].code, 0).0, "ReadLnEnd");
        // `StrInsert` reads `B` and `B+1`; with B = 2, B+1 is past the 3 registers
        m.funcs[0].code[0] = encode(op::INTRINSIC, 0, 2, u32::from(op::intr::STR_INSERT));
        assert!(m.verify().is_err());
        // 97 is not given to an intrinsic yet (`docs/spec/bytecode.md` 2.19)
        m.funcs[0].code[0] = encode(op::INTRINSIC, 0, 0, 97);
        assert!(m.verify().is_err());
    }

    // The intrinsics only the standard library uses (`I73`-`I75`, `I96`; 2.22) take registers of
    // the function, `ArrayMoveRange`, `IsOverridden` and `ArraySortRange` also `B+1` and `B+2`,
    // and show by their names (3.3, 5).
    #[test]
    fn stdlib_intrinsics_check_their_registers() {
        let mut m = sample();
        let word = |a, b, id: u8| encode(op::INTRINSIC, a, b, u32::from(id));
        let ok = [
            (
                word(2, 0, op::intr::ARRAY_MOVE_RANGE),
                "ArrayMoveRange r2, r0",
            ),
            (word(0, 2, op::intr::IS_REF_OBJ), "IsRefObj     r0, r2"),
            (word(1, 0, op::intr::IS_OVERRIDDEN), "IsOverridden r1, r0"),
            (
                word(2, 0, op::intr::ARRAY_SORT_RANGE),
                "ArraySortRange r2, r0",
            ),
        ];
        for (w, text) in ok {
            m.funcs[0].code[0] = w;
            assert!(m.verify().is_ok(), "{text}: {:?}", m.verify());
            assert_eq!(disassemble_one(&m.funcs[0].code, 0).0, text);
        }
        let bad = [
            // `B+2` is past the 3 registers
            word(0, 1, op::intr::ARRAY_MOVE_RANGE),
            word(3, 0, op::intr::ARRAY_MOVE_RANGE),
            word(3, 0, op::intr::IS_REF_OBJ),
            word(0, 3, op::intr::IS_REF_OBJ),
            word(0, 1, op::intr::IS_OVERRIDDEN),
            word(3, 0, op::intr::IS_OVERRIDDEN),
            word(0, 1, op::intr::ARRAY_SORT_RANGE),
            word(3, 0, op::intr::ARRAY_SORT_RANGE),
        ];
        for (i, w) in bad.into_iter().enumerate() {
            m.funcs[0].code[0] = w;
            assert!(m.verify().is_err(), "{i}");
        }
    }

    // The hash table lookups (90, 96, 98, `I95`; 2.23) take the registers of their ranges,
    // `HashFind` a `C` of 0, `HashLookupInt` / `HashLookupStr` any slot number in `C` (checked
    // when they run, as `ObjGet`'s), and show by their names (3.3, 5).
    #[test]
    fn hash_table_instructions_check_their_operands() {
        let mut m = sample();
        m.funcs[0].regs = 7;
        let relink = |a, b| encode(op::INTRINSIC, a, b, u32::from(op::intr::HASH_RELINK));
        let ok = [
            (encode(op::HASH_FIND, 6, 2, 0), "HashFind     r6, r2"),
            (
                encode(op::HASH_LOOKUP_INT, 3, 0, 2),
                "HashLookupInt r3, r0, 2",
            ),
            (
                encode(op::HASH_LOOKUP_STR, 6, 5, 255),
                "HashLookupStr r6, r5, 255",
            ),
            (relink(6, 4), "HashRelink   r6, r4"),
        ];
        for (w, text) in ok {
            m.funcs[0].code[0] = w;
            assert!(m.verify().is_ok(), "{text}: {:?}", m.verify());
            assert_eq!(disassemble_one(&m.funcs[0].code, 0).0, text);
        }
        let bad = [
            // `B+4`, `B+1`, `B+2` past the 7 registers
            encode(op::HASH_FIND, 0, 3, 0),
            encode(op::HASH_LOOKUP_INT, 0, 6, 0),
            encode(op::HASH_LOOKUP_STR, 0, 6, 0),
            relink(0, 5),
            encode(op::HASH_FIND, 7, 0, 0),
            encode(op::HASH_LOOKUP_INT, 7, 0, 0),
            encode(op::HASH_LOOKUP_STR, 7, 0, 0),
            relink(7, 0),
            // `C`
            encode(op::HASH_FIND, 0, 0, 1),
            // 91 is free again (`HashFindKey` until M4-132)
            encode(91u8, 0, 0, 0),
        ];
        for (i, w) in bad.into_iter().enumerate() {
            m.funcs[0].code[0] = w;
            assert!(m.verify().is_err(), "{i}");
        }
    }

    // The intrinsics of the key equality and hash (`I68`-`I72`; 2.21) take registers of the
    // function, `CallableKeyEq` also `B+1`, and show by their names (3.3, 5).
    #[test]
    fn hash_intrinsics_check_their_registers() {
        let mut m = sample();
        let word = |a, b, id: u8| encode(op::INTRINSIC, a, b, u32::from(id));
        let ok = [
            (word(0, 2, op::intr::OBJ_ALIVE), "ObjAlive     r0, r2"),
            (word(2, 1, op::intr::HASH_UTF8), "HashUtf8     r2, r1"),
            (word(1, 0, op::intr::HASH_SET_BIG), "HashSetBig   r1, r0"),
            (
                word(2, 1, op::intr::CALLABLE_KEY_EQ),
                "CallableKeyEq r2, r1",
            ),
            (word(0, 2, op::intr::CALLABLE_HASH), "CallableHash r0, r2"),
        ];
        for (w, text) in ok {
            m.funcs[0].code[0] = w;
            assert!(m.verify().is_ok(), "{text}: {:?}", m.verify());
            assert_eq!(disassemble_one(&m.funcs[0].code, 0).0, text);
        }
        let bad = [
            word(3, 0, op::intr::OBJ_ALIVE),
            word(0, 3, op::intr::OBJ_ALIVE),
            word(0, 3, op::intr::HASH_UTF8),
            word(3, 0, op::intr::HASH_SET_BIG),
            // `B+1` is past the 3 registers
            word(0, 2, op::intr::CALLABLE_KEY_EQ),
            word(0, 3, op::intr::CALLABLE_HASH),
        ];
        for (i, w) in bad.into_iter().enumerate() {
            m.funcs[0].code[0] = w;
            assert!(m.verify().is_err(), "{i}");
        }
    }

    // The managed register list must name registers of the function, each once (3.3).
    #[test]
    fn verify_rejects_bad_managed_registers() {
        let regs = |rs: &[u16]| -> Vec<ManagedReg> {
            rs.iter()
                .map(|&reg| ManagedReg {
                    reg,
                    kind: MANAGED_HEAP_REF,
                })
                .collect()
        };
        for (idx, managed) in [vec![3], vec![0, 0], vec![1, 2, 1]].into_iter().enumerate() {
            let mut m = sample();
            m.funcs[0].managed = regs(&managed);
            assert!(m.verify().is_err(), "{idx}");
        }
        let mut m = sample();
        m.funcs[0].managed = regs(&[2, 0, 1]);
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
        bad[pos + 6] = 3;
        assert_eq!(
            Module::decode(&bad, "0.0.0"),
            Err(LoadError::Invalid("unknown managed register kind 3".into()))
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

    // A `Wide` must be followed by an instruction and its extension word, and not by another
    // `Wide`; the verifier reports these as load errors instead of reading past the code.
    #[test]
    fn wide_without_its_words_or_before_wide() {
        let truncated = "function 0 (Hello.<main>): `Wide` without a following instruction";
        let cases = [
            (vec![encode(op::WIDE, 0, 0, 0)], truncated),
            (
                vec![encode(op::WIDE, 0, 0, 0), encode(op::RET_VOID, 0, 0, 0)],
                truncated,
            ),
            (
                vec![
                    encode(op::WIDE, 0, 0, 0),
                    encode(op::WIDE, 0, 0, 0),
                    encode_ext(0, 0, 0),
                    encode(op::RET_VOID, 0, 0, 0),
                ],
                "function 0 (Hello.<main>): `Wide` after `Wide`",
            ),
        ];
        for (idx, (code, message)) in cases.into_iter().enumerate() {
            let mut m = sample();
            m.funcs[0].code = code;
            m.funcs[0].lines = vec![(0, 1)];
            assert_eq!(m.verify(), Err(LoadError::Invalid(message.into())), "{idx}");
        }
    }

    // An instruction numbered past 255 is its `op` after the prefix `Ext` (1.2): `Ext` adds a
    // word, comes after `Wide`, and the instruction verifies and shows by its own number and name.
    #[test]
    fn ext_instruction_decodes_verifies_and_disassembles() {
        let ext = 0x0000_00FE;
        let ret_void = 0x0000_0073;
        let mut m = sample();
        // `Ext`, then `TestExt r0, r1, r2` (its word holds the `op` of `AddInt`, 16)
        m.funcs[0].code = vec![ext, 0x0201_0010, ret_void];
        m.funcs[0].lines = vec![(0, 1)];
        assert_eq!(decode_at(&m.funcs[0].code, 0), (op::TEST_EXT, 0, 1, 2, 2));
        assert_eq!(m.verify(), Ok(()));
        assert_eq!(
            disassemble_one(&m.funcs[0].code, 0),
            ("TestExt      r0, r1, r2".to_string(), 2)
        );
        // `Wide`, `Ext`, then `TestExt r300, r1, r2` and the extension word
        m.funcs[0].code = vec![0x0000_0001, ext, 0x0201_2C10, 0x0000_0100, ret_void];
        m.funcs[0].regs = 301;
        assert_eq!(decode_at(&m.funcs[0].code, 0), (op::TEST_EXT, 300, 1, 2, 4));
        assert_eq!(m.verify(), Ok(()));
        assert_eq!(
            disassemble_one(&m.funcs[0].code, 0),
            ("w.TestExt    r300, r1, r2".to_string(), 4)
        );
        // The words after the prefixes are not instruction boundaries: `Jmp +1` lands on the word
        // of `TestExt`
        m.funcs[0].code = vec![encode(op::JMP, 0, 1, 0), ext, 0x0201_0010, ret_void];
        m.funcs[0].regs = 3;
        assert_eq!(
            m.verify(),
            Err(LoadError::Invalid(
                "function 0 (Hello.<main>): jump target 2 is not an instruction boundary at 0000"
                    .into()
            ))
        );
    }

    // The prefixes come in the order `Wide`, `Ext`, each once, and are followed by their words;
    // `Ext` before a number no instruction has is an unknown opcode (1.2, 3.3).
    #[test]
    fn ext_prefix_errors() {
        let (ext, wide, ret_void) = (0x0000_00FE, 0x0000_0001, 0x0000_0073);
        let cases = [
            (vec![ext], "`Ext` without a following instruction"),
            (vec![ext, ext, ret_void], "`Ext` after `Ext`"),
            (
                vec![ext, wide, 0x0201_0010, 0, ret_void],
                "`Wide` after `Ext`",
            ),
            (vec![wide, ext], "`Ext` without a following instruction"),
            (
                vec![wide, ext, 0x0201_0010],
                "`Wide` without a following instruction",
            ),
            // `Ext` with the `op` of `Nop` is 256, and with that of `Intrinsic` 511
            (
                vec![ext, 0x0000_0000, ret_void],
                "unknown opcode 256 at 0000",
            ),
            (
                vec![ext, 0x0000_00FF, ret_void],
                "unknown opcode 511 at 0000",
            ),
        ];
        for (idx, (code, message)) in cases.into_iter().enumerate() {
            let mut m = sample();
            m.funcs[0].code = code;
            m.funcs[0].lines = vec![(0, 1)];
            assert_eq!(
                m.verify(),
                Err(LoadError::Invalid(format!(
                    "function 0 (Hello.<main>): {message}"
                ))),
                "{idx}"
            );
        }
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
