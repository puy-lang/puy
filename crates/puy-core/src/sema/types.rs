//! Types and constant values (`docs/spec/semantics.md` chapter 2, `docs/spec/types.md`).

use crate::sema::hir::GlobalId;
use crate::source::{FileId, TextRange};
use crate::syntax::ast::ParamMode;

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
    /// A class (`docs/spec/objects.md` 2.7): a handle to an instance, `nil` by default. `TObject`
    /// is the built-in class at its type index ([`TOBJECT`]). Instances of the classes M3-25
    /// implements are managed by hand (`Create` / `Free`), so the value is not counted.
    Class(TypeId),
    /// A class reference (`class of T`; `docs/spec/objects.md` 2.8): the class itself (the type
    /// index + 1 in a slot), `nil` by default. The table holds one entry per class, so the same
    /// class is the same type (named by its first written form, as dynamic arrays are). Not
    /// counted.
    ClassRef(TypeId),
    /// An interface (`docs/spec/objects.md` 7.4): the handle of the instance implementing it,
    /// `nil` by default, counted by its contents (as `TObject` is). `IInterface` is the built-in
    /// interface at its type index ([`IINTERFACE`]).
    Intf(TypeId),
    /// Where a `[weak]` field or variable keeps a reference to an instance of the class
    /// (`docs/spec/objects.md` 4.4): the handle, not counted. It is the type of the storage only:
    /// reading it gives a counted value of `Ty::Class` (`ExprKind::Weak`), and writing it takes
    /// one.
    Weak(TypeId),
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
    /// A procedural type (`procedure(…)` / `function(…): T`; `docs/spec/objects.md` 9.2): the
    /// function index + 1 in a slot, `nil` (0) by default. Not counted.
    Proc(TypeId),
    /// A method reference type (`… of object`) or an anonymous method type (`reference to …`;
    /// `docs/spec/objects.md` 9.3, 14.9): a reference to a callable value block, `nil` by
    /// default (managed).
    Callable(TypeId),
    /// A class helper or record helper (`docs/spec/objects.md` chapter 10): only its name has
    /// this type. It is not the type of a value (E0753); its name qualifies its class members
    /// and names it in `H.M(X)` (10.5).
    Helper(TypeId),
    /// A type parameter of a generic declaration (`docs/spec/generics.md` 2.2,
    /// `docs/design/generics.md` 3.1): seen only while the generic body is checked, never by
    /// lower.
    Param(TypeId),
    /// A distinct type (`type X = type Y`; `docs/spec/types.md` 1.3): the representation and
    /// the operations of its root type ([`TypeTable::root`]), another identity. Only the analysis
    /// sees it: the program lowered has the root types in its place (`sema::erase`).
    Distinct(TypeId),
    /// `PTypeInfo` (`docs/spec/rtti.md` 2.1): an opaque value naming an entry of the type
    /// information section (its type information number; 0 is `nil`). Not counted.
    TypeInfo,
    /// The managed part of a `TValue` (`docs/spec/rtti.md` 3.1): a heap reference of any kind (a
    /// string, an array, a box, an instance of a reference counted class, …), `nil` by default,
    /// counted. Only the part of `System` written in Pascal names it (`TValueRef`).
    ValueRef,
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
                | Ty::BigSet(_)
                | Ty::Callable(_)
                | Ty::ValueRef
        )
    }

    /// The procedural types of the three families (`docs/spec/objects.md` chapter 9).
    pub fn is_procedural(self) -> bool {
        matches!(self, Ty::Proc(_) | Ty::Callable(_))
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
            | Ty::Class(id)
            | Ty::ClassRef(id)
            | Ty::Intf(id)
            | Ty::Enum(id)
            | Ty::Subrange(id)
            | Ty::Set(id)
            | Ty::BigSet(id)
            | Ty::Proc(id)
            | Ty::Callable(id)
            | Ty::Helper(id)
            | Ty::Param(id)
            | Ty::Distinct(id) => Some(id),
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
            | Ty::Class(_)
            | Ty::ClassRef(_)
            | Ty::Intf(_)
            | Ty::Weak(_)
            | Ty::Enum(_)
            | Ty::Subrange(_)
            | Ty::Set(_)
            | Ty::BigSet(_)
            | Ty::Proc(_)
            | Ty::Callable(_)
            | Ty::Helper(_)
            | Ty::Param(_)
            | Ty::Distinct(_) => "<type>",
            Ty::TypeInfo => "PTypeInfo",
            Ty::ValueRef => "TValueRef",
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
            "double" | "extended" | "real" | "tdatetime" => Ty::Double,
            "currency" => Ty::Currency,
            "ptypeinfo" => Ty::TypeInfo,
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

/// How [`TypeTable`] writes a type name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameForm {
    /// As in diagnostics and dumps ([`TypeTable::name`]).
    Short,
    /// Declared types by their fully qualified name ([`TypeTable::full_name`]).
    Full,
    /// As `Short`, with each anonymous record marked ([`TypeTable::marked_name`]).
    Marked,
    /// As `Full`, with the types of `System` qualified with `System` ([`TypeTable::rtti_name`]).
    Rtti,
}

/// The characters around the type number in the mark of an anonymous record (private use code
/// points, which no type name holds).
const MARK_OPEN: char = '\u{E000}';
const MARK_CLOSE: char = '\u{E001}';

/// The name of an anonymous record in a diagnostic's text: `record` followed by a mark of its type,
/// taken out by [`take_anon_record_marks`].
fn mark_anon_record(id: TypeId) -> String {
    format!("record{MARK_OPEN}{}{MARK_CLOSE}", id.0)
}

/// Takes the marks of [`mark_anon_record`] out of `text`, adding the types they name to `found`
/// (in order, each once). Text that only looks like a mark is left as written.
pub fn take_anon_record_marks(text: &mut String, found: &mut Vec<TypeId>) {
    if !text.contains(MARK_OPEN) {
        return;
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find(MARK_OPEN) {
        out.push_str(&rest[..at]);
        let after = &rest[at + MARK_OPEN.len_utf8()..];
        let id = after
            .split_once(MARK_CLOSE)
            .and_then(|(digits, tail)| Some((digits.parse::<u32>().ok()?, tail)));
        match id {
            Some((n, tail)) => {
                if !found.contains(&TypeId(n)) {
                    found.push(TypeId(n));
                }
                rest = tail;
            }
            None => {
                out.push(MARK_OPEN);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    *text = out;
}

/// Maximum string length (in code units; 3.5).
pub const MAX_STRING_LEN: usize = i32::MAX as usize;

/// The most slots a record or static array laid out flat in its containers takes
/// (`docs/design/records.md` 3.2; the bytecode allows up to 16, `docs/spec/bytecode.md` 3.3).
pub const MAX_FLAT_WIDTH: u64 = 16;

/// A field of a record.
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub span: TextRange,
    /// The visibility section of the field (`docs/spec/objects.md` chapter 6).
    pub vis: Visibility,
}

/// The visibility of a class member (`docs/spec/objects.md` 2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    StrictPrivate,
    Private,
    StrictProtected,
    Protected,
    Public,
    Published,
}

impl Visibility {
    /// Whether the member is accessible from any scope.
    pub fn is_public(self) -> bool {
        matches!(self, Self::Public | Self::Published)
    }

    /// The ordering for override checks, where public and published grant the same access.
    pub(super) fn access_rank(self) -> u8 {
        match self {
            Self::StrictPrivate => 0,
            Self::Private => 1,
            Self::StrictProtected => 2,
            Self::Protected => 3,
            Self::Public | Self::Published => 4,
        }
    }

    /// The words of the section (for diagnostics).
    pub fn text(self) -> &'static str {
        match self {
            Visibility::StrictPrivate => "strict private",
            Visibility::Private => "private",
            Visibility::StrictProtected => "strict protected",
            Visibility::Protected => "protected",
            Visibility::Public => "public",
            Visibility::Published => "published",
        }
    }
}

/// A field of a class (`docs/spec/objects.md` 2.2).
#[derive(Debug, Clone)]
pub struct ClassField {
    pub name: String,
    pub ty: Ty,
    pub span: TextRange,
    pub vis: Visibility,
    /// The slot number: after the parent's fields, in declaration order.
    pub slot: u32,
}

/// What a method is (`docs/spec/objects.md` 2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Procedure,
    Function,
    Constructor,
    Destructor,
    /// `class operator` (`docs/spec/objects.md` chapter 11): a static function chosen only by
    /// the form of an expression, never by its name.
    Operator,
}

/// What `Self` of a method is (`docs/spec/objects.md` 2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    /// An instance method, constructor or destructor: the instance.
    Instance,
    /// A class method (`class procedure` / `class function`): the class reference.
    Class,
    /// A static method (`class …; static;`): no `Self`.
    Static,
}

/// What a member of a class that is not part of its instances is (`docs/spec/objects.md` 2.2).
#[derive(Debug, Clone)]
pub enum StaticKind {
    /// `class var`: a global variable of the class, shared with its descendants. In the type
    /// table of an IR module, a `class var` no kept function uses is
    /// [`crate::ir::reach::DROPPED`] (it is not a global variable of the module;
    /// `docs/spec/units.md` 6.2).
    Var(crate::sema::hir::GlobalId, Ty),
    /// A class constant (`const`).
    Const(Const, Ty),
    /// A nested type (`type`).
    Type(Ty),
}

/// A `class var`, class constant or nested type of a class (`docs/spec/objects.md` 2.2).
#[derive(Debug, Clone)]
pub struct ClassStatic {
    pub name: String,
    pub span: TextRange,
    pub vis: Visibility,
    pub kind: StaticKind,
}

/// Where a property reads or writes (`docs/spec/objects.md` 2.6).
#[derive(Debug, Clone, PartialEq)]
pub enum Accessor {
    /// A field of the instance: its slot.
    Field {
        slot: u32,
        name: String,
        /// The field's type (`Ty::Weak` for a `[weak]` field, whose reads and writes are those
        /// of the field).
        ty: Ty,
    },
    /// A `class var` (a `class property`): [`crate::ir::reach::DROPPED`] in an IR module when no
    /// kept function uses it (as [`StaticKind::Var`]).
    ClassVar(crate::sema::hir::GlobalId),
    /// A method: its routine (a static class method for a `class property`).
    Method(crate::sema::hir::RoutineId),
}

/// A property of a class (`docs/spec/objects.md` 2.6).
#[derive(Debug, Clone)]
pub struct ClassProperty {
    pub name: String,
    pub span: TextRange,
    pub vis: Visibility,
    pub ty: Ty,
    /// The types of the index parameters of an indexed property (`Items[I: Integer]`).
    pub params: Vec<Ty>,
    /// `index N`: passed to the accessor methods before the index parameters.
    pub index: Option<i128>,
    pub read: Option<Accessor>,
    pub write: Option<Accessor>,
    /// `class property`.
    pub is_class: bool,
    /// `; default;`: a default array property (`docs/spec/objects.md` 12.1).
    pub is_default: bool,
}

/// An entry of a class's virtual method table (`docs/spec/objects.md` 14.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmtEntry {
    /// A method with a body (the routine).
    Func(crate::sema::hir::RoutineId),
    /// A built-in body of the VM (`0xFFFFFF00 + k`: 0 does nothing, 1 is `TObject.ToString`, 2 is
    /// `Exception.ToString`, 3 is `TObject.Equals`, 4 is `TObject.GetHashCode`).
    Builtin(u32),
    /// An abstract method (`0xFFFFFFFF`).
    Abstract,
}

/// An entry of a class's interface table (`docs/spec/objects.md` 7.3, 14.7): the interface, and
/// the method implementing each of its methods, in method number order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntfImpl {
    pub intf: TypeId,
    pub entries: Vec<IntfEntry>,
}

/// The method implementing an interface's method (`docs/spec/objects.md` 7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntfEntry {
    /// A method that is not virtual: its routine.
    Func(crate::sema::hir::RoutineId),
    /// A virtual method: entry `K` of the virtual method table of the instance's class (so that
    /// an override is called; a descendant's table copies its own entry).
    Virtual(u32),
    /// No method implements it (after E0749; the program is not generated).
    Missing,
}

/// A method of a class (`docs/spec/objects.md` 2.3, 2.4).
#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub name: String,
    /// The routine of the method (its parameters after `Self`, and its body). `None` for the
    /// methods of the built-in classes, whose bodies the VM has.
    pub routine: Option<crate::sema::hir::RoutineId>,
    /// The name in the class declaration (`TextRange::default()` for a built-in method).
    pub span: TextRange,
    pub vis: Visibility,
    pub kind: MethodKind,
    /// The parameters' types and modes, and the return type.
    pub params: Vec<(Ty, crate::syntax::ast::ParamMode)>,
    pub ret: Option<Ty>,
    /// The number in the virtual method table of a virtual method (`virtual`, `dynamic` or
    /// `override`).
    pub vmt: Option<u32>,
    pub is_override: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    /// What `Self` is: an instance, a class reference (`class` methods) or nothing (`static`).
    pub self_kind: SelfKind,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    /// A record (`docs/spec/types.md` 2.1) and its members (`docs/spec/objects.md` chapter 6):
    /// the unit declaring it (for visibility), the methods, the `class var`s, constants and nested
    /// types, and the properties, in declaration order.
    Record {
        fields: Vec<Field>,
        unit: Option<u32>,
        methods: Vec<ClassMethod>,
        statics: Vec<ClassStatic>,
        properties: Vec<ClassProperty>,
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
    DynArray { elem: Ty, generic: bool },
    /// A class (`docs/spec/objects.md` chapter 2): its parent (`None` for `TObject`), its own
    /// fields, the unit declaring it (for visibility), and whether it is `TRefObject` or a
    /// descendant (a reference counted class).
    Class {
        parent: Option<TypeId>,
        fields: Vec<ClassField>,
        unit: Option<u32>,
        ref_counted: bool,
        /// The methods the class declares (`docs/spec/objects.md` 2.3), in declaration order.
        methods: Vec<ClassMethod>,
        /// The whole virtual method table: the parent's, overridden, then the class's new
        /// virtual methods (`docs/spec/objects.md` 2.4, 14.1).
        vmt: Vec<VmtEntry>,
        /// `class abstract` / `class sealed` (`docs/spec/objects.md` 2.4).
        is_abstract: bool,
        sealed: bool,
        /// The `class var`s, constants and nested types, in declaration order (2.2).
        statics: Vec<ClassStatic>,
        /// The properties the class declares, in declaration order (2.6).
        properties: Vec<ClassProperty>,
        /// The interface table: the interfaces the class implements, its parent's included
        /// (`docs/spec/objects.md` 7.3, 14.7).
        intfs: Vec<IntfImpl>,
    },
    /// An interface (`docs/spec/objects.md` 7.1): its parent (`None` for `IInterface`), the unit
    /// declaring it, its own methods (`ClassMethod::vmt` is the method number, after the
    /// parent's) and properties, the number of methods with the ancestors', and the GUID.
    Interface {
        parent: Option<TypeId>,
        unit: Option<u32>,
        methods: Vec<ClassMethod>,
        properties: Vec<ClassProperty>,
        count: u32,
        guid: Option<[u8; 16]>,
    },
    /// `class of T` (`docs/spec/objects.md` 2.8): the class the references point to (`T` or a
    /// descendant).
    ClassRef { class: TypeId },
    /// `(Red, Green, Blue)`: the values in declaration order, and the range (the smallest and
    /// largest ordinal; `docs/spec/types.md` 8.1).
    Enum {
        values: Vec<EnumValueDef>,
        low: i128,
        high: i128,
    },
    /// `L..H` of the ordinal type `base` (`docs/spec/types.md` 8.2).
    Subrange { base: Ty, low: i128, high: i128 },
    /// `set of base` (`docs/spec/types.md` 8.3). `high` is the largest ordinal of the base type,
    /// which decides the representation (`Ty::Set` up to 63, `Ty::BigSet` above).
    Set { base: Ty, low: i128, high: i128 },
    /// A procedural type of one of the three families (`docs/spec/objects.md` 9.1): the
    /// parameters (names for the dump) and the return type.
    Proc {
        family: ProcFamily,
        params: Vec<ProcParam>,
        ret: Option<Ty>,
    },
    /// A class helper or record helper (`docs/spec/objects.md` chapter 10): the type it is for
    /// (`Ty::Error` after E0753), its parent helper (10.6), the unit declaring it, and its
    /// members in declaration order (it has no fields). `builtin` marks a built-in helper of
    /// `System` (10.7), whose members are the VM's.
    Helper {
        is_class: bool,
        target: Ty,
        parent: Option<TypeId>,
        unit: Option<u32>,
        methods: Vec<ClassMethod>,
        statics: Vec<ClassStatic>,
        properties: Vec<ClassProperty>,
        builtin: Option<BuiltinHelper>,
    },
    /// A type parameter (`docs/spec/generics.md` 2.2): the unit declaring it (its generic type
    /// or routine), its number in the type parameters, and its constraints (3.1; set once they
    /// are resolved).
    Param {
        unit: u32,
        index: u32,
        constraints: Vec<Constraint>,
    },
    /// `type X = type Y` (`docs/spec/types.md` 1.3): the original type `Y` (a distinct type or
    /// a type its root may be).
    Distinct { base: Ty },
    /// Declared but not yet resolved (a forward reference inside a `type` section), or failed.
    Pending,
}

/// A constraint of a type parameter (`docs/spec/generics.md` 1.1, 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    Class,
    Record,
    Constructor,
    /// `NotRefCounted` (3.6).
    NotRefCounted,
    /// A class type or an interface type.
    Type(Ty),
}

/// The member of a built-in helper (`docs/spec/objects.md` 10.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinHelper {
    /// `function ToString: string` (the integer, real, `Boolean`, `Char` and enumeration types).
    ToString,
    /// `property Length: Integer` (`string`, `UTF8String`, dynamic arrays).
    Length,
}

impl BuiltinHelper {
    /// The member's name.
    pub fn name(self) -> &'static str {
        match self {
            BuiltinHelper::ToString => "ToString",
            BuiltinHelper::Length => "Length",
        }
    }
}

/// The family of a procedural type (`docs/spec/objects.md` chapter 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcFamily {
    /// `procedure(…)` / `function(…): T` (`Ty::Proc`).
    Plain,
    /// `… of object` (`Ty::Callable`).
    Method,
    /// `reference to …` (`Ty::Callable`).
    Reference,
}

/// A parameter of a procedural type.
#[derive(Debug, Clone)]
pub struct ProcParam {
    pub name: String,
    pub ty: Ty,
    pub mode: ParamMode,
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
    /// `TIOErrorKind` of `System.SysUtils`, the type of `EInOutError.ErrorKind` and
    /// `ENotSupportedException.ErrorKind` (`docs/spec/exceptions.md` 2.2).
    pub io_error_kind: TypeId,
    /// The generic declarations (`docs/spec/generics.md` 2.1) and their type parameters
    /// (`Ty::Param`), in order.
    pub generics: Vec<(TypeId, Vec<TypeId>)>,
    /// The instantiations of the generic types (`docs/spec/generics.md` 4.1), in the order they
    /// were made: one per (definition, type arguments).
    pub instances: Vec<Instance>,
    /// The index in `instances` of each instantiation's type.
    pub instance_index: std::collections::HashMap<TypeId, usize>,
    /// The instantiation of each key (`instances` by definition and type arguments).
    pub instance_keys: std::collections::HashMap<(TypeId, Vec<Ty>), TypeId>,
    /// The generic definitions whose members are all declared (their instantiations can copy
    /// them).
    pub generic_done: std::collections::HashSet<TypeId>,
    /// The type of each generic definition (`Ty::Class`, `Ty::Record`, `Ty::Intf`, `Ty::Proc`,
    /// `Ty::Callable`, `Ty::Array`, `Ty::DynArray`; `docs/spec/generics.md` 2.1): the form its
    /// instantiations take.
    pub generic_tys: std::collections::HashMap<TypeId, Ty>,
    /// The generic procedural types of `System` made so far (`TProc<T>`, `TFunc<…>`,
    /// `TPredicate<T>`; `docs/spec/generics.md` 6.2), by lowercase name and number of type
    /// parameters.
    pub system_generics: std::collections::HashMap<(String, usize), TypeId>,
    /// The class or record each generic definition declared inside one is nested in
    /// (`docs/spec/generics.md` 2.5): its type parameters start with the outer type's.
    pub generic_outer: std::collections::HashMap<TypeId, TypeId>,
    /// The generic types declared inside a class or a record (`TFoo.TBox<T>`), found by the
    /// owner, the name and the number of their own type parameters (2.3).
    pub nested_generics: Vec<NestedGeneric>,
    /// The types declared in the body of a generic routine or of a method of a generic type
    /// (`docs/spec/generics.md` 2.6): generic definitions with the type parameters of the body,
    /// and how their instantiations are named.
    pub generic_local: std::collections::HashMap<TypeId, LocalGeneric>,
    /// The anonymous types with type parameters in them, replaced (by the type and the type
    /// arguments of the replacement).
    pub subst_cache: std::collections::HashMap<(TypeId, Vec<Ty>), TypeId>,
    /// The classes treated as `[ValueEquality]` (`docs/spec/equality.md` 3.4): those it is written
    /// on and their descendants until one overrides both `Equals` and `GetHashCode` or declares
    /// its own `Equal`. An instantiation is treated as its definition.
    pub value_equality: std::collections::HashSet<TypeId>,
    /// The fields marked `[EqualityMember]` (`docs/spec/equality.md` 3.4): (the class declaring
    /// it, its slot). An instantiation's are its definition's.
    pub equality_members: std::collections::HashSet<(TypeId, u32)>,
    /// The file of each anonymous record written in a declaration (`docs/spec/types.md` 1.2):
    /// where the `record type declared here` of a diagnostic naming it points
    /// (`docs/spec/diagnostics.md` 2.2).
    pub anon_record_files: std::collections::HashMap<TypeId, FileId>,
    /// The root type of each distinct type, once they are erased (`sema::erase`): the type
    /// information of a distinct type is its root's kind under its own name
    /// (`docs/spec/rtti.md` 2.2).
    pub distinct_roots: std::collections::HashMap<TypeId, Ty>,
    /// The parameters of each method as declared in its class or interface: their names and
    /// declared names (where their attributes are written), for the type information
    /// (`docs/spec/rtti.md` 1.4). A body put off or never written has no variables to name them.
    pub decl_params:
        std::collections::HashMap<crate::sema::hir::RoutineId, Vec<(String, TextRange)>>,
}

/// The type's own `Equal` among its methods: the operator both of whose operands are of the
/// type `own` (`docs/spec/equality.md` 2.5, 3.3), which is its key equality.
pub fn own_equal(methods: &[ClassMethod], own: Ty) -> Option<&ClassMethod> {
    methods.iter().find(|m| {
        m.kind == MethodKind::Operator
            && m.name.eq_ignore_ascii_case("Equal")
            && m.params.iter().all(|(t, _)| *t == own)
    })
}

/// How the key equality and the key hash of a type are expanded (`docs/spec/equality.md` 2.1,
/// 8.3), the same for `KeyEquals` / `KeyHash` and the generated bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    /// The 64 bits of the slot: `EqInt` / `HashInt` (the integer types, `Boolean`, `Char`,
    /// enumerations, subranges, small sets, `Currency`, class references, procedural types).
    Int,
    /// `F32ToF64`, then as `Double`.
    Single,
    /// `KeyEqF64` / `HashF64`.
    Double,
    /// `EqStr` / `HashStr`.
    Str,
    /// `EqU8` / `HashUtf8`.
    Utf8,
    /// `SetEq` / `HashSetBig`.
    BigSet,
    /// `CallableKeyEq` / `CallableHash` (method reference and anonymous method types).
    Callable,
    /// A class or an interface (2.6): the same handle, else `Equals` / `GetHashCode` through the
    /// table when not destroyed (`ObjAlive`).
    Object,
    /// A record, a static array or a dynamic array: a call of a routine (the generated
    /// `<eq:…>` / `<hash:…>`, or a record's own `Equal` and `GetHashCode`).
    Call,
}

/// How `KeyCompare` orders the values of a type (`docs/spec/collections.md` 10.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderKind {
    /// The 64 bits of the slot as a signed integer (the integer types but `UInt64`, `Boolean`,
    /// `Char`, enumerations, subranges, `Currency`).
    Int,
    /// The 64 bits of the slot as an unsigned integer (`UInt64`).
    UInt,
    /// The total order of `Single`.
    Single,
    /// The total order of `Double`.
    Double,
    /// `CmpStr` (code point order, as `<`).
    Str,
    /// `CmpU8` (byte order).
    Utf8,
    /// A call of the generated `<cmp:…>` of a record or a reference counted class with its own
    /// `LessThan`.
    Call,
    /// No default order: `EInvalidOperation` when called in a generic body, E0703 otherwise.
    None,
}

/// A generic type declared inside a class or a record (`docs/spec/generics.md` 2.1, 2.3).
#[derive(Debug, Clone)]
pub struct NestedGeneric {
    pub owner: TypeId,
    pub lower: String,
    /// The number of its own type parameters (after the owner's).
    pub arity: usize,
    pub id: TypeId,
    pub span: TextRange,
}

/// A type declared in the body of a generic routine or of a method of a generic type
/// (`docs/spec/generics.md` 2.6). Its instantiation is named after the routine's:
/// `Hello.Find<Integer>.TLoc` (`Find<Integer>.TLoc` in diagnostics),
/// `Hello.TBox<Integer>.Get.TLoc`.
#[derive(Debug, Clone)]
pub struct LocalGeneric {
    /// The generic type of the method the body belongs to: its type parameters come first, and
    /// its instantiation is written before `head`.
    pub class: Option<TypeId>,
    /// The name of the routine (with the class for a method of a type that is not generic;
    /// only the method's for a method of `class`).
    pub head: String,
    /// The program's or unit's name, before `head` in the full name when there is no `class`.
    pub unit: String,
    /// The number of the routine's own type parameters (after the class's), written after
    /// `head`.
    pub own: usize,
    /// The enclosing routines after the generic one, and the type's name (`.Inner.TLoc`).
    pub tail: String,
}

/// An instantiation of a generic type (`docs/spec/generics.md` 4.1, `docs/design/generics.md`
/// 3.1): a type of its own whose members are the definition's with the type parameters replaced
/// by the type arguments. Its arguments may hold type parameters while a generic declaration is
/// checked (`TBox<T>` in the body of `TStack<T>`: an open instantiation, which has no bodies).
#[derive(Debug, Clone)]
pub struct Instance {
    pub id: TypeId,
    pub def: TypeId,
    pub args: Vec<Ty>,
    /// Its members are made (they wait until the definition's are declared).
    pub filled: bool,
    /// The unit whose analysis asked for it first (the `hir` dump shows the instantiations of the
    /// file passed; `docs/spec/generics.md` 8).
    pub unit: u32,
    /// Where it was asked for: the type arguments written by the user, or, for one asked for while
    /// another was made, that one's (E0764; `docs/spec/generics.md` 4.4).
    pub origin: Option<crate::source::Span>,
    /// The instantiation whose members or bodies asked for it (the path of E0764).
    pub from: Option<TypeId>,
}

/// The fully qualified name of `TValue` (`docs/spec/rtti.md` chapter 3).
pub const TVALUE_NAME: &str = "System.TValue";

/// The id of the root exception class `Exception`.
pub const EXCEPTION: TypeId = TypeId(crate::bytecode::exc::EXCEPTION);

/// The id of the root class `TObject` (`docs/spec/objects.md` 14.1).
pub const TOBJECT: TypeId = TypeId(crate::bytecode::TOBJECT_TYPE);

/// The id of `TRefObject`, the root of the reference counted classes.
pub const TREFOBJECT: TypeId = TypeId(crate::bytecode::TREFOBJECT_TYPE);

/// The id of `TCustomAttribute`, the root of the attribute classes (`docs/spec/attributes.md` 4.1).
pub const TCUSTOMATTRIBUTE: TypeId = TypeId(crate::bytecode::TCUSTOMATTRIBUTE_TYPE);

/// The id of the root interface `IInterface` (`docs/spec/objects.md` 7.2, 14.7).
pub const IINTERFACE: TypeId = TypeId(crate::bytecode::IINTERFACE_TYPE);

/// The id of `TIndexRange`, the built-in record of a range written as an index
/// (`docs/spec/objects.md` 12.3, 14.10).
pub const TINDEXRANGE: TypeId = TypeId(crate::bytecode::TINDEXRANGE_TYPE);
/// `TAssertionValue`, an element of `EAssertionFailed.Values` (`docs/spec/exceptions.md` 2.1).
pub const TASSERTIONVALUE: TypeId = TypeId(crate::bytecode::TASSERTIONVALUE_TYPE);

/// The name of `TIndexRange.Clamp`, whose body the compiler writes out where it is called
/// (`docs/spec/objects.md` 12.3, 14.10).
pub const INDEX_RANGE_CLAMP: &str = "Clamp";

/// `TIndexRange` (`docs/spec/objects.md` 12.3): the fields `First`, `Last`, `HasFirst` and
/// `HasLast` (slots 0 to 3), and `Clamp`, a method without a routine.
fn index_range_record() -> TypeDef {
    let field = |name: &str, ty: Ty| Field {
        name: name.to_string(),
        ty,
        span: TextRange::default(),
        vis: Visibility::Public,
    };
    use crate::syntax::ast::ParamMode;
    let clamp = ClassMethod {
        name: INDEX_RANGE_CLAMP.to_string(),
        routine: None,
        span: TextRange::default(),
        vis: Visibility::Public,
        kind: MethodKind::Procedure,
        params: vec![
            (Ty::Integer, ParamMode::Value),
            (Ty::Integer, ParamMode::Value),
            (Ty::Integer, ParamMode::Out),
            (Ty::Integer, ParamMode::Out),
        ],
        ret: None,
        vmt: None,
        is_override: false,
        is_abstract: false,
        is_final: false,
        self_kind: SelfKind::Instance,
    };
    TypeDef {
        name: Some("TIndexRange".into()),
        full_name: Some("TIndexRange".into()),
        kind: TypeKind::Record {
            fields: vec![
                field("First", Ty::Integer),
                field("Last", Ty::Integer),
                field("HasFirst", Ty::Boolean),
                field("HasLast", Ty::Boolean),
            ],
            unit: None,
            methods: vec![clamp],
            statics: Vec::new(),
            properties: Vec::new(),
        },
        span: TextRange::default(),
    }
}

/// `TAssertionValue` (`docs/spec/exceptions.md` 2.1): a subexpression of a failed `Assert` and
/// its value as shown (`docs/spec/semantics.md` 5.4).
fn assertion_value_record() -> TypeDef {
    let field = |name: &str| Field {
        name: name.to_string(),
        ty: Ty::String,
        span: TextRange::default(),
        vis: Visibility::Public,
    };
    TypeDef {
        name: Some("TAssertionValue".into()),
        full_name: Some("TAssertionValue".into()),
        kind: TypeKind::Record {
            fields: vec![field("Expr"), field("Value")],
            unit: None,
            methods: Vec::new(),
            statics: Vec::new(),
            properties: Vec::new(),
        },
        span: TextRange::default(),
    }
}

/// The id of `TClass` (`class of TObject`; `docs/spec/objects.md` 2.8): the first entry after the
/// reserved built-in type indexes 0 to 63 (14.1), so that it never shares a number with a built-in
/// type added later. A class reference has no type index, so it is not one of the VM's types.
pub const TCLASS: TypeId = TypeId(crate::bytecode::FIRST_TYPE_INDEX);

/// The id of `TProc` (`reference to procedure`; `docs/spec/objects.md` 9.9), after `TClass`. A
/// procedural type has no type index either.
pub const TPROC: TypeId = TypeId(crate::bytecode::FIRST_TYPE_INDEX + 1);

/// The id of `TTypeKind` (`System`; `docs/spec/rtti.md` 2.3), after the built-in helpers.
pub const TTYPEKIND: TypeId = TypeId(TPROC.0 + 1 + super::helpers::BUILTIN_HELPER_COUNT);

/// The values of `TTypeKind` in ordinal order (`docs/spec/rtti.md` 2.3).
pub const TYPE_KINDS: [&str; 22] = [
    "tkUnknown",
    "tkInteger",
    "tkChar",
    "tkEnumeration",
    "tkFloat",
    "tkString",
    "tkSet",
    "tkClass",
    "tkMethod",
    "tkWChar",
    "tkLString",
    "tkWString",
    "tkVariant",
    "tkArray",
    "tkRecord",
    "tkInterface",
    "tkInt64",
    "tkDynArray",
    "tkUString",
    "tkClassRef",
    "tkPointer",
    "tkProcedure",
];

/// The virtual method table of `TObject` and `TRefObject` (`docs/spec/objects.md` 14.1):
/// `Destroy`, `AfterConstruction`, `BeforeDestruction` (doing nothing), `ToString`, `Equals` and
/// `GetHashCode` (`docs/spec/equality.md` 3.1, 8.2).
pub const TOBJECT_VMT: [VmtEntry; 6] = [
    VmtEntry::Builtin(0),
    VmtEntry::Builtin(0),
    VmtEntry::Builtin(0),
    VmtEntry::Builtin(1),
    VmtEntry::Builtin(3),
    VmtEntry::Builtin(4),
];

/// The number of `ToString` in the virtual method table.
pub const VMT_TO_STRING: u32 = 3;
/// The numbers of `Equals` and `GetHashCode` in the virtual method table
/// (`docs/spec/equality.md` 3.1).
pub const VMT_EQUALS: u32 = crate::bytecode::VMT_EQUALS;
pub const VMT_GET_HASH_CODE: u32 = crate::bytecode::VMT_GET_HASH_CODE;

/// The methods of `TObject` whose bodies are the VM's (`docs/spec/objects.md` chapter 5): the
/// constructor `Create`, `Free`, then the virtual methods in table order.
fn tobject_methods() -> Vec<ClassMethod> {
    let m = |name: &str, kind: MethodKind, ret: Option<Ty>, vmt: u32| ClassMethod {
        name: name.to_string(),
        routine: None,
        span: TextRange::default(),
        vis: Visibility::Public,
        kind,
        params: Vec::new(),
        ret,
        vmt: Some(vmt),
        is_override: false,
        is_abstract: false,
        is_final: false,
        self_kind: SelfKind::Instance,
    };
    let create = ClassMethod {
        vmt: None,
        ..m("Create", MethodKind::Constructor, None, 0)
    };
    let free = ClassMethod {
        vmt: None,
        ..m("Free", MethodKind::Procedure, None, 0)
    };
    vec![
        create,
        free,
        m("Destroy", MethodKind::Destructor, None, 0),
        m("AfterConstruction", MethodKind::Procedure, None, 1),
        m("BeforeDestruction", MethodKind::Procedure, None, 2),
        m(
            "ToString",
            MethodKind::Function,
            Some(Ty::String),
            VMT_TO_STRING,
        ),
        ClassMethod {
            params: vec![(Ty::Class(TOBJECT), crate::syntax::ast::ParamMode::Value)],
            ..m(
                "Equals",
                MethodKind::Function,
                Some(Ty::Boolean),
                VMT_EQUALS,
            )
        },
        m(
            "GetHashCode",
            MethodKind::Function,
            Some(Ty::Int64),
            VMT_GET_HASH_CODE,
        ),
    ]
}

/// The names of the fields behind the properties of the built-in exception classes
/// (`docs/spec/objects.md` 8.2, 14.8): private to the class, so only the properties reach them.
pub const EXC_FIELD_MESSAGE: &str = "FMessage";
pub const EXC_FIELD_INNER: &str = "FInnerException";

/// A built-in exception class (`docs/spec/objects.md` 8.1, 8.2): a reference counted class whose
/// slots are `Message`, `InnerException` and the class's own slot (`EConvertError.Value`,
/// `EInOutError.ErrorCode`), each behind a property. `Exception` declares the constructors
/// `Create(Msg)` / `CreateFmt` (bodies of the compiler; the routine of `Create` is made when first
/// used) and overrides `ToString` (returns `Message`).
fn exception_class(name: &str, parent: Option<u32>, extra: &[bool]) -> TypeDef {
    let field = |name: &str, ty: Ty, slot: u32| ClassField {
        name: name.to_string(),
        ty,
        span: TextRange::default(),
        vis: Visibility::StrictPrivate,
        slot,
    };
    let read_only = |name: &str, field: &str, ty: Ty, slot: u32| ClassProperty {
        name: name.to_string(),
        span: TextRange::default(),
        vis: Visibility::Public,
        ty,
        params: Vec::new(),
        index: None,
        read: Some(Accessor::Field {
            slot,
            name: field.to_string(),
            ty,
        }),
        write: None,
        is_class: false,
        is_default: false,
    };
    let root = parent == Some(crate::bytecode::TREFOBJECT_TYPE);
    let mut fields = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    if root {
        let msg = EXC_FIELD_MESSAGE;
        let inner = Ty::Class(EXCEPTION);
        fields.push(field(msg, Ty::String, crate::bytecode::EXC_SLOT_MESSAGE));
        fields.push(field(
            EXC_FIELD_INNER,
            inner,
            crate::bytecode::EXC_SLOT_INNER,
        ));
        let mut message = read_only("Message", msg, Ty::String, 0);
        message.write = message.read.clone();
        properties.push(message);
        properties.push(read_only(
            "InnerException",
            EXC_FIELD_INNER,
            inner,
            crate::bytecode::EXC_SLOT_INNER,
        ));
        let ctor = |name: &str, params: Vec<(Ty, crate::syntax::ast::ParamMode)>| ClassMethod {
            name: name.to_string(),
            routine: None,
            span: TextRange::default(),
            vis: Visibility::Public,
            kind: MethodKind::Constructor,
            params,
            ret: None,
            vmt: None,
            is_override: false,
            is_abstract: false,
            is_final: false,
            self_kind: SelfKind::Instance,
        };
        let c = crate::syntax::ast::ParamMode::Const;
        methods.push(ctor(EXC_CREATE, vec![(Ty::String, c)]));
        methods.push(ctor(EXC_CREATE_FMT, vec![(Ty::String, c)]));
        methods.push(ClassMethod {
            name: "ToString".into(),
            routine: None,
            span: TextRange::default(),
            vis: Visibility::Public,
            kind: MethodKind::Function,
            params: Vec::new(),
            ret: Some(Ty::String),
            vmt: Some(VMT_TO_STRING),
            is_override: true,
            is_abstract: false,
            is_final: false,
            self_kind: SelfKind::Instance,
        });
    }
    let slot = crate::bytecode::EXC_SLOT_EXTRA;
    // `ErrorKind` (the slot after `ErrorCode`, or the only one of `ENotSupportedException`)
    // is added by `TypeTable::new` once its type exists
    match extra {
        [true] => {
            fields.push(field("FValue", Ty::String, slot));
            properties.push(read_only("Value", "FValue", Ty::String, slot));
        }
        [false, ..] if name == "EInOutError" => {
            fields.push(field("FErrorCode", Ty::Integer, slot));
            let mut code = read_only("ErrorCode", "FErrorCode", Ty::Integer, slot);
            code.write = code.read.clone();
            properties.push(code);
        }
        _ => {}
    }
    let mut vmt = TOBJECT_VMT.to_vec();
    vmt[VMT_TO_STRING as usize] = VmtEntry::Builtin(2);
    TypeDef {
        name: Some(name.to_string()),
        full_name: Some(name.to_string()),
        kind: TypeKind::Class {
            parent: parent.map(TypeId),
            fields,
            unit: None,
            ref_counted: true,
            methods,
            vmt,
            is_abstract: false,
            sealed: false,
            statics: Vec::new(),
            properties,
            intfs: Vec::new(),
        },
        span: TextRange::default(),
    }
}

/// The names of the built-in constructors of `Exception` (`docs/spec/objects.md` 8.2).
pub const EXC_CREATE: &str = "Create";
pub const EXC_CREATE_FMT: &str = "CreateFmt";

impl Default for TypeTable {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeTable {
    /// A table holding the built-in classes (the exception classes, `TObject`, `TRefObject`) and
    /// `TArray<Exception>` at the ids that are their type indexes in the bytecode
    /// (`docs/spec/exceptions.md` 6.1, `docs/spec/objects.md` 14.1, 14.8).
    pub fn new() -> Self {
        let mut t = TypeTable {
            defs: Vec::new(),
            io_error_kind: TypeId(0),
            generics: Vec::new(),
            instances: Vec::new(),
            instance_keys: std::collections::HashMap::new(),
            generic_done: Default::default(),
            generic_tys: Default::default(),
            system_generics: Default::default(),
            generic_outer: Default::default(),
            nested_generics: Vec::new(),
            generic_local: Default::default(),
            instance_index: Default::default(),
            subst_cache: std::collections::HashMap::new(),
            value_equality: Default::default(),
            equality_members: Default::default(),
            anon_record_files: Default::default(),
            distinct_roots: Default::default(),
            decl_params: Default::default(),
        };
        let exceptions = crate::bytecode::BUILTIN_EXCEPTIONS;
        let root = |name: &str, parent: Option<TypeId>, ref_counted: bool| TypeDef {
            name: Some(name.to_string()),
            full_name: Some(name.to_string()),
            kind: TypeKind::Class {
                parent,
                fields: Vec::new(),
                unit: None,
                ref_counted,
                methods: tobject_methods(),
                vmt: TOBJECT_VMT.to_vec(),
                is_abstract: false,
                sealed: false,
                statics: Vec::new(),
                properties: Vec::new(),
                intfs: Vec::new(),
            },
            span: TextRange::default(),
        };
        // the unused ids of the reserved range stay pending and are never referred to
        while (t.defs.len() as u32) < crate::bytecode::FIRST_TYPE_INDEX {
            let id = t.defs.len() as u32;
            let def = match exceptions.iter().find(|e| e.0 == id) {
                Some((_, name, parent, extra)) => exception_class(name, *parent, extra),
                None if id == crate::bytecode::EXCEPTION_ARRAY_TYPE => TypeDef {
                    name: None,
                    full_name: None,
                    kind: TypeKind::DynArray {
                        elem: Ty::Class(EXCEPTION),
                        generic: true,
                    },
                    span: TextRange::default(),
                },
                None if id == TOBJECT.0 => root("TObject", None, false),
                None if id == TREFOBJECT.0 => root("TRefObject", Some(TOBJECT), true),
                None if id == TCUSTOMATTRIBUTE.0 => {
                    root("TCustomAttribute", Some(TREFOBJECT), true)
                }
                None if id == TINDEXRANGE.0 => index_range_record(),
                None if id == TASSERTIONVALUE.0 => assertion_value_record(),
                None if id == crate::bytecode::ASSERTION_VALUES_TYPE => TypeDef {
                    name: None,
                    full_name: None,
                    kind: TypeKind::DynArray {
                        elem: Ty::Record(TASSERTIONVALUE),
                        generic: true,
                    },
                    span: TextRange::default(),
                },
                None if id == IINTERFACE.0 => TypeDef {
                    name: Some("IInterface".into()),
                    full_name: Some("IInterface".into()),
                    kind: TypeKind::Interface {
                        parent: None,
                        unit: None,
                        methods: Vec::new(),
                        properties: Vec::new(),
                        count: 0,
                        guid: None,
                    },
                    span: TextRange::default(),
                },
                None => TypeDef {
                    name: None,
                    full_name: None,
                    kind: TypeKind::Pending,
                    span: TextRange::default(),
                },
            };
            t.defs.push(def);
        }
        let tclass = t.class_ref(
            TOBJECT,
            DynWritten::Named {
                name: "TClass".into(),
                full_name: "TClass".into(),
                span: TextRange::default(),
            },
        );
        debug_assert_eq!(tclass, Ty::ClassRef(TCLASS));
        let tproc = t.add(TypeDef {
            name: Some("TProc".into()),
            full_name: Some("TProc".into()),
            kind: TypeKind::Proc {
                family: ProcFamily::Reference,
                params: Vec::new(),
                ret: None,
            },
            span: TextRange::default(),
        });
        debug_assert_eq!(tproc, TPROC);
        let helpers = t.add_builtin_helpers();
        debug_assert_eq!(
            helpers.first().map(|h| h.0),
            Some(TPROC.0 + 1),
            "the built-in helpers follow `TProc`"
        );
        let kinds = t.add(TypeDef {
            name: Some("TTypeKind".into()),
            full_name: Some("System.TTypeKind".into()),
            kind: TypeKind::Enum {
                values: TYPE_KINDS
                    .iter()
                    .enumerate()
                    .map(|(i, n)| EnumValueDef {
                        name: (*n).to_string(),
                        ord: i as i128,
                        span: TextRange::default(),
                    })
                    .collect(),
                low: 0,
                high: TYPE_KINDS.len() as i128 - 1,
            },
            span: TextRange::default(),
        });
        debug_assert_eq!(kinds, TTYPEKIND, "`TTypeKind` follows the built-in helpers");
        t.add_io_error_kind();
        t
    }

    /// `TIOErrorKind` (`System.SysUtils`) and the `ErrorKind` properties of `EInOutError` and
    /// `ENotSupportedException` that have it as their type (`docs/spec/exceptions.md` 2.2): read
    /// and write, so that Pascal code makes the exception of a host error.
    fn add_io_error_kind(&mut self) {
        let values = puy_ext::IO_ERROR_KINDS
            .iter()
            .enumerate()
            .map(|(i, n)| EnumValueDef {
                name: (*n).to_string(),
                ord: i as i128,
                span: TextRange::default(),
            })
            .collect();
        let id = self.add(TypeDef {
            name: Some("TIOErrorKind".into()),
            full_name: Some("System.SysUtils.TIOErrorKind".into()),
            kind: TypeKind::Enum {
                values,
                low: 0,
                high: puy_ext::IO_ERROR_KINDS.len() as i128 - 1,
            },
            span: TextRange::default(),
        });
        self.io_error_kind = id;
        let ty = Ty::Enum(id);
        for (class, slot) in [
            (
                crate::bytecode::exc::IN_OUT_ERROR,
                crate::bytecode::EXC_SLOT_EXTRA + 1,
            ),
            (
                crate::bytecode::exc::NOT_SUPPORTED,
                crate::bytecode::EXC_SLOT_EXTRA,
            ),
        ] {
            let TypeKind::Class {
                fields, properties, ..
            } = &mut self.defs[class as usize].kind
            else {
                unreachable!("a built-in exception class");
            };
            fields.push(ClassField {
                name: "FErrorKind".into(),
                ty,
                span: TextRange::default(),
                vis: Visibility::StrictPrivate,
                slot,
            });
            let access = Accessor::Field {
                slot,
                name: "FErrorKind".into(),
                ty,
            };
            properties.push(ClassProperty {
                name: "ErrorKind".into(),
                span: TextRange::default(),
                vis: Visibility::Public,
                ty,
                params: Vec::new(),
                index: None,
                read: Some(access.clone()),
                write: Some(access),
                is_class: false,
                is_default: false,
            });
        }
    }

    /// Whether a class is one of the built-in exception classes or a descendant of one
    /// (`docs/spec/objects.md` 8.1).
    pub fn is_exception_class(&self, id: TypeId) -> bool {
        matches!(self.get(id).kind, TypeKind::Class { .. }) && self.is_descendant(id, EXCEPTION)
    }

    /// Whether an id is one of the VM's built-in types (its id is its type index).
    pub fn is_builtin(id: TypeId) -> bool {
        id.0 < crate::bytecode::BUILTIN_TYPE_END
    }

    /// The parent of a class or an interface.
    pub fn parent(&self, id: TypeId) -> Option<TypeId> {
        match self.get(id).kind {
            TypeKind::Class { parent, .. }
            | TypeKind::Interface { parent, .. }
            | TypeKind::Helper { parent, .. } => parent,
            _ => None,
        }
    }

    /// Whether a type id is a helper (`docs/spec/objects.md` chapter 10).
    pub fn is_helper(&self, id: TypeId) -> bool {
        matches!(self.get(id).kind, TypeKind::Helper { .. })
    }

    /// The type a helper is for (`docs/spec/objects.md` 10.2).
    pub fn helper_target(&self, id: TypeId) -> Option<Ty> {
        match self.get(id).kind {
            TypeKind::Helper { target, .. } => Some(target),
            _ => None,
        }
    }

    /// Whether `Self` of the instance methods of a record or record helper is passed like a
    /// `var` argument (`docs/spec/objects.md` chapter 6, 10.8).
    pub fn self_by_ref(&self, id: TypeId) -> bool {
        matches!(
            self.get(id).kind,
            TypeKind::Record { .. }
                | TypeKind::Helper {
                    is_class: false,
                    ..
                }
        )
    }

    /// Whether a type id is an interface (`docs/spec/objects.md` 7.1).
    pub fn is_interface(&self, id: TypeId) -> bool {
        matches!(self.get(id).kind, TypeKind::Interface { .. })
    }

    /// The number of methods of an interface, its ancestors' included.
    pub fn intf_method_count(&self, id: TypeId) -> u32 {
        match self.get(id).kind {
            TypeKind::Interface { count, .. } => count,
            _ => 0,
        }
    }

    /// The interface table of a class (`docs/spec/objects.md` 7.3).
    pub fn class_intfs(&self, id: TypeId) -> &[IntfImpl] {
        match &self.get(id).kind {
            TypeKind::Class { intfs, .. } => intfs,
            _ => &[],
        }
    }

    /// Whether the class `class` implements the interface `intf` (7.3: its interface table has
    /// it).
    pub fn implements(&self, class: TypeId, intf: TypeId) -> bool {
        self.class_intfs(class).iter().any(|e| e.intf == intf)
    }

    /// The methods of an interface and its ancestors, by method number.
    pub fn intf_methods(&self, id: TypeId) -> Vec<(TypeId, ClassMethod)> {
        let mut out = Vec::new();
        for c in self.class_chain(id).into_iter().rev() {
            out.extend(self.class_methods(c).iter().map(|m| (c, m.clone())));
        }
        out
    }

    /// The fields of a class and its ancestors, the parent's first (slot order). Stops on a cycle
    /// (E0727).
    pub fn class_fields(&self, id: TypeId) -> Vec<(TypeId, &ClassField)> {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            if chain.contains(&c) || chain.len() > self.defs.len() {
                break;
            }
            chain.push(c);
            cur = self.parent(c);
        }
        let mut out = Vec::new();
        for c in chain.into_iter().rev() {
            if let TypeKind::Class { fields, .. } = &self.get(c).kind {
                out.extend(fields.iter().map(|f| (c, f)));
            }
        }
        out
    }

    /// The class and its ancestors, the class first. Stops on a cycle (E0727).
    pub fn class_chain(&self, id: TypeId) -> Vec<TypeId> {
        let mut chain = Vec::with_capacity(8);
        let mut cur = Some(id);
        while let Some(c) = cur {
            if chain.contains(&c) || chain.len() > self.defs.len() {
                break;
            }
            chain.push(c);
            cur = self.parent(c);
        }
        chain
    }

    /// The properties a class declares.
    pub fn class_properties(&self, id: TypeId) -> &[ClassProperty] {
        match &self.get(id).kind {
            TypeKind::Class { properties, .. }
            | TypeKind::Record { properties, .. }
            | TypeKind::Interface { properties, .. }
            | TypeKind::Helper { properties, .. } => properties,
            _ => &[],
        }
    }

    /// The methods a class declares.
    pub fn class_methods(&self, id: TypeId) -> &[ClassMethod] {
        match &self.get(id).kind {
            TypeKind::Class { methods, .. }
            | TypeKind::Record { methods, .. }
            | TypeKind::Interface { methods, .. }
            | TypeKind::Helper { methods, .. } => methods,
            _ => &[],
        }
    }

    /// The method of entry `k` of a class's table as `TShape.Area`: the nearest class declaring a
    /// method of the entry, and the method's name (`docs/spec/objects.md` 3.1, `EAbstractError`).
    pub fn vmt_method_name(&self, class: TypeId, k: u32) -> String {
        for c in self.class_chain(class) {
            if let Some(m) = self.class_methods(c).iter().find(|m| m.vmt == Some(k)) {
                return format!("{}.{}", self.name(Ty::Class(c)), m.name);
            }
        }
        format!("{}.#{k}", self.name(Ty::Class(class)))
    }

    /// The `class var`s, constants and nested types a class declares.
    pub fn class_statics(&self, id: TypeId) -> &[ClassStatic] {
        match &self.get(id).kind {
            TypeKind::Class { statics, .. }
            | TypeKind::Record { statics, .. }
            | TypeKind::Helper { statics, .. } => statics,
            _ => &[],
        }
    }

    /// The class reference type of a class (`docs/spec/objects.md` 2.8). The first use of a
    /// class creates the entry and gives it its name, like `dyn_array`.
    pub fn class_ref(&mut self, class: TypeId, written: DynWritten) -> Ty {
        if let Some(i) = self
            .defs
            .iter()
            .position(|d| matches!(d.kind, TypeKind::ClassRef { class: c } if c == class))
        {
            return Ty::ClassRef(TypeId(i as u32));
        }
        let (name, full_name, span) = match written {
            DynWritten::Named {
                name,
                full_name,
                span,
            } => (Some(name), Some(full_name), span),
            DynWritten::Anonymous { span, .. } => (None, None, span),
        };
        Ty::ClassRef(self.add(TypeDef {
            name,
            full_name,
            kind: TypeKind::ClassRef { class },
            span,
        }))
    }

    /// The class reference type of a class, if it has been made.
    pub fn find_class_ref(&self, class: TypeId) -> Option<Ty> {
        self.defs
            .iter()
            .position(|d| matches!(d.kind, TypeKind::ClassRef { class: c } if c == class))
            .map(|i| Ty::ClassRef(TypeId(i as u32)))
    }

    /// The class a class reference type points to.
    pub fn class_ref_target(&self, ty: Ty) -> Option<TypeId> {
        match ty {
            Ty::ClassRef(id) => match self.get(id).kind {
                TypeKind::ClassRef { class } => Some(class),
                _ => None,
            },
            _ => None,
        }
    }

    /// The virtual method table of a class.
    pub fn class_vmt(&self, id: TypeId) -> &[VmtEntry] {
        match &self.get(id).kind {
            TypeKind::Class { vmt, .. } => vmt,
            _ => &[],
        }
    }

    /// Whether `AfterConstruction` of the class is overridden, so that creating an instance of it
    /// calls it (`docs/spec/objects.md` 3.1).
    pub fn has_after_construction(&self, class: TypeId) -> bool {
        self.class_vmt(class)
            .get(crate::bytecode::VMT_AFTER_CONSTRUCTION as usize)
            .is_some_and(|e| matches!(e, VmtEntry::Func(_)))
    }

    /// Whether a type id is a type parameter (`docs/spec/generics.md` 2.2).
    pub fn is_param(&self, id: TypeId) -> bool {
        matches!(self.get(id).kind, TypeKind::Param { .. })
    }

    /// Whether a type id is a record (`docs/spec/objects.md` chapter 6).
    pub fn is_record(&self, id: TypeId) -> bool {
        matches!(self.get(id).kind, TypeKind::Record { .. })
    }

    /// The unit declaring a class (`None` for a built-in class).
    pub fn class_unit(&self, id: TypeId) -> Option<u32> {
        match &self.get(id).kind {
            TypeKind::Class { unit, .. }
            | TypeKind::Record { unit, .. }
            | TypeKind::Interface { unit, .. }
            | TypeKind::Helper { unit, .. } => *unit,
            _ => None,
        }
    }

    /// Whether instances of a class cannot be created (`docs/spec/objects.md` 3.1): `class
    /// abstract`, or an abstract method no one has overridden.
    pub fn class_is_abstract(&self, id: TypeId) -> bool {
        match &self.get(id).kind {
            TypeKind::Class {
                is_abstract, vmt, ..
            } => *is_abstract || vmt.contains(&VmtEntry::Abstract),
            _ => false,
        }
    }

    /// The number of slots of a class's instances (its fields and its ancestors').
    pub fn class_slot_count(&self, id: TypeId) -> u32 {
        self.class_fields(id).len() as u32
    }

    /// Whether class `a` is `b` or a descendant of it. Stops on a cycle (E0727).
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

    /// `System.TValue` (`docs/spec/rtti.md` chapter 3), when the part of `System` written in
    /// Pascal that declares it was read.
    pub fn tvalue(&self) -> Option<Ty> {
        self.defs
            .iter()
            .position(|d| {
                matches!(d.kind, TypeKind::Record { .. })
                    && d.full_name.as_deref() == Some(TVALUE_NAME)
            })
            .map(|k| Ty::Record(TypeId(k as u32)))
    }

    /// Whether `ty` is `System.TValue`.
    pub fn is_tvalue(&self, ty: Ty) -> bool {
        matches!(ty, Ty::Record(id) if self.get(id).full_name.as_deref() == Some(TVALUE_NAME))
    }

    /// Whether `ty` is `TValue` or a record or array holding one (`docs/spec/rtti.md` 3.8).
    pub fn contains_tvalue(&self, ty: Ty) -> bool {
        let mut work = vec![ty];
        let mut seen: Vec<Ty> = Vec::new();
        while let Some(t) = work.pop() {
            if seen.contains(&t) {
                continue;
            }
            seen.push(t);
            if self.is_tvalue(t) {
                return true;
            }
            match t {
                Ty::Record(_) => work.extend(self.fields(t).iter().map(|f| f.ty)),
                Ty::Array(_) | Ty::DynArray(_) | Ty::OpenArray(_) => work.extend(self.elem_of(t)),
                _ => {}
            }
        }
        false
    }

    /// How a value of `ty` is counted (`docs/spec/objects.md` 4.1, 14.1): [`SLOT_REF`] for the
    /// managed types of [`Ty::is_managed`] and the reference counted classes, [`SLOT_OBJ`] for
    /// exactly `TObject` (counted by its contents), [`SLOT_PLAIN`] for the rest (the classes
    /// managed by hand among them).
    ///
    /// [`SLOT_REF`]: crate::bytecode::SLOT_REF
    /// [`SLOT_OBJ`]: crate::bytecode::SLOT_OBJ
    /// [`SLOT_PLAIN`]: crate::bytecode::SLOT_PLAIN
    pub fn slot_kind(&self, ty: Ty) -> u8 {
        use crate::bytecode::{SLOT_OBJ, SLOT_PLAIN, SLOT_REF};
        match ty {
            // An interface's value is counted by its contents, as `TObject`'s (7.4, 14.7)
            Ty::Class(TOBJECT) | Ty::Intf(_) => SLOT_OBJ,
            Ty::Class(id) => match self.get(id).kind {
                TypeKind::Class {
                    ref_counted: true, ..
                } => SLOT_REF,
                _ => SLOT_PLAIN,
            },
            t if t.is_managed() => SLOT_REF,
            _ => SLOT_PLAIN,
        }
    }

    /// The type of the value a `[weak]` reference reads and takes (`docs/spec/objects.md` 4.4,
    /// 7.4): a class or an interface; any other type as it is.
    pub fn strong(&self, ty: Ty) -> Ty {
        match ty {
            Ty::Weak(c) if self.is_interface(c) => Ty::Intf(c),
            Ty::Weak(c) => Ty::Class(c),
            t => t,
        }
    }

    /// Whether a value of `ty` holds a reference (the managed types and the classes counted
    /// directly or by their contents; [`TypeTable::slot_kind`]).
    pub fn managed(&self, ty: Ty) -> bool {
        self.slot_kind(ty) != crate::bytecode::SLOT_PLAIN
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

    /// Whether `ty` is a dynamic or open array of `Byte`, whose elements the VM holds one byte
    /// each (a dynamic array of bytes: requirements §5.5, `docs/spec/types.md` 9): the type entry
    /// of a dynamic array of bytes and the instructions reading and writing its elements
    /// (`BytesGet` / `BytesSet`) both follow this.
    pub fn is_bytes_array(&self, ty: Ty) -> bool {
        self.dyn_elem(ty) == Some(Ty::Byte)
    }

    /// The element type of any array (static, dynamic, open).
    pub fn elem_of(&self, ty: Ty) -> Option<Ty> {
        self.array(ty).map(|a| a.3).or_else(|| self.dyn_elem(ty))
    }

    /// Whether a value of `ty` can hold an object (an exception, or in M3 any class instance) in
    /// itself, its fields or its elements: letting go of such a value can run a destructor, so the
    /// moment it is let go can be observed (`docs/design/lifetime.md` 3.4).
    pub fn holds_object(&self, ty: Ty) -> bool {
        let mut seen = Vec::new();
        self.holds_object_in(ty, &mut seen)
    }

    fn holds_object_in(&self, ty: Ty, seen: &mut Vec<Ty>) -> bool {
        if seen.contains(&ty) {
            return false;
        }
        seen.push(ty);
        match ty {
            // An instance of a class managed by hand is not let go with the value
            // (`docs/spec/objects.md` 3.2); one of a reference counted class is, and so may be
            // one held by `TObject` (4.2)
            Ty::Class(_) | Ty::Intf(_) => self.slot_kind(ty) != crate::bytecode::SLOT_PLAIN,
            // An anonymous method holds the `Self` it captured (`docs/spec/objects.md` 9.6)
            Ty::Callable(_) => self.proc_family(ty) == Some(ProcFamily::Reference),
            // A type parameter may stand for a reference counted class, and the managed part of a
            // `TValue` may hold an instance of one (`docs/spec/rtti.md` 3.1)
            Ty::Param(_) | Ty::ValueRef => true,
            Ty::Record(_) => self
                .fields(ty)
                .iter()
                .any(|f| self.holds_object_in(f.ty, seen)),
            Ty::Array(_) | Ty::DynArray(_) | Ty::OpenArray(_) => self
                .elem_of(ty)
                .is_some_and(|e| self.holds_object_in(e, seen)),
            // Every other type is listed, so that a new one (a class in M3) has to be decided here
            Ty::TypeInfo
            | Ty::ShortInt
            | Ty::Byte
            | Ty::SmallInt
            | Ty::Word
            | Ty::Integer
            | Ty::Cardinal
            | Ty::Int64
            | Ty::UInt64
            | Ty::Boolean
            | Ty::Char
            | Ty::String
            | Ty::Utf8String
            | Ty::Single
            | Ty::Double
            | Ty::Currency
            | Ty::ClassRef(_)
            // A procedural value points to code, and a method reference holds its instance
            // weakly (`docs/spec/objects.md` 9.2, 9.3)
            | Ty::Proc(_)
            // A `[weak]` reference holds no count (`docs/spec/objects.md` 4.4)
            | Ty::Weak(_)
            | Ty::Enum(_)
            | Ty::Subrange(_)
            | Ty::Set(_)
            | Ty::BigSet(_)
            // Not the type of a value (E0753)
            | Ty::Helper(_)
            // The root of a distinct type is a simple type (`docs/spec/types.md` 1.3)
            | Ty::Distinct(_)
            | Ty::Error => false,
        }
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
                TypeKind::Record { fields, .. } => fields,
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

    /// The root type of a distinct type (`docs/spec/types.md` 1.3): its original type followed
    /// until one is not a distinct type. Any other type is its own root.
    pub fn root(&self, mut ty: Ty) -> Ty {
        while let Ty::Distinct(id) = ty {
            ty = match &self.get(id).kind {
                TypeKind::Distinct { base } => *base,
                _ => return Ty::Error,
            };
        }
        ty
    }

    /// The original type of a distinct type (`docs/spec/types.md` 1.3), `None` for any other.
    pub fn origin(&self, ty: Ty) -> Option<Ty> {
        match ty {
            Ty::Distinct(id) => match &self.get(id).kind {
                TypeKind::Distinct { base } => Some(*base),
                _ => Some(Ty::Error),
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
            Ty::Distinct(_) => self.ord_range(self.root(ty)),
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
        bound_text(self.base(self.root(ty)), v)
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

    // ---- Flat layout (`docs/design/records.md` 3.2, 3.3; `docs/spec/bytecode.md` 1.5) ----

    /// The number of slots a value of `ty` takes in a container (a field, an element, an
    /// instance's field): the slots of a record or static array laid out flat
    /// ([`TypeTable::flat_width`]), 1 for any other value (a block among them).
    pub fn slot_width(&self, ty: Ty) -> u32 {
        self.flat_width(ty).unwrap_or(1)
    }

    /// The number of slots of a record or static array laid out flat in its containers: its
    /// [`TypeTable::agg_width`] when that is 1 to [`MAX_FLAT_WIDTH`]. `None` for an aggregate
    /// kept as a block in its containers, and for any other type.
    pub fn flat_width(&self, ty: Ty) -> Option<u32> {
        self.agg_width(ty)
            .filter(|w| (1..=MAX_FLAT_WIDTH).contains(w))
            .map(|w| w as u32)
    }

    /// The number of slots of the block of a record or static array: the slot widths of its
    /// fields, or of its elements times their number. `None` for any other type.
    pub fn agg_width(&self, ty: Ty) -> Option<u64> {
        match ty {
            Ty::Record(_) => Some(
                self.fields(ty)
                    .iter()
                    .map(|f| u64::from(self.slot_width(f.ty)))
                    .sum(),
            ),
            Ty::Array(_) => {
                let (_, lo, hi, elem) = self.array(ty)?;
                let n = u64::try_from((hi - lo + 1).max(0)).unwrap_or(u64::MAX);
                Some(n.saturating_mul(u64::from(self.slot_width(elem))))
            }
            _ => None,
        }
    }

    /// The first slot of field `index` of the record `rec` in its block or flat place.
    pub fn field_offset(&self, rec: Ty, index: u32) -> u32 {
        self.fields(rec)
            .iter()
            .take(index as usize)
            .map(|f| self.slot_width(f.ty))
            .sum()
    }

    /// The first slot of the field at `slot` (its number among the fields of the class and its
    /// ancestors, [`ClassField::slot`]) in an instance of the class `id`.
    pub fn class_field_offset(&self, id: TypeId, slot: u32) -> u32 {
        self.class_fields(id)
            .iter()
            .take(slot as usize)
            .map(|(_, f)| self.slot_width(f.ty))
            .sum()
    }

    /// The slot kinds ([`TypeTable::slot_kind`]) of a value of `ty` in a container: those of its
    /// slots in order when it is laid out flat, else its own kind.
    pub fn flat_kinds(&self, ty: Ty, out: &mut Vec<u8>) {
        if self.flat_width(ty).is_none() {
            out.push(self.slot_kind(ty));
            return;
        }
        match ty {
            Ty::Record(_) => {
                for f in self.fields(ty) {
                    self.flat_kinds(f.ty, out);
                }
            }
            _ => {
                let (_, lo, hi, elem) = self.array(ty).expect("a flat value is an aggregate");
                for _ in lo..=hi {
                    self.flat_kinds(elem, out);
                }
            }
        }
    }

    /// The type name in diagnostics and dumps (`docs/spec/types.md` 1.2): the declared name, or
    /// `record` / `array[L..H] of T` for an anonymous type.
    pub fn name(&self, ty: Ty) -> String {
        self.name_with(ty, NameForm::Short)
    }

    /// The name in the bytecode type section, the `ir` dump and the leak report: declared types by
    /// their fully qualified name, anonymous types written out with qualified names inside.
    pub fn full_name(&self, ty: Ty) -> String {
        self.name_with(ty, NameForm::Full)
    }

    /// [`TypeTable::name`] with each anonymous record in it followed by a mark of its type (see
    /// [`mark_anon_record`]), for the text of a diagnostic: the marks are taken out with
    /// [`take_anon_record_marks`] when the diagnostics of the file are complete, and become its
    /// `record type declared here` (`docs/spec/diagnostics.md` 2.2).
    pub fn marked_name(&self, ty: Ty) -> String {
        self.name_with(ty, NameForm::Marked)
    }

    /// The fully qualified name in the type information (`docs/spec/rtti.md` 2.4): as
    /// [`TypeTable::full_name`], with the types of `System` qualified (`System.Integer`,
    /// `System.TObject`, `array of System.Integer`).
    pub fn rtti_name(&self, ty: Ty) -> String {
        self.name_with(ty, NameForm::Rtti)
    }

    fn name_with(&self, ty: Ty, form: NameForm) -> String {
        let full = matches!(form, NameForm::Full | NameForm::Rtti);
        if let Ty::Weak(c) = ty {
            return self.name_with(self.strong(Ty::Weak(c)), form);
        }
        let Some(id) = ty.type_id() else {
            return match form {
                NameForm::Rtti => format!("System.{}", ty.builtin_name()),
                _ => ty.builtin_name().to_string(),
            };
        };
        let def = self.get(id);
        if let (Ty::OpenArray(_), TypeKind::DynArray { elem, .. }) = (ty, &def.kind) {
            return format!("array of {}", self.name_with(*elem, form));
        }
        if form == NameForm::Rtti
            && let Some(inst) = self.instance_of(id)
            && let Some(def_name) = &self.get(inst.def).full_name
        {
            // An instantiation's type arguments are named as the types themselves
            // (`docs/spec/rtti.md` 2.4)
            let head = def_name.split('<').next().unwrap_or(def_name);
            let args: Vec<String> = inst.args.iter().map(|a| self.name_with(*a, form)).collect();
            return format!("{head}<{}>", args.join(","));
        }
        if let (true, Some(n)) = (full, &def.full_name) {
            // The types `System` declares are named without their unit elsewhere
            if form == NameForm::Rtti && id.0 <= TTYPEKIND.0 && !n.contains('.') {
                return format!("System.{n}");
            }
            return n.clone();
        }
        if let Some(n) = &def.name {
            return n.clone();
        }
        match &def.kind {
            TypeKind::Record { .. } if form == NameForm::Marked => mark_anon_record(id),
            TypeKind::Record { .. } | TypeKind::Pending => "record".to_string(),
            TypeKind::Class { .. } => "class".to_string(),
            TypeKind::Interface { .. } => "interface".to_string(),
            TypeKind::Helper { .. } => "helper".to_string(),
            TypeKind::ClassRef { class } => {
                format!("class of {}", self.name_with(Ty::Class(*class), form))
            }
            TypeKind::Enum { .. } => "enumeration".to_string(),
            TypeKind::Subrange { base, low, high } => {
                format!("{}..{}", bound_text(*base, *low), bound_text(*base, *high))
            }
            TypeKind::Set { base, .. } => format!("set of {}", self.name_with(*base, form)),
            TypeKind::Array {
                index,
                low,
                high,
                elem,
            } => format!(
                "array[{}..{}] of {}",
                bound_text(*index, *low),
                bound_text(*index, *high),
                self.name_with(*elem, form)
            ),
            TypeKind::DynArray { elem, generic } => {
                let e = self.name_with(*elem, form);
                if *generic {
                    format!("TArray<{e}>")
                } else {
                    format!("array of {e}")
                }
            }
            TypeKind::Proc { .. } => self.proc_text(id, form, false),
            TypeKind::Param { .. } => "<type>".to_string(),
            TypeKind::Distinct { base } => self.name_with(*base, form),
        }
    }

    /// The type parameters of a generic declaration (`docs/spec/generics.md` 2.1), empty for
    /// any other type.
    pub fn type_params(&self, id: TypeId) -> &[TypeId] {
        self.generics
            .iter()
            .find(|(g, _)| *g == id)
            .map_or(&[], |(_, p)| p.as_slice())
    }

    /// Renumbers the global variables the members of the types refer to (`class var`s and the
    /// properties reading or writing them; `docs/spec/generics.md` 2.5). After
    /// `ir::reach::drop_unreachable`, a dropped `class var` is [`crate::ir::reach::DROPPED`].
    pub fn remap_globals(&mut self, map: &impl Fn(GlobalId) -> GlobalId) {
        for def in &mut self.defs {
            let (statics, properties) = match &mut def.kind {
                TypeKind::Class {
                    statics,
                    properties,
                    ..
                }
                | TypeKind::Record {
                    statics,
                    properties,
                    ..
                }
                | TypeKind::Helper {
                    statics,
                    properties,
                    ..
                } => (Some(statics), properties),
                TypeKind::Interface { properties, .. } => (None, properties),
                _ => continue,
            };
            for s in statics.into_iter().flatten() {
                if let StaticKind::Var(g, _) = &mut s.kind {
                    *g = map(*g);
                }
            }
            for p in properties {
                for a in [&mut p.read, &mut p.write].into_iter().flatten() {
                    if let Accessor::ClassVar(g) = a {
                        *g = map(*g);
                    }
                }
            }
        }
    }

    /// The generic procedural types of `System` named `lower` (`docs/spec/generics.md` 6.2), made
    /// on first use: (number of type parameters, definition).
    pub fn system_generics_named(&mut self, lower: &str) -> Vec<(usize, TypeId)> {
        let arities: &[usize] = match lower {
            "tproc" => &[1, 2, 3, 4],
            "tfunc" => &[1, 2, 3, 4, 5],
            "tpredicate" => &[1],
            _ => return Vec::new(),
        };
        arities
            .iter()
            .map(|&n| (n, self.system_generic(lower, n)))
            .collect()
    }

    /// `TProc<T1, …>` = `reference to procedure(Arg1: T1; …)`, `TFunc<T1, …, TResult>` =
    /// `reference to function(Arg1: T1; …): TResult`, `TPredicate<T>` = `reference to
    /// function(Arg1: T): Boolean` (`docs/spec/generics.md` 6.2): parameters by value.
    fn system_generic(&mut self, lower: &str, arity: usize) -> TypeId {
        if let Some(id) = self.system_generics.get(&(lower.to_string(), arity)) {
            return *id;
        }
        let (base, has_result) = match lower {
            "tproc" => ("TProc", false),
            "tfunc" => ("TFunc", true),
            _ => ("TPredicate", false),
        };
        let args = if has_result { arity - 1 } else { arity };
        let mut names: Vec<String> = match args {
            1 if !has_result || arity == 2 => vec!["T".to_string()],
            n => (1..=n).map(|k| format!("T{k}")).collect(),
        };
        if has_result {
            names.push("TResult".to_string());
        }
        let id = self.add(TypeDef {
            name: Some(format!("{base}<{}>", names.join(", "))),
            full_name: Some(base.to_string()),
            kind: TypeKind::Pending,
            span: TextRange::default(),
        });
        let params: Vec<TypeId> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                self.add(TypeDef {
                    name: Some(n.clone()),
                    full_name: None,
                    // No diagnostic points at them: they have no constraints and no body
                    kind: TypeKind::Param {
                        unit: 0,
                        index: i as u32,
                        constraints: Vec::new(),
                    },
                    span: TextRange::default(),
                })
            })
            .collect();
        let proc_params = params[..args]
            .iter()
            .enumerate()
            .map(|(k, p)| ProcParam {
                name: format!("Arg{}", k + 1),
                ty: Ty::Param(*p),
                mode: ParamMode::Value,
            })
            .collect();
        let ret = match lower {
            "tfunc" => Some(Ty::Param(params[args])),
            "tpredicate" => Some(Ty::Boolean),
            _ => None,
        };
        self.get_mut(id).kind = TypeKind::Proc {
            family: ProcFamily::Reference,
            params: proc_params,
            ret,
        };
        self.generics.push((id, params));
        self.generic_tys.insert(id, Ty::Callable(id));
        self.generic_done.insert(id);
        self.system_generics.insert((lower.to_string(), arity), id);
        id
    }

    /// The type whose declaration gives the layout of `id` (its parent, fields and kind): the
    /// generic definition of an instantiation whose members are not made yet (a class of the
    /// same `type` section inheriting from it; `docs/spec/generics.md` 2.4), `id` otherwise.
    pub fn layout(&self, id: TypeId) -> TypeId {
        match self.instance_of(id) {
            Some(i) if !i.filled => i.def,
            _ => id,
        }
    }

    /// The type parameters a generic definition declares itself (those after its outer type's,
    /// for a type nested in a generic type; `docs/spec/generics.md` 2.5). None for a type declared
    /// in a generic body, whose type parameters are all the body's (2.6).
    pub fn own_type_params(&self, id: TypeId) -> &[TypeId] {
        let all = self.type_params(id);
        if self.generic_local.contains_key(&id) {
            return &[];
        }
        let outer = self
            .generic_outer
            .get(&id)
            .map_or(0, |o| self.type_params(*o).len());
        &all[outer.min(all.len())..]
    }

    /// The instantiation a type is, if it is one.
    pub fn instance_of(&self, id: TypeId) -> Option<&Instance> {
        self.instance_index.get(&id).map(|i| &self.instances[*i])
    }

    /// The definition of an instantiation, or the type itself.
    fn definition(&self, id: TypeId) -> TypeId {
        self.instance_of(id).map_or(id, |i| i.def)
    }

    /// Whether a class is treated as `[ValueEquality]` (`docs/spec/equality.md` 3.4).
    pub fn is_value_equality(&self, id: TypeId) -> bool {
        self.value_equality.contains(&self.definition(id))
    }

    /// Whether the field at `slot` of the class `owner` (which declares it) is marked
    /// `[EqualityMember]` (`docs/spec/equality.md` 3.4).
    pub fn is_equality_member(&self, owner: TypeId, slot: u32) -> bool {
        self.equality_members
            .contains(&(self.definition(owner), slot))
    }

    /// The fields the generated `Equals` / `GetHashCode` of a `[ValueEquality]` class compare
    /// (`docs/spec/equality.md` 3.4): those of the class and its ancestors but `TObject` /
    /// `TRefObject`, the root's first; the ones marked `[EqualityMember]` if any is, otherwise
    /// every field but the `[weak]` ones.
    pub fn equality_fields(&self, id: TypeId) -> Vec<&ClassField> {
        let all: Vec<(TypeId, &ClassField)> = self
            .class_fields(id)
            .into_iter()
            .filter(|(c, _)| *c != TOBJECT && *c != TREFOBJECT)
            .collect();
        let marked: Vec<&ClassField> = all
            .iter()
            .filter(|(c, f)| self.is_equality_member(*c, f.slot))
            .map(|(_, f)| *f)
            .collect();
        if !marked.is_empty() {
            return marked;
        }
        all.into_iter()
            .filter(|(_, f)| !matches!(f.ty, Ty::Weak(_)))
            .map(|(_, f)| f)
            .collect()
    }

    /// A record's own `Equal` (both operands of its type) and its `GetHashCode`, which are its
    /// key equality and key hash (`docs/spec/equality.md` 2.5, 3.2). `None` when it declares no
    /// such `Equal` or no `function GetHashCode: Int64` (E0765).
    pub fn record_key_methods(
        &self,
        id: TypeId,
    ) -> Option<(crate::sema::hir::RoutineId, crate::sema::hir::RoutineId)> {
        let methods = self.class_methods(id);
        let equal = own_equal(methods, Ty::Record(id))?;
        let hash = methods.iter().find(|m| {
            m.kind == MethodKind::Function
                && m.name.eq_ignore_ascii_case("GetHashCode")
                && m.self_kind == SelfKind::Instance
                && m.params.is_empty()
                && m.ret == Some(Ty::Int64)
        })?;
        Some((equal.routine?, hash.routine?))
    }

    /// Whether a record declares its own `Equal` (`docs/spec/equality.md` 2.5): its key equality
    /// is then that operator, never the generated one.
    pub fn record_declares_equal(&self, id: TypeId) -> bool {
        own_equal(self.class_methods(id), Ty::Record(id)).is_some()
    }

    /// The default order of `ty` (`docs/spec/collections.md` 10.2). `ty` has no type parameter
    /// in it.
    pub fn order_kind(&self, ty: Ty) -> OrderKind {
        match self.strong(ty) {
            Ty::UInt64 => OrderKind::UInt,
            Ty::ShortInt
            | Ty::Byte
            | Ty::SmallInt
            | Ty::Word
            | Ty::Integer
            | Ty::Cardinal
            | Ty::Int64
            | Ty::Boolean
            | Ty::Char
            | Ty::Currency
            | Ty::Enum(_) => OrderKind::Int,
            Ty::Subrange(_) => self.order_kind(self.base(ty)),
            // `docs/spec/types.md` 1.3
            Ty::Distinct(_) => self.order_kind(self.root(ty)),
            Ty::Single => OrderKind::Single,
            Ty::Double => OrderKind::Double,
            Ty::String => OrderKind::Str,
            Ty::Utf8String => OrderKind::Utf8,
            t @ (Ty::Record(_) | Ty::Class(_)) if self.own_less_than(t).is_some() => {
                OrderKind::Call
            }
            _ => OrderKind::None,
        }
    }

    /// The `LessThan` giving the default order of a record or a reference counted class
    /// (`docs/spec/collections.md` 10.2): the record's own (both operands of its type), or the
    /// one of the nearest class of the chain declaring its own, with that class's type.
    pub fn own_less_than(&self, ty: Ty) -> Option<(crate::sema::hir::RoutineId, Ty)> {
        let own = |id: TypeId, t: Ty| {
            self.class_methods(id)
                .iter()
                .find(|m| {
                    m.kind == MethodKind::Operator
                        && m.name.eq_ignore_ascii_case("LessThan")
                        && m.params.len() == 2
                        && m.params.iter().all(|(p, _)| *p == t)
                })
                .and_then(|m| m.routine)
                .map(|r| (r, t))
        };
        match ty {
            Ty::Record(id) => own(id, ty),
            Ty::Class(id) if self.is_descendant(id, TREFOBJECT) => self
                .class_chain(id)
                .into_iter()
                .find_map(|c| own(c, Ty::Class(c))),
            _ => None,
        }
    }

    /// How the key equality and hash of a value of `ty` are expanded
    /// (`docs/spec/equality.md` 8.3). `ty` has no type parameter in it.
    pub fn key_kind(&self, ty: Ty) -> KeyKind {
        match ty {
            Ty::Single => KeyKind::Single,
            Ty::Double => KeyKind::Double,
            Ty::String => KeyKind::Str,
            Ty::Utf8String => KeyKind::Utf8,
            Ty::BigSet(_) => KeyKind::BigSet,
            Ty::Callable(_) => KeyKind::Callable,
            Ty::Class(_) | Ty::Intf(_) | Ty::Weak(_) => KeyKind::Object,
            Ty::Record(_) | Ty::Array(_) | Ty::DynArray(_) | Ty::OpenArray(_) => KeyKind::Call,
            Ty::Subrange(_) => match &self.get(ty.type_id().expect("a subrange")).kind {
                TypeKind::Subrange { base, .. } => self.key_kind(*base),
                _ => KeyKind::Int,
            },
            // `docs/spec/types.md` 1.3
            Ty::Distinct(_) => self.key_kind(self.root(ty)),
            _ => KeyKind::Int,
        }
    }

    /// The generic definition and type arguments of a generic definition (its own type
    /// parameters) or an instantiation; `None` for any other type.
    pub fn generic_key(&self, id: TypeId) -> Option<(TypeId, Vec<Ty>)> {
        if let Some(i) = self.instance_of(id) {
            return Some((i.def, i.args.clone()));
        }
        let params = self.type_params(id);
        (!params.is_empty()).then(|| (id, params.iter().map(|p| Ty::Param(*p)).collect()))
    }

    /// Whether a type has a type parameter in it (`T`, `TArray<T>`, `TBox<T>`, a procedural type
    /// taking a `T`, a generic definition itself).
    pub fn has_param(&self, ty: Ty) -> bool {
        self.has_param_in(ty, 0)
    }

    fn has_param_in(&self, ty: Ty, depth: u32) -> bool {
        // Types refer to each other only through declarations, which end (a class's fields are
        // not walked); the bound guards against a cycle through anonymous types
        if depth > 64 {
            return false;
        }
        let Some(id) = ty.type_id() else {
            return false;
        };
        if let Ty::Param(_) = ty {
            return true;
        }
        if let Some((def, args)) = self.generic_key(id) {
            return def == id && !self.type_params(id).is_empty()
                || args.iter().any(|a| self.has_param_in(*a, depth + 1));
        }
        match &self.get(id).kind {
            TypeKind::DynArray { elem, .. } | TypeKind::Array { elem, .. } => {
                self.has_param_in(*elem, depth + 1)
            }
            TypeKind::ClassRef { class } => self.has_param_in(Ty::Class(*class), depth + 1),
            TypeKind::Proc { params, ret, .. } => {
                params.iter().any(|p| self.has_param_in(p.ty, depth + 1))
                    || ret.is_some_and(|r| self.has_param_in(r, depth + 1))
            }
            TypeKind::Record { fields, .. } if self.get(id).name.is_none() => {
                fields.iter().any(|f| self.has_param_in(f.ty, depth + 1))
            }
            _ => false,
        }
    }

    /// The depth of the type arguments' tree (`docs/spec/generics.md` 4.4): an instantiation or
    /// a dynamic array is one more than its deepest argument or element.
    pub fn nesting_depth(&self, ty: Ty) -> u32 {
        let Some(id) = ty.type_id() else { return 0 };
        if let Some(i) = self.instance_of(id) {
            return 1 + i
                .args
                .iter()
                .map(|a| self.nesting_depth(*a))
                .max()
                .unwrap_or(0);
        }
        match &self.get(id).kind {
            TypeKind::DynArray { elem, .. } => 1 + self.nesting_depth(*elem),
            TypeKind::Array { elem, .. } => self.nesting_depth(*elem),
            _ => 0,
        }
    }

    /// The name of a type argument in the full name of an instantiation
    /// (`docs/spec/generics.md` 4.1): the built-in types unqualified, declared types by their full
    /// name, dynamic arrays as `array of T` (an alias is the type it names).
    pub fn canonical_name(&self, ty: Ty) -> String {
        match ty {
            Ty::DynArray(id) | Ty::OpenArray(id) => match &self.get(id).kind {
                TypeKind::DynArray { elem, .. } => {
                    format!("array of {}", self.canonical_name(*elem))
                }
                _ => self.full_name(ty),
            },
            _ => self.full_name(ty),
        }
    }

    /// Whether a type is a generic declaration (its methods have no function; `docs/spec/generics.md` 7).
    pub fn is_generic(&self, id: TypeId) -> bool {
        self.generics.iter().any(|(g, _)| *g == id)
    }

    /// The constraints of a type parameter (`docs/spec/generics.md` 3.1).
    pub fn constraints(&self, id: TypeId) -> &[Constraint] {
        match &self.get(id).kind {
            TypeKind::Param { constraints, .. } => constraints,
            _ => &[],
        }
    }

    /// A procedural type written out (`docs/spec/objects.md` 16.2): `procedure(Integer) of
    /// object`, `reference to function(const string): Boolean`. With `names`, the parameters
    /// have their names (`function(A: Integer; B: Integer): Integer`; the `hir` dump, 15.5).
    pub fn proc_text(&self, id: TypeId, form: NameForm, names: bool) -> String {
        let TypeKind::Proc {
            family,
            params,
            ret,
        } = &self.get(id).kind
        else {
            return "<type>".to_string();
        };
        let list: Vec<(Option<&str>, Ty, ParamMode)> = params
            .iter()
            .map(|p| (names.then_some(p.name.as_str()), p.ty, p.mode))
            .collect();
        self.signature_text(*family, &list, *ret, form)
    }

    /// A signature written out as a procedural type of `family` (the type of a routine or
    /// method in E0702; `docs/spec/objects.md` 16.2). Parameters with a name are written
    /// `Name: T` and separated by `; `, the others by `, `.
    pub fn signature_text(
        &self,
        family: ProcFamily,
        params: &[(Option<&str>, Ty, ParamMode)],
        ret: Option<Ty>,
        form: NameForm,
    ) -> String {
        let named = params.iter().any(|p| p.0.is_some());
        let list: Vec<String> = params
            .iter()
            .map(|(name, ty, mode)| {
                let mode = match mode {
                    ParamMode::Value => "",
                    ParamMode::Const => "const ",
                    ParamMode::Var => "var ",
                    ParamMode::Out => "out ",
                };
                let t = self.name_with(*ty, form);
                match name {
                    Some(n) => format!("{mode}{n}: {t}"),
                    None => format!("{mode}{t}"),
                }
            })
            .collect();
        let mut out = String::new();
        if family == ProcFamily::Reference {
            out.push_str("reference to ");
        }
        out.push_str(if ret.is_some() {
            "function"
        } else {
            "procedure"
        });
        if !list.is_empty() {
            out.push_str(&format!("({})", list.join(if named { "; " } else { ", " })));
        }
        if let Some(r) = ret {
            out.push_str(&format!(": {}", self.name_with(r, form)));
        }
        if family == ProcFamily::Method {
            out.push_str(" of object");
        }
        out
    }

    /// The family, parameters and return type of a procedural type.
    pub fn proc_sig(&self, ty: Ty) -> Option<(ProcFamily, &[ProcParam], Option<Ty>)> {
        match ty {
            Ty::Proc(id) | Ty::Callable(id) => match &self.get(id).kind {
                TypeKind::Proc {
                    family,
                    params,
                    ret,
                } => Some((*family, params, *ret)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether two procedural types have the same parameters (modes and types) and return type,
    /// whatever their families and names (`docs/spec/objects.md` 9.2: compared by structure).
    pub fn same_signature(&self, a: Ty, b: Ty) -> bool {
        if a == b {
            return true;
        }
        let Some((_, pb, rb)) = self.proc_sig(b) else {
            return false;
        };
        let pb: Vec<(Ty, ParamMode)> = pb.iter().map(|p| (p.ty, p.mode)).collect();
        self.has_signature(a, &pb, rb)
    }

    /// Whether the procedural type `ty` has these parameters and return type
    /// (`docs/spec/objects.md` 9.2): the same number, and the same mode and type of each, where
    /// procedural types are compared by structure and open arrays by element type.
    pub fn has_signature(&self, ty: Ty, params: &[(Ty, ParamMode)], ret: Option<Ty>) -> bool {
        self.has_signature_at(ty, params, ret, 0)
    }

    fn has_signature_at(
        &self,
        ty: Ty,
        params: &[(Ty, ParamMode)],
        ret: Option<Ty>,
        depth: u32,
    ) -> bool {
        let Some((_, pa, ra)) = self.proc_sig(ty) else {
            return false;
        };
        // A type that names itself in its parameters is compared to a bounded depth
        if depth > 8 {
            return false;
        }
        let same = |x: Ty, y: Ty| {
            x == y
                || match (x, y) {
                    (Ty::OpenArray(e), Ty::OpenArray(f)) => e == f,
                    (Ty::Proc(_), Ty::Proc(_)) | (Ty::Callable(_), Ty::Callable(_)) => {
                        self.proc_family(x) == self.proc_family(y)
                            && self.proc_sig(y).is_some_and(|(_, pb, rb)| {
                                let pb: Vec<(Ty, ParamMode)> =
                                    pb.iter().map(|p| (p.ty, p.mode)).collect();
                                self.has_signature_at(x, &pb, rb, depth + 1)
                            })
                    }
                    _ => false,
                }
        };
        pa.len() == params.len()
            && pa
                .iter()
                .zip(params)
                .all(|(p, (t, m))| p.mode == *m && same(p.ty, *t))
            && match (ra, ret) {
                (None, None) => true,
                (Some(x), Some(y)) => same(x, y),
                _ => false,
            }
    }

    /// The family of a procedural type.
    pub fn proc_family(&self, ty: Ty) -> Option<ProcFamily> {
        self.proc_sig(ty).map(|(f, _, _)| f)
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

#[cfg(test)]
mod tests {
    use super::*;

    // The marks of anonymous records are taken out of a diagnostic's text, each type found once in
    // the order named; the text keeps no trace of them.
    #[test]
    fn anon_record_marks_are_taken_out() {
        let mut text = format!(
            "expected `{}`, found `array[0..1] of {}` and `{}`",
            mark_anon_record(TypeId(7)),
            mark_anon_record(TypeId(3)),
            mark_anon_record(TypeId(7))
        );
        let mut found = Vec::new();
        take_anon_record_marks(&mut text, &mut found);
        assert_eq!(
            text,
            "expected `record`, found `array[0..1] of record` and `record`"
        );
        assert_eq!(found, vec![TypeId(7), TypeId(3)]);
    }

    // A private use character that does not start a whole mark (a string constant shown in a
    // message) is left as written.
    #[test]
    fn text_like_a_mark_is_kept() {
        for (idx, s) in ["'\u{E000}'", "'\u{E000}12'", "'\u{E000}x\u{E001}'"]
            .into_iter()
            .enumerate()
        {
            let mut text = s.to_string();
            let mut found = Vec::new();
            take_anon_record_marks(&mut text, &mut found);
            assert_eq!(text, s, "{idx}");
            assert!(found.is_empty(), "{idx}");
        }
    }
}
