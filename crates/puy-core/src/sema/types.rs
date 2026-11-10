//! Types and constant values (`docs/spec/semantics.md` chapter 2, `docs/spec/types.md`).

use crate::source::TextRange;

/// Index of a declared or anonymous type in the [`TypeTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// Types. Aliases (`LongInt`, `NativeInt`, `type X = Y`) resolve to the same type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ty {
    ShortInt,
    Byte,
    SmallInt,
    Word,
    Integer,
    Cardinal,
    Int64,
    UInt64,
    Boolean,
    Char,
    String,
    /// A `UTF8String` (`docs/spec/types.md` 8.5): a heap reference to valid UTF-8 bytes (managed).
    Utf8String,
    /// IEEE 754 binary32 (`docs/spec/types.md` 8.4). The slot holds the `f32` bits.
    Single,
    /// IEEE 754 binary64 (`Extended` and `Real` are aliases). The slot holds the `f64` bits.
    Double,
    /// A fixed-point number: the value times 10000 in an `i64` slot (`docs/spec/types.md` 8.4).
    Currency,
    /// A record (`docs/spec/types.md` 2.1). The same declaration is the same type.
    Record(TypeId),
    /// A static array (`docs/spec/types.md` 2.2).
    Array(TypeId),
    /// A dynamic array (`docs/spec/types.md` 2.3). The table holds one entry per element type, so
    /// the same element type is the same type (1.2).
    DynArray(TypeId),
    /// An open array parameter (`docs/spec/types.md` 2.4). The id is the dynamic array of the same
    /// element type.
    OpenArray(TypeId),
    /// An exception class (`docs/spec/exceptions.md` chapter 1): a reference to a heap block, `nil`
    /// by default. The built-in classes are the first entries of the table (their id is the type
    /// index the VM knows; [`TypeTable::new`]).
    Exception(TypeId),
    /// An enumeration type (`docs/spec/types.md` 8.1): the ordinal in an `i64` slot.
    Enum(TypeId),
    /// A subrange type (`docs/spec/types.md` 8.2). A value read from it has the base type; the
    /// subrange matters when a value is stored (the range check) and for `var` parameters.
    Subrange(TypeId),
    /// A set whose base type's largest ordinal is at most 63 (`docs/spec/types.md` 8.3): the
    /// elements as the bits of an `i64` slot. The table holds one entry per base type.
    Set(TypeId),
    /// A set with a larger base type: a copy-on-write heap block of bits (managed).
    BigSet(TypeId),
    /// An expression whose type could not be determined (e.g. one that already got a diagnostic). Passes every check.
    Error,
}

impl Ty {
    pub fn is_integer(self) -> bool {
        matches!(
            self,
            Ty::ShortInt
                | Ty::Byte
                | Ty::SmallInt
                | Ty::Word
                | Ty::Integer
                | Ty::Cardinal
                | Ty::Int64
                | Ty::UInt64
        )
    }

    pub fn is_signed(self) -> bool {
        matches!(self, Ty::ShortInt | Ty::SmallInt | Ty::Integer | Ty::Int64)
    }

    /// Types whose range fits in `Integer`.
    pub fn fits_in_integer(self) -> bool {
        matches!(
            self,
            Ty::ShortInt | Ty::Byte | Ty::SmallInt | Ty::Word | Ty::Integer
        )
    }

    /// Ordinal types: integers, `Boolean`, `Char`, enumerations and subranges.
    pub fn is_ordinal(self) -> bool {
        self.is_integer() || matches!(self, Ty::Boolean | Ty::Char | Ty::Enum(_) | Ty::Subrange(_))
    }

    pub fn is_error(self) -> bool {
        self == Ty::Error
    }

    /// `Single` and `Double` (`docs/spec/types.md` 8.4).
    pub fn is_real(self) -> bool {
        matches!(self, Ty::Single | Ty::Double)
    }

    /// Integers, reals and `Currency`: the types of arithmetic.
    pub fn is_number(self) -> bool {
        self.is_integer() || self.is_real() || self == Ty::Currency
    }

    /// Managed types: values that are heap references with a reference count (`docs/design/lifetime.md` 3.1).
    pub fn is_managed(self) -> bool {
        matches!(
            self,
            Ty::String
                | Ty::Utf8String
                | Ty::Record(_)
                | Ty::Array(_)
                | Ty::DynArray(_)
                | Ty::OpenArray(_)
                | Ty::Exception(_)
                | Ty::BigSet(_)
        )
    }

    /// Set types (small or big; `docs/spec/types.md` 8.3).
    pub fn is_set(self) -> bool {
        matches!(self, Ty::Set(_) | Ty::BigSet(_))
    }

    /// Dynamic arrays and open array parameters: indexed from 0, elements shared by reference
    /// (an open array parameter may hold a static array block, made unique before writing).
    pub fn is_dyn_indexed(self) -> bool {
        matches!(self, Ty::DynArray(_) | Ty::OpenArray(_))
    }

    /// Records and static arrays: values held in a heap block that is copied on write (`docs/design/lifetime.md` 3.6).
    pub fn is_aggregate(self) -> bool {
        matches!(self, Ty::Record(_) | Ty::Array(_))
    }

    /// The type table entry of a record or a static array.
    pub fn type_id(self) -> Option<TypeId> {
        match self {
            Ty::Record(id)
            | Ty::Array(id)
            | Ty::DynArray(id)
            | Ty::OpenArray(id)
            | Ty::Exception(id)
            | Ty::Enum(id)
            | Ty::Subrange(id)
            | Ty::Set(id)
            | Ty::BigSet(id) => Some(id),
            _ => None,
        }
    }

    /// Name of a type of the language itself (for diagnostics; `string` is lowercase). The name of
    /// a declared type comes from [`TypeTable::name`].
    fn builtin_name(self) -> &'static str {
        match self {
            Ty::ShortInt => "ShortInt",
            Ty::Byte => "Byte",
            Ty::SmallInt => "SmallInt",
            Ty::Word => "Word",
            Ty::Integer => "Integer",
            Ty::Cardinal => "Cardinal",
            Ty::Int64 => "Int64",
            Ty::UInt64 => "UInt64",
            Ty::Boolean => "Boolean",
            Ty::Char => "Char",
            Ty::String => "string",
            Ty::Utf8String => "UTF8String",
            Ty::Single => "Single",
            Ty::Double => "Double",
            Ty::Currency => "Currency",
            Ty::Record(_)
            | Ty::Array(_)
            | Ty::DynArray(_)
            | Ty::OpenArray(_)
            | Ty::Exception(_)
            | Ty::Enum(_)
            | Ty::Subrange(_)
            | Ty::Set(_)
            | Ty::BigSet(_) => "<type>",
            Ty::Error => "<error>",
        }
    }

    /// Range of an integer type (`Low` .. `High`).
    pub fn range(self) -> Option<(i128, i128)> {
        Some(match self {
            Ty::ShortInt => (i8::MIN as i128, i8::MAX as i128),
            Ty::Byte => (0, u8::MAX as i128),
            Ty::SmallInt => (i16::MIN as i128, i16::MAX as i128),
            Ty::Word => (0, u16::MAX as i128),
            Ty::Integer => (i32::MIN as i128, i32::MAX as i128),
            Ty::Cardinal => (0, u32::MAX as i128),
            Ty::Int64 => (i64::MIN as i128, i64::MAX as i128),
            Ty::UInt64 => (0, u64::MAX as i128),
            Ty::Char => (0, u16::MAX as i128),
            Ty::Boolean => (0, 1),
            _ => return None,
        })
    }

    pub fn contains(self, v: i128) -> bool {
        self.range().is_some_and(|(lo, hi)| lo <= v && v <= hi)
    }

    /// Width code for the `Narrow` family of instructions (`docs/spec/bytecode.md` 2.2).
    pub fn width_code(self) -> Option<u8> {
        Some(match self {
            Ty::ShortInt => 0,
            Ty::Byte => 1,
            Ty::SmallInt => 2,
            Ty::Word => 3,
            Ty::Integer => 4,
            Ty::Cardinal => 5,
            Ty::Int64 => 6,
            Ty::UInt64 => 7,
            Ty::Char => 8,
            _ => return None,
        })
    }

    /// Bit width of an integer type (including `Char`).
    pub fn bits(self) -> Option<u32> {
        match self {
            Ty::Char => Some(16),
            _ => self.width_code().map(|c| 8 << (c / 2)),
        }
    }

    /// Value truncated in two's complement.
    pub fn wrap(self, v: i128) -> i128 {
        match self {
            Ty::ShortInt => v as i8 as i128,
            Ty::Byte => v as u8 as i128,
            Ty::SmallInt => v as i16 as i128,
            Ty::Word | Ty::Char => v as u16 as i128,
            Ty::Integer => v as i32 as i128,
            Ty::Cardinal => v as u32 as i128,
            Ty::Int64 => v as i64 as i128,
            Ty::UInt64 => v as u64 as i128,
            _ => v,
        }
    }

    /// Looks up a type by name (case-insensitive), including aliases.
    pub fn from_name(lower: &str) -> Option<Ty> {
        Some(match lower {
            "shortint" => Ty::ShortInt,
            "byte" => Ty::Byte,
            "smallint" => Ty::SmallInt,
            "word" => Ty::Word,
            "integer" | "longint" => Ty::Integer,
            "cardinal" | "longword" | "ucs4char" => Ty::Cardinal,
            "int64" | "nativeint" => Ty::Int64,
            "uint64" | "nativeuint" => Ty::UInt64,
            "boolean" | "bytebool" | "wordbool" | "longbool" => Ty::Boolean,
            "char" | "widechar" => Ty::Char,
            "string" => Ty::String,
            "utf8string" => Ty::Utf8String,
            "single" => Ty::Single,
            "double" | "extended" | "real" => Ty::Double,
            "currency" => Ty::Currency,
            _ => return None,
        })
    }

    /// Type of an integer literal (2.2).
    pub fn of_literal(v: i128) -> Ty {
        if Ty::Integer.contains(v) {
            Ty::Integer
        } else if Ty::Int64.contains(v) {
            Ty::Int64
        } else {
            Ty::UInt64
        }
    }
}

/// A constant value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Const {
    /// A `Single` or `Double` as the bits of an `f64` (a `Single` holds a value rounded to
    /// binary32; `docs/spec/types.md` 8.4). Bits compare equal only for the same value and sign.
    Real(u64),
    /// A `Currency`: the value times 10000.
    Curr(i64),
    /// Integer (the type is carried by the expression).
    Int(i128),
    Bool(bool),
    /// A single UTF-16 code unit.
    Char(u16),
    /// A sequence of UTF-16 code units.
    Str(Vec<u16>),
    /// A `UTF8String` constant (`docs/spec/types.md` 8.5): valid UTF-8.
    U8(String),
    /// A record or static array constant (`docs/spec/types.md` chapter 4): the fields in declaration
    /// order, or the elements in index order, each converted to its type.
    Agg(Vec<Const>),
    /// `nil` (`docs/spec/types.md` 2.3): an empty dynamic array.
    Nil,
    /// A set (`docs/spec/types.md` 8.3): bit `i` of word `i / 64` is the ordinal `i`. Trailing zero
    /// words are dropped, so equal sets compare equal.
    Set(Vec<u64>),
}

/// Operations on the bit words of a set constant (`Const::Set`).
pub mod setbits {
    /// Drops the trailing zero words.
    pub fn trim(mut w: Vec<u64>) -> Vec<u64> {
        while w.last() == Some(&0) {
            w.pop();
        }
        w
    }

    /// Adds the ordinals `lo..=hi` (both within 0..65535).
    pub fn insert_range(w: &mut Vec<u64>, lo: u32, hi: u32) {
        if lo > hi {
            return;
        }
        let need = hi as usize / 64 + 1;
        if w.len() < need {
            w.resize(need, 0);
        }
        for i in lo..=hi {
            w[i as usize / 64] |= 1 << (i % 64);
        }
    }

    pub fn contains(w: &[u64], i: i128) -> bool {
        if i < 0 {
            return false;
        }
        let k = (i / 64) as usize;
        k < w.len() && w[k] & (1 << (i % 64)) != 0
    }

    pub fn zip(a: &[u64], b: &[u64], f: impl Fn(u64, u64) -> u64) -> Vec<u64> {
        let n = a.len().max(b.len());
        let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
        trim((0..n).map(|i| f(at(a, i), at(b, i))).collect())
    }

    /// The ordinals in ascending order.
    pub fn elements(w: &[u64]) -> impl Iterator<Item = i128> + '_ {
        w.iter().enumerate().flat_map(|(k, word)| {
            (0..64).filter_map(move |b| (word & (1 << b) != 0).then_some((k * 64 + b) as i128))
        })
    }

    /// Whether every element of `a` is in `b`.
    pub fn is_subset(a: &[u64], b: &[u64]) -> bool {
        zip(a, b, |x, y| x & !y).is_empty()
    }

    /// The bits as the `i64` of a small set.
    pub fn to_small(w: &[u64]) -> i64 {
        w.first().copied().unwrap_or(0) as i64
    }
}

impl Const {
    pub fn real(v: f64) -> Const {
        Const::Real(v.to_bits())
    }

    /// The value as an `f64` (a real, a `Currency` or an integer).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Const::Real(b) => Some(f64::from_bits(*b)),
            Const::Curr(c) => Some(crate::numfmt::curr_to_f64(*c)),
            Const::Int(v) => Some(crate::numfmt::int_to_f64(*v)),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i128> {
        match self {
            Const::Int(v) => Some(*v),
            Const::Char(c) => Some(*c as i128),
            Const::Bool(b) => Some(*b as i128),
            _ => None,
        }
    }

    /// The value viewed as a `string` (a `Char` has length 1).
    pub fn as_str(&self) -> Option<Vec<u16>> {
        match self {
            Const::Str(s) => Some(s.clone()),
            Const::U8(s) => Some(s.encode_utf16().collect()),
            Const::Char(c) => Some(vec![*c]),
            _ => None,
        }
    }

    /// Human-readable form (for the E0714 message).
    pub fn display(&self) -> String {
        match self {
            Const::Real(b) => crate::numfmt::f64_to_str(f64::from_bits(*b)),
            Const::Curr(c) => crate::numfmt::curr_to_str(*c),
            Const::Int(v) => v.to_string(),
            Const::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            Const::Char(c) => format!("#{c}"),
            Const::Str(s) => format!("'{}'", String::from_utf16_lossy(s)),
            Const::U8(s) => format!("'{s}'"),
            Const::Agg(items) => {
                let parts: Vec<String> = items.iter().map(Const::display).collect();
                format!("({})", parts.join(", "))
            }
            Const::Nil => "nil".to_string(),
            Const::Set(w) => {
                let parts: Vec<String> = setbits::elements(w).map(|v| v.to_string()).collect();
                format!("[{}]", parts.join(", "))
            }
        }
    }
}

/// Maximum string length (in code units; 3.5).
pub const MAX_STRING_LEN: usize = i32::MAX as usize;

/// A field of a record.
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub span: TextRange,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Record {
        fields: Vec<Field>,
    },
    /// `array[Low..High] of Elem`. `index` is the type of the index: `Integer` for an integer
    /// range, otherwise the ordinal type written or the type of the bounds (`Char`, `Boolean`, `Byte`).
    Array {
        index: Ty,
        low: i128,
        high: i128,
        elem: Ty,
    },
    /// `array of Elem` / `TArray<Elem>` (`generic`: first written as `TArray<…>`).
    DynArray {
        elem: Ty,
        generic: bool,
    },
    /// An exception class: its parent (`None` for the root `Exception`; `docs/spec/exceptions.md`
    /// chapters 2 and 3).
    Exception {
        parent: Option<TypeId>,
    },
    /// `(Red, Green, Blue)`: the values in declaration order, and the range (the smallest and
    /// largest ordinal; `docs/spec/types.md` 8.1).
    Enum {
        values: Vec<EnumValueDef>,
        low: i128,
        high: i128,
    },
    /// `L..H` of the ordinal type `base` (`docs/spec/types.md` 8.2).
    Subrange {
        base: Ty,
        low: i128,
        high: i128,
    },
    /// `set of base` (`docs/spec/types.md` 8.3). `high` is the largest ordinal of the base type,
    /// which decides the representation (`Ty::Set` up to 63, `Ty::BigSet` above).
    Set {
        base: Ty,
        low: i128,
        high: i128,
    },
    /// Declared but not yet resolved (a forward reference inside a `type` section), or failed.
    Pending,
}

/// A value of an enumeration type.
#[derive(Debug, Clone)]
pub struct EnumValueDef {
    /// The name as declared.
    pub name: String,
    pub ord: i128,
    pub span: TextRange,
}

#[derive(Debug, Clone)]
pub struct TypeDef {
    /// The declared name (`None` for an anonymous type).
    pub name: Option<String>,
    /// The fully qualified name of a declared type (`Hello.TPoint`; `docs/spec/bytecode.md` 3.2).
    pub full_name: Option<String>,
    pub kind: TypeKind,
    /// The declared name, or the `record` / `array` keyword of an anonymous type.
    pub span: TextRange,
}

/// The declared types of a program (`docs/spec/types.md` 1.2): one entry per declaration, so the
/// same entry is the same type. The first entries are the VM's built-in types ([`TypeTable::new`]).
#[derive(Debug, Clone)]
pub struct TypeTable {
    pub defs: Vec<TypeDef>,
}

/// The id of the root exception class `Exception`.
pub const EXCEPTION: TypeId = TypeId(crate::bytecode::exc::EXCEPTION);

impl Default for TypeTable {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeTable {
    /// A table holding the built-in exception classes and `TArray<Exception>` at the ids that
    /// are their type indexes in the bytecode (`docs/spec/exceptions.md` 6.1).
    pub fn new() -> Self {
        let mut t = TypeTable { defs: Vec::new() };
        let class = |name: &str, parent: Option<u32>| TypeDef {
            name: Some(name.to_string()),
            full_name: Some(name.to_string()),
            kind: TypeKind::Exception {
                parent: parent.map(TypeId),
            },
            span: TextRange::default(),
        };
        let exceptions = crate::bytecode::BUILTIN_EXCEPTIONS;
        for (index, name, parent, _) in exceptions {
            if *index >= crate::bytecode::EXCEPTION_ARRAY_TYPE {
                break;
            }
            t.defs.push(class(name, *parent));
        }
        let arr = t.dyn_array(
            Ty::Exception(EXCEPTION),
            DynWritten::Anonymous {
                generic: true,
                span: TextRange::default(),
            },
        );
        debug_assert_eq!(
            arr.type_id(),
            Some(TypeId(crate::bytecode::EXCEPTION_ARRAY_TYPE))
        );
        // The classes after `TArray<Exception>`, with an unused id where a class is not there yet
        while (t.defs.len() as u32) < crate::bytecode::BUILTIN_TYPE_END {
            let id = t.defs.len() as u32;
            let def = match exceptions.iter().find(|e| e.0 == id) {
                Some((_, name, parent, _)) => class(name, *parent),
                None => TypeDef {
                    name: None,
                    full_name: None,
                    kind: TypeKind::Pending,
                    span: TextRange::default(),
                },
            };
            t.defs.push(def);
        }
        t
    }

    /// Whether an id is one of the VM's built-in types (its id is its type index).
    pub fn is_builtin(id: TypeId) -> bool {
        id.0 < crate::bytecode::BUILTIN_TYPE_END
    }

    /// The parent of an exception class.
    pub fn parent(&self, id: TypeId) -> Option<TypeId> {
        match self.get(id).kind {
            TypeKind::Exception { parent } => parent,
            _ => None,
        }
    }

    /// Whether exception class `a` is `b` or a descendant of it. Stops on a cycle (E0727).
    pub fn is_descendant(&self, a: TypeId, b: TypeId) -> bool {
        let mut cur = Some(a);
        let mut steps = 0;
        while let Some(c) = cur {
            if c == b {
                return true;
            }
            steps += 1;
            if steps > self.defs.len() {
                return false;
            }
            cur = self.parent(c);
        }
        false
    }

    /// The number of slots of an exception class (`docs/spec/exceptions.md` 6.1): the class's
    /// own added slot, or the nearest ancestor's.
    pub fn exception_slots(&self, id: TypeId) -> Vec<bool> {
        let mut cur = Some(id);
        let mut steps = 0;
        while let Some(c) = cur {
            if TypeTable::is_builtin(c) {
                return crate::bytecode::builtin_types()[c.0 as usize].refs.clone();
            }
            steps += 1;
            if steps > self.defs.len() {
                break;
            }
            cur = self.parent(c);
        }
        vec![true, true]
    }

    pub fn add(&mut self, def: TypeDef) -> TypeId {
        self.defs.push(def);
        TypeId(self.defs.len() as u32 - 1)
    }

    /// The dynamic array of an element type. The first use of an element type creates the entry
    /// and gives it its name (`docs/spec/types.md` 1.2: the first written form).
    pub fn dyn_array(&mut self, elem: Ty, written: DynWritten) -> Ty {
        if let Some(i) = self
            .defs
            .iter()
            .position(|d| matches!(d.kind, TypeKind::DynArray { elem: e, .. } if e == elem))
        {
            return Ty::DynArray(TypeId(i as u32));
        }
        let (name, full_name, generic, span) = match written {
            DynWritten::Named {
                name,
                full_name,
                span,
            } => (Some(name), Some(full_name), false, span),
            DynWritten::Anonymous { generic, span } => (None, None, generic, span),
        };
        Ty::DynArray(self.add(TypeDef {
            name,
            full_name,
            kind: TypeKind::DynArray { elem, generic },
            span,
        }))
    }

    /// The set of a base type (whose ordinals are within 0..65535; the caller checks E0737). The
    /// first use of a base type creates the entry and gives it its name, like `dyn_array`.
    pub fn set_of(&mut self, base: Ty, written: DynWritten) -> Ty {
        let (low, high) = self.ord_range(base).unwrap_or((0, 0));
        let make = |id: TypeId| {
            if high > 63 {
                Ty::BigSet(id)
            } else {
                Ty::Set(id)
            }
        };
        if let Some(i) = self
            .defs
            .iter()
            .position(|d| matches!(d.kind, TypeKind::Set { base: b, .. } if b == base))
        {
            return make(TypeId(i as u32));
        }
        let (name, full_name, span) = match written {
            DynWritten::Named {
                name,
                full_name,
                span,
            } => (Some(name), Some(full_name), span),
            DynWritten::Anonymous { span, .. } => (None, None, span),
        };
        make(self.add(TypeDef {
            name,
            full_name,
            kind: TypeKind::Set { base, low, high },
            span,
        }))
    }

    /// The base type of a set.
    pub fn set_base(&self, ty: Ty) -> Option<Ty> {
        match ty {
            Ty::Set(id) | Ty::BigSet(id) => match &self.get(id).kind {
                TypeKind::Set { base, .. } => Some(*base),
                _ => None,
            },
            _ => None,
        }
    }

    /// The range of the elements of a set (the range of its base type).
    pub fn set_range(&self, ty: Ty) -> Option<(i128, i128)> {
        match ty {
            Ty::Set(id) | Ty::BigSet(id) => match &self.get(id).kind {
                TypeKind::Set { low, high, .. } => Some((*low, *high)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The number of bits of a big set's block (`docs/spec/bytecode.md` 3.2, type kind 5).
    pub fn set_bits(&self, ty: Ty) -> u32 {
        self.set_range(ty).map_or(0, |(_, hi)| (hi + 1) as u32)
    }

    /// The `` `TDigits` (0..9) `` part of a range message of a set's element (`docs/spec/types.md`
    /// 8.3): the set's name, and the range of its base type.
    pub fn set_range_tail(&self, ty: Ty) -> String {
        let base = self.set_base(ty).unwrap_or(Ty::Error);
        let (lo, hi) = self.set_range(ty).unwrap_or((0, 0));
        format!(
            "`{}` ({}..{})",
            self.name(ty),
            self.bound_text(base, lo),
            self.bound_text(base, hi)
        )
    }

    /// The element type of a dynamic array or an open array parameter.
    pub fn dyn_elem(&self, ty: Ty) -> Option<Ty> {
        match ty {
            Ty::DynArray(id) | Ty::OpenArray(id) => match &self.get(id).kind {
                TypeKind::DynArray { elem, .. } => Some(*elem),
                _ => None,
            },
            _ => None,
        }
    }

    /// The element type of any array (static, dynamic, open).
    pub fn elem_of(&self, ty: Ty) -> Option<Ty> {
        self.array(ty).map(|a| a.3).or_else(|| self.dyn_elem(ty))
    }

    pub fn get(&self, id: TypeId) -> &TypeDef {
        &self.defs[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: TypeId) -> &mut TypeDef {
        &mut self.defs[id.0 as usize]
    }

    /// The fields of a record (empty for anything else).
    pub fn fields(&self, ty: Ty) -> &[Field] {
        match ty {
            Ty::Record(id) => match &self.get(id).kind {
                TypeKind::Record { fields } => fields,
                _ => &[],
            },
            _ => &[],
        }
    }

    /// `(index type, low, high, element type)` of a static array.
    pub fn array(&self, ty: Ty) -> Option<(Ty, i128, i128, Ty)> {
        match ty {
            Ty::Array(id) => match &self.get(id).kind {
                TypeKind::Array {
                    index,
                    low,
                    high,
                    elem,
                } => Some((*index, *low, *high, *elem)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The base type of a subrange (any other type is its own base; `docs/spec/types.md` 8.2).
    pub fn base(&self, ty: Ty) -> Ty {
        match ty {
            Ty::Subrange(id) => match &self.get(id).kind {
                TypeKind::Subrange { base, .. } => *base,
                _ => Ty::Error,
            },
            t => t,
        }
    }

    /// The range of an ordinal type, including enumerations and subranges.
    pub fn ord_range(&self, ty: Ty) -> Option<(i128, i128)> {
        match ty {
            Ty::Enum(id) | Ty::Subrange(id) => match &self.get(id).kind {
                TypeKind::Enum { low, high, .. } | TypeKind::Subrange { low, high, .. } => {
                    Some((*low, *high))
                }
                _ => None,
            },
            t => t.range(),
        }
    }

    /// The values of an enumeration type (empty for anything else).
    pub fn enum_values(&self, ty: Ty) -> &[EnumValueDef] {
        match ty {
            Ty::Enum(id) => match &self.get(id).kind {
                TypeKind::Enum { values, .. } => values,
                _ => &[],
            },
            _ => &[],
        }
    }

    /// A bound of a subrange or an ordinal as written in a type name and in range messages: the
    /// ordinal of an enumeration in decimal, `#97` for a `Char`, `False` for a `Boolean`.
    pub fn bound_text(&self, ty: Ty, v: i128) -> String {
        bound_text(self.base(ty), v)
    }

    /// The `` `T` (L..H) `` part of a range message (`docs/spec/types.md` 8.1, 8.2).
    pub fn range_tail(&self, ty: Ty) -> String {
        let (lo, hi) = self.ord_range(ty).unwrap_or((0, 0));
        format!(
            "`{}` ({}..{})",
            self.name(ty),
            self.bound_text(ty, lo),
            self.bound_text(ty, hi)
        )
    }

    /// The number of slots of a record or static array (fields, or elements).
    pub fn slot_count(&self, ty: Ty) -> u64 {
        match ty {
            Ty::Record(_) => self.fields(ty).len() as u64,
            Ty::Array(_) => self
                .array(ty)
                .map_or(0, |(_, lo, hi, _)| (hi - lo + 1).max(0) as u64),
            _ => 0,
        }
    }

    /// The type name in diagnostics and dumps (`docs/spec/types.md` 1.2): the declared name, or
    /// `record` / `array[L..H] of T` for an anonymous type.
    pub fn name(&self, ty: Ty) -> String {
        self.name_with(ty, false)
    }

    /// The name in the bytecode type section, the `ir` dump and the leak report: declared types by
    /// their fully qualified name, anonymous types written out with qualified names inside.
    pub fn full_name(&self, ty: Ty) -> String {
        self.name_with(ty, true)
    }

    fn name_with(&self, ty: Ty, full: bool) -> String {
        let Some(id) = ty.type_id() else {
            return ty.builtin_name().to_string();
        };
        let def = self.get(id);
        if let (Ty::OpenArray(_), TypeKind::DynArray { elem, .. }) = (ty, &def.kind) {
            return format!("array of {}", self.name_with(*elem, full));
        }
        if let (true, Some(n)) = (full, &def.full_name) {
            return n.clone();
        }
        if let Some(n) = &def.name {
            return n.clone();
        }
        match &def.kind {
            TypeKind::Record { .. } | TypeKind::Pending => "record".to_string(),
            TypeKind::Exception { .. } => "class".to_string(),
            TypeKind::Enum { .. } => "enumeration".to_string(),
            TypeKind::Subrange { base, low, high } => {
                format!("{}..{}", bound_text(*base, *low), bound_text(*base, *high))
            }
            TypeKind::Set { base, .. } => format!("set of {}", self.name_with(*base, full)),
            TypeKind::Array {
                index,
                low,
                high,
                elem,
            } => format!(
                "array[{}..{}] of {}",
                bound_text(*index, *low),
                bound_text(*index, *high),
                self.name_with(*elem, full)
            ),
            TypeKind::DynArray { elem, generic } => {
                let e = self.name_with(*elem, full);
                if *generic {
                    format!("TArray<{e}>")
                } else {
                    format!("array of {e}")
                }
            }
        }
    }
}

/// How a dynamic array type is first written (its name; `TypeTable::dyn_array`).
pub enum DynWritten {
    /// `type TInts = array of Integer`.
    Named {
        name: String,
        full_name: String,
        span: TextRange,
    },
    /// `array of T` (or `TArray<T>` when `generic`).
    Anonymous { generic: bool, span: TextRange },
}

/// An array bound as written in a type name (`0`, `#97`, `False`).
pub fn bound_text(index: Ty, v: i128) -> String {
    match index {
        Ty::Char => Const::Char(v as u16).display(),
        Ty::Boolean => Const::Bool(v != 0).display(),
        _ => v.to_string(),
    }
}

/// Maximum number of elements of a static array (`docs/spec/types.md` 2.2).
pub const MAX_ARRAY_LEN: i128 = i32::MAX as i128;
