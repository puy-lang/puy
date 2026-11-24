//! The API for writing Puy extensions in Rust: functions and classes whose bodies are Rust,
//! declared to Pascal in a unit (requirements §6; design: `docs/design/extensions.md`).
//!
//! An extension publishes one registration table, an [`Extension`]: the unit's Pascal
//! declarations (its `interface` part; routines marked `native` have their bodies here) and the
//! Rust functions that are those bodies. A runtime gathers the extensions it carries into an
//! [`ExtensionSet`] and hands it to the compiler (which reads the unit's declarations) and to the
//! isolate (which binds the functions by name).
//!
//! The VM's own types never show here: a function reads its arguments and calls back into Pascal
//! through [`CallCtx`], which the VM implements ([`Vm`]), and gives back an owned [`Value`] or a
//! [`PasError`]. Like `puy-host`, this crate does no I/O and builds for both wasm32 targets;
//! an extension that needs the OS goes through the isolate's [`HostSet`] ([`CallCtx::host`]).
//!
//! An extension's values are not `Send`: they live in the isolate that made them.
//!
//! An extension is not written against [`Extension`] by hand: the attributes of
//! `puy-ext-macros`, re-exported here ([`pascal_module`] and the attributes it reads inside the
//! module), generate both the registration table and the unit's declarations from the Rust
//! definitions (requirements §6, the direction of declarations; `docs/spec/extensions.md`
//! chapter 7).
//!
//! ```
//! #[puy_ext::pascal_module(unit = "Demo.Sums")]
//! mod sums {
//!     use puy_ext::prelude::*;
//!
//!     /// The sum of the bytes.
//!     #[pascal_function]
//!     pub fn byte_sum(data: &[u8]) -> i64 {
//!         data.iter().map(|&b| i64::from(b)).sum()
//!     }
//! }
//!
//! assert_eq!(sums::extension().unit, "Demo.Sums");
//! assert!(sums::SOURCE.contains("function ByteSum(const Data: TArray<Byte>): Int64; native;"));
//! ```

// The generated code names this crate `::puy_ext`, here as in an extension
extern crate self as puy_ext;

use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::Rc;

pub use puy_ext_macros::{
    pascal_callback, pascal_class, pascal_const, pascal_constructor, pascal_enum, pascal_exception,
    pascal_function, pascal_methods, pascal_module, pascal_record,
};
pub use puy_host::{
    Absent, CancelMode, Capability, File, FileTimes, FileType, HostError, HostErrorKind,
    HostFuture, HostSet, MemoryStats, OpenOptions, SeekFrom, TestRunner,
};

mod env;
pub use env::{EnvTable, is_env_name};

/// What an extension module uses: `use puy_ext::prelude::*;`.
pub mod prelude {
    pub use crate::{
        CallCtx, CancelMode, PasChar, PasError, PasFn, PasObj, PasRef, PasStr, PasString,
        PascalException, Utf8String, Value, pascal_callback, pascal_class, pascal_const,
        pascal_constructor, pascal_enum, pascal_exception, pascal_function, pascal_methods,
        pascal_module, pascal_record,
    };
}

// ---- The registration table ----

/// The registration table of an extension: one unit (`docs/design/extensions.md` 2, 3.2).
#[derive(Debug)]
pub struct Extension {
    /// The unit's name (`Puy.Zip`).
    pub unit: &'static str,
    /// The source of the unit: its heading, the `interface` part with the declarations, and an
    /// `implementation` part that writes no body for a `native` routine.
    pub source: &'static str,
    /// The fingerprint of `source` ([`fingerprint`]). The compiler writes it into the module, and
    /// the isolate refuses a module whose fingerprint differs (F0007).
    pub fingerprint: u64,
    /// The bodies of the unit's `native` routines.
    pub functions: &'static [Function],
    /// The classes whose instances hold a Rust value.
    pub classes: &'static [Class],
    /// The capabilities of the OS the extension uses directly, outside the host interface
    /// (requirements §6, the declaration of capabilities). Empty for one that only goes through
    /// [`HostSet`].
    pub capabilities: &'static [&'static str],
}

/// The body of a `native` routine.
#[derive(Debug, Clone, Copy)]
pub struct Function {
    /// The routine's name as the unit declares it: `Crc32`, or `TZipArchive.EntryCount` for a
    /// method (compared ignoring ASCII case, as Pascal names are).
    pub name: &'static str,
    pub body: Body,
    /// What a deadline or cancellation does to a suspendable call (recorded only, until M5c).
    pub cancel: CancelMode,
}

impl Function {
    /// A synchronous function.
    pub const fn sync(name: &'static str, f: SyncFn) -> Function {
        Function {
            name,
            body: Body::Sync(f),
            cancel: CancelMode::Drop,
        }
    }

    /// A suspendable function, cancelled as `cancel` says.
    pub const fn suspendable(name: &'static str, f: AsyncFn, cancel: CancelMode) -> Function {
        Function {
            name,
            body: Body::Async(f),
            cancel,
        }
    }
}

/// How a `native` routine runs (requirements §4.3).
#[derive(Clone, Copy)]
pub enum Body {
    /// Synchronous: runs to its end in the call.
    Sync(SyncFn),
    /// Suspendable: reads its arguments in the call and gives back the work to wait for, which
    /// the task waits on when it is not done at once.
    Async(AsyncFn),
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Body::Sync(_) => "Sync",
            Body::Async(_) => "Async",
        })
    }
}

/// The body of a synchronous function.
pub type SyncFn = fn(&mut CallCtx<'_>) -> Result<Value, PasError>;
/// The start of a suspendable function: it takes what it needs of the arguments (the future
/// outlives the call, so it cannot borrow them) and returns the future.
pub type AsyncFn = fn(&mut CallCtx<'_>) -> Result<ExtFuture, PasError>;
/// The work a suspendable function gives back.
pub type ExtFuture = Pin<Box<dyn Future<Output = Result<Value, PasError>> + 'static>>;

/// A class whose instances hold a Rust value of one type (`docs/design/extensions.md` 3.4).
#[derive(Debug, Clone, Copy)]
pub struct Class {
    /// The class's name as the unit declares it (`TZipArchive`).
    pub name: &'static str,
    /// The type of the value its instances hold.
    pub type_id: fn() -> TypeId,
}

impl Class {
    /// The class `name` whose instances hold a `T`.
    pub const fn of<T: 'static>(name: &'static str) -> Class {
        Class {
            name,
            type_id: cell_type_id::<T>,
        }
    }
}

/// The type the instances of a class hold their value in ([`Value::object`]).
fn cell_type_id<T: 'static>() -> TypeId {
    TypeId::of::<RefCell<T>>()
}

/// The fingerprint of a unit's source (64-bit FNV-1a of its bytes): a `const fn`, so that the
/// registration table is a `static`.
pub const fn fingerprint(source: &str) -> u64 {
    let bytes = source.as_bytes();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    h
}

/// The extensions a runtime carries (`docs/design/extensions.md` 2).
#[derive(Clone, Default)]
pub struct ExtensionSet {
    list: Vec<&'static Extension>,
}

impl ExtensionSet {
    /// A set without extensions.
    pub fn new() -> ExtensionSet {
        ExtensionSet::default()
    }

    /// Adds an extension. A unit of the same name that is already in the set stays and the new
    /// one is not added (the first registration wins).
    pub fn add(&mut self, ext: &'static Extension) -> &mut ExtensionSet {
        if self.find(ext.unit).is_none() {
            self.list.push(ext);
        }
        self
    }

    /// The set with `ext` added.
    pub fn with(mut self, ext: &'static Extension) -> ExtensionSet {
        self.add(ext);
        self
    }

    /// The extension of the unit `unit` (compared ignoring ASCII case).
    pub fn find(&self, unit: &str) -> Option<&'static Extension> {
        self.list
            .iter()
            .copied()
            .find(|e| e.unit.eq_ignore_ascii_case(unit))
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static Extension> + '_ {
        self.list.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

impl fmt::Debug for ExtensionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.list.iter().map(|e| e.unit))
            .finish()
    }
}

impl PartialEq for ExtensionSet {
    fn eq(&self, other: &Self) -> bool {
        self.list.len() == other.list.len()
            && self
                .list
                .iter()
                .zip(&other.list)
                .all(|(a, b)| std::ptr::eq(*a, *b))
    }
}

impl Eq for ExtensionSet {}

// ---- Strings ----

/// A Pascal `string` borrowed as the VM holds it (`docs/design/extensions.md` 3.5, D): one byte
/// per character when every character is below U+0080, UTF-16 code units otherwise. Lengths and
/// positions count UTF-16 code units, as Pascal's do.
#[derive(Debug, Clone, Copy)]
pub enum PasStr<'a> {
    Ascii(&'a str),
    Utf16(&'a [u16]),
}

impl<'a> PasStr<'a> {
    /// The length in UTF-16 code units (`Length`).
    pub fn len(&self) -> usize {
        match self {
            PasStr::Ascii(s) => s.len(),
            PasStr::Utf16(u) => u.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The UTF-16 code unit at the 0-based position `i`.
    pub fn unit(&self, i: usize) -> Option<u16> {
        match self {
            PasStr::Ascii(s) => s.as_bytes().get(i).map(|&b| u16::from(b)),
            PasStr::Utf16(u) => u.get(i).copied(),
        }
    }

    /// The UTF-16 code units.
    pub fn units(&self) -> impl Iterator<Item = u16> + 'a {
        let (a, u): (&'a [u8], &'a [u16]) = match *self {
            PasStr::Ascii(s) => (s.as_bytes(), &[]),
            PasStr::Utf16(u) => (&[], u),
        };
        a.iter().map(|&b| u16::from(b)).chain(u.iter().copied())
    }

    /// The characters; a lone surrogate reads as U+FFFD.
    pub fn chars(&self) -> impl Iterator<Item = char> + 'a {
        char::decode_utf16(self.units()).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
    }

    /// The text without converting, when it is one byte per character.
    pub fn as_ascii(&self) -> Option<&'a str> {
        match *self {
            PasStr::Ascii(s) => Some(s),
            PasStr::Utf16(_) => None,
        }
    }

    /// The string copied, in the same form.
    pub fn to_pas_string(&self) -> PasString {
        match *self {
            PasStr::Ascii(s) => PasString(PasStringRepr::Ascii(s.to_string())),
            PasStr::Utf16(u) => PasString(PasStringRepr::Utf16(u.to_vec())),
        }
    }
}

impl fmt::Display for PasStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasStr::Ascii(s) => f.write_str(s),
            PasStr::Utf16(_) => self.chars().try_for_each(|c| fmt::Write::write_char(f, c)),
        }
    }
}

impl PartialEq<str> for PasStr<'_> {
    fn eq(&self, other: &str) -> bool {
        match self {
            PasStr::Ascii(s) => *s == other,
            PasStr::Utf16(u) => u.iter().copied().eq(other.encode_utf16()),
        }
    }
}

/// An owned Pascal `string` in the VM's form, given back without conversion
/// (`docs/design/extensions.md` 3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasString(PasStringRepr);

#[derive(Debug, Clone, PartialEq, Eq)]
enum PasStringRepr {
    Ascii(String),
    Utf16(Vec<u16>),
}

impl PasString {
    /// The string of the UTF-16 code units `units` (lone surrogates are kept, as Pascal keeps
    /// them).
    pub fn from_utf16(units: Vec<u16>) -> PasString {
        if units.iter().all(|&u| u < 0x80) {
            let s = units.iter().map(|&u| u as u8 as char).collect();
            return PasString(PasStringRepr::Ascii(s));
        }
        PasString(PasStringRepr::Utf16(units))
    }

    /// The one-byte text, or the UTF-16 code units.
    #[doc(hidden)]
    pub fn into_parts(self) -> Result<String, Vec<u16>> {
        match self.0 {
            PasStringRepr::Ascii(s) => Ok(s),
            PasStringRepr::Utf16(u) => Err(u),
        }
    }

    /// The string borrowed as a [`PasStr`].
    pub fn as_pas_str(&self) -> PasStr<'_> {
        match &self.0 {
            PasStringRepr::Ascii(s) => PasStr::Ascii(s),
            PasStringRepr::Utf16(u) => PasStr::Utf16(u),
        }
    }
}

impl From<String> for PasString {
    fn from(s: String) -> PasString {
        if s.is_ascii() {
            PasString(PasStringRepr::Ascii(s))
        } else {
            PasString(PasStringRepr::Utf16(s.encode_utf16().collect()))
        }
    }
}

impl From<&str> for PasString {
    fn from(s: &str) -> PasString {
        PasString::from(s.to_string())
    }
}

/// A Pascal `UTF8String`, owned (`docs/spec/extensions.md` 7.3): the Rust type of a
/// `UTF8String` argument or result.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Utf8String(pub String);

impl From<Utf8String> for Value {
    fn from(v: Utf8String) -> Value {
        Value::Str(v.0)
    }
}

/// A Pascal `Char`: one UTF-16 code unit, which may be half of a surrogate pair
/// (`docs/spec/extensions.md` 7.3). Rust's `char` is a Unicode scalar value and stands for
/// `UCS4Char` instead (requirements §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct PasChar(pub u16);

impl PasChar {
    /// The character, or `None` for half of a surrogate pair.
    pub fn to_char(self) -> Option<char> {
        char::from_u32(u32::from(self.0))
    }

    /// The `Char` of `c`, or `None` past U+FFFF (two code units).
    pub fn from_char(c: char) -> Option<PasChar> {
        u16::try_from(u32::from(c)).ok().map(PasChar)
    }
}

impl From<PasChar> for Value {
    fn from(v: PasChar) -> Value {
        Value::Int(i64::from(v.0))
    }
}

/// A `UCS4Char` (requirements §6).
impl From<char> for Value {
    fn from(v: char) -> Value {
        Value::Int(i64::from(u32::from(v)))
    }
}

// ---- Values ----

/// An owned value a function gives back, or passes to a Pascal callback
/// (`docs/design/extensions.md` 3.5). The VM makes it the Pascal value of the declared type.
pub enum Value {
    /// No value (a procedure).
    Nothing,
    /// An ordinal: an integer, `Boolean` (0 or 1), `Char` (a UTF-16 code unit), an enumeration's
    /// ordinal. `UInt64` is held by its bits.
    Int(i64),
    /// `Single` or `Double`.
    Float(f64),
    /// A `string` or `UTF8String` from UTF-8.
    Str(String),
    /// A `string` in the VM's form.
    PasString(PasString),
    /// A `TBytes`.
    Bytes(Vec<u8>),
    /// A record: the values of its fields, in the order they are declared
    /// (`docs/spec/extensions.md` 3; copied each time it is passed).
    Record(Vec<Value>),
    /// A dynamic array other than `TBytes`: the values of its elements (copied each time it is
    /// passed).
    Array(Vec<Value>),
    /// A new instance of an extension class holding this value ([`Value::object`]).
    Object(Rc<dyn Any>),
    /// A Pascal value the extension holds (an instance, a callable value, …): the result takes a
    /// reference of its own.
    Ref(PasRef),
    /// `nil`.
    Nil,
}

impl Value {
    /// A new instance of the extension class whose instances hold a `T`.
    pub fn object<T: 'static>(value: T) -> Value {
        Value::Object(Rc::new(RefCell::new(value)))
    }

    /// The ordinal of an [`Value::Int`].
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// The number of a [`Value::Float`].
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// The text of a [`Value::Str`] or [`Value::PasString`] (lone surrogates become U+FFFD).
    pub fn into_string(self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s),
            Value::PasString(s) => Some(s.as_pas_str().to_string()),
            _ => None,
        }
    }

    /// A `Char` (`ERangeError` past U+FFFF, which takes two code units).
    pub fn char(c: char) -> Result<Value, PasError> {
        let mut buf = [0u16; 2];
        match c.encode_utf16(&mut buf) {
            [u] => Ok(Value::Int(i64::from(*u))),
            _ => Err(PasError::range_error(format!(
                "U+{:04X} does not fit in a Char",
                c as u32
            ))),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nothing => f.write_str("Nothing"),
            Value::Int(v) => write!(f, "Int({v})"),
            Value::Float(v) => write!(f, "Float({v})"),
            Value::Str(s) => write!(f, "Str({s:?})"),
            Value::PasString(s) => write!(f, "PasString({s:?})"),
            Value::Bytes(b) => write!(f, "Bytes({b:?})"),
            Value::Record(v) => f.debug_tuple("Record").field(v).finish(),
            Value::Array(v) => f.debug_tuple("Array").field(v).finish(),
            Value::Object(_) => f.write_str("Object"),
            Value::Ref(r) => write!(f, "Ref({})", r.raw),
            Value::Nil => f.write_str("Nil"),
        }
    }
}

impl From<()> for Value {
    fn from((): ()) -> Value {
        Value::Nothing
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Int(i64::from(v))
    }
}

macro_rules! int_value {
    ($($t:ty),*) => {$(
        impl From<$t> for Value {
            fn from(v: $t) -> Value {
                Value::Int(v as i64)
            }
        }
    )*};
}
int_value!(i8, i16, i32, i64, u8, u16, u32, u64);

impl From<f64> for Value {
    fn from(v: f64) -> Value {
        Value::Float(v)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Value {
        Value::Float(f64::from(v))
    }
}

impl From<String> for Value {
    fn from(v: String) -> Value {
        Value::Str(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Value {
        Value::Str(v.to_string())
    }
}

impl From<PasString> for Value {
    fn from(v: PasString) -> Value {
        Value::PasString(v)
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Value {
        Value::Bytes(v)
    }
}

impl From<PasRef> for Value {
    fn from(v: PasRef) -> Value {
        Value::Ref(v)
    }
}

impl<T> From<PasObj<T>> for Value {
    fn from(v: PasObj<T>) -> Value {
        Value::Ref(v.handle)
    }
}

impl<S> From<PasFn<S>> for Value {
    fn from(v: PasFn<S>) -> Value {
        Value::Ref(v.handle)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Value {
        v.map_or(Value::Nil, Into::into)
    }
}

// ---- Errors ----

/// The failure of a function: the exception the VM raises at the call
/// (`docs/design/extensions.md` 3.8).
pub enum PasError {
    /// A new exception of the class `class` (a class of the extension's unit, or one of
    /// `System` / `System.SysUtils`, by its declared name).
    Raise {
        class: Cow<'static, str>,
        message: String,
        extra: ErrorExtra,
    },
    /// An exception object a Pascal callback raised: raised again as it is (its class and stack
    /// trace kept).
    Pascal(PasRef),
    /// A callback ran `Halt`: the VM goes on with it once the function returns, whatever the
    /// function does with this error.
    Halt,
}

/// The value of the property a built-in exception class adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorExtra {
    None,
    /// `EInOutError.ErrorCode` (its `ErrorKind` is `ekNone`).
    Int(i64),
    /// `EConvertError.Value`.
    Str(String),
    /// The error of the host: `EInOutError.ErrorCode` and `ErrorKind`, or the `ErrorKind` of
    /// `ENotSupportedException` (`code` unused). `kind` is an ordinal of `TIOErrorKind`
    /// ([`io_error_kind`]).
    Host {
        code: i64,
        kind: i64,
    },
}

/// The values of `TIOErrorKind` (`System.SysUtils`; `docs/spec/exceptions.md` 2.2), in ordinal
/// order: the kind of a host error that `EInOutError` and `ENotSupportedException` carry.
pub const IO_ERROR_KINDS: [&str; 9] = [
    "ekNone",
    "ekNotFound",
    "ekPermissionDenied",
    "ekIsDirectory",
    "ekAlreadyExists",
    "ekUnsupported",
    "ekNoPermission",
    "ekInterrupted",
    "ekOther",
];

/// What the exception of a host error carries (`docs/spec/bytecode.md` chapter 4): whether it
/// is `ENotSupportedException` (a missing capability: `Unsupported`, `NoPermission`) rather than
/// `EInOutError`, its `ErrorCode` (the OS code, 0 when there is none) and its `ErrorKind`. The one
/// mapping, shared by [`PasError::host`] and the VM's own host calls.
pub fn host_exception(e: &HostError) -> (bool, i64, i64) {
    let not_supported = matches!(
        e.kind,
        HostErrorKind::Unsupported | HostErrorKind::NoPermission
    );
    (
        not_supported,
        i64::from(e.os_code.unwrap_or(0)),
        io_error_kind(e.kind),
    )
}

/// The ordinal of `TIOErrorKind` (an index of [`IO_ERROR_KINDS`]) for a host error's kind.
pub fn io_error_kind(kind: HostErrorKind) -> i64 {
    match kind {
        HostErrorKind::NotFound => 1,
        HostErrorKind::PermissionDenied => 2,
        HostErrorKind::IsDirectory => 3,
        HostErrorKind::AlreadyExists => 4,
        HostErrorKind::Unsupported => 5,
        HostErrorKind::NoPermission => 6,
        HostErrorKind::Interrupted => 7,
        HostErrorKind::Other => 8,
    }
}

impl PasError {
    /// A new exception of the class `class` with the message `message`.
    pub fn new(class: impl Into<Cow<'static, str>>, message: impl Into<String>) -> PasError {
        PasError::Raise {
            class: class.into(),
            message: message.into(),
            extra: ErrorExtra::None,
        }
    }

    /// `ERangeError`.
    pub fn range_error(message: impl Into<String>) -> PasError {
        PasError::new("ERangeError", message)
    }

    /// `EArgumentException`.
    pub fn argument(message: impl Into<String>) -> PasError {
        PasError::new("EArgumentException", message)
    }

    /// `EInvalidOperation`.
    pub fn invalid_operation(message: impl Into<String>) -> PasError {
        PasError::new("EInvalidOperation", message)
    }

    /// `EAccessViolation` of a `nil` reference.
    pub fn nil_access() -> PasError {
        PasError::new("EAccessViolation", "access to a nil reference")
    }

    /// `EInOutError` with its `ErrorCode`.
    pub fn in_out(message: impl Into<String>, code: i64) -> PasError {
        PasError::Raise {
            class: Cow::Borrowed("EInOutError"),
            message: message.into(),
            extra: ErrorExtra::Int(code),
        }
    }
}

/// An exception class an extension declares with `#[pascal_exception]`: its marker type names
/// the class.
pub trait PascalException {
    /// The class's name as the unit declares it (`EZipError`).
    const NAME: &'static str;

    /// A new exception of the class with the message `message`.
    fn error(message: impl Into<String>) -> PasError {
        PasError::new(Self::NAME, message)
    }
}

impl fmt::Debug for PasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasError::Raise { class, message, .. } => write!(f, "{class}: {message}"),
            PasError::Pascal(r) => write!(f, "Pascal({})", r.raw),
            PasError::Halt => f.write_str("Halt"),
        }
    }
}

/// The exception of a host error (`docs/spec/bytecode.md` chapter 4): the one mapping, shared by
/// the VM's own host calls and the extensions'.
impl From<HostError> for PasError {
    fn from(e: HostError) -> PasError {
        PasError::host(&e, e.message.clone())
    }
}

impl PasError {
    /// The exception of a host error with the message `message`: `ENotSupportedException` for a
    /// missing capability (`Unsupported`, `NoPermission`), `EInOutError` for the rest, with the
    /// error's kind (`ErrorKind`) and OS code (`ErrorCode`) (`docs/spec/bytecode.md` chapter 4).
    pub fn host(e: &HostError, message: impl Into<String>) -> PasError {
        let (not_supported, code, kind) = host_exception(e);
        let class = if not_supported {
            "ENotSupportedException"
        } else {
            "EInOutError"
        };
        PasError::Raise {
            class: Cow::Borrowed(class),
            message: message.into(),
            extra: ErrorExtra::Host { code, kind },
        }
    }
}

// ---- References the extension holds ----

/// What a reference held by Rust points at: a heap block (a string, an array, a closure, …) or
/// an instance's handle.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Block,
    Object,
}

/// The references Rust let go of, which the VM lets go of when it next can
/// (`docs/design/extensions.md` 3.9): dropping a [`PasRef`] cannot touch the heap.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct ReleaseQueue(RefCell<Vec<(i64, RefKind)>>);

impl ReleaseQueue {
    /// Takes the references queued so far.
    pub fn take(&self) -> Vec<(i64, RefKind)> {
        std::mem::take(&mut self.0.borrow_mut())
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }
}

/// A Pascal value held by Rust: it counts as one reference while Rust holds it, so the value
/// lives on, and the leak report counts it (requirements §6, `PasRef<T>`). Dropped, it is let go
/// of once the VM gets control back.
pub struct PasRef {
    raw: i64,
    kind: RefKind,
    /// What the VM needs to know of the value besides itself (the signature of a callable).
    tag: u32,
    queue: Rc<ReleaseQueue>,
}

impl PasRef {
    /// A reference the VM counted for Rust.
    #[doc(hidden)]
    pub fn from_raw(raw: i64, kind: RefKind, tag: u32, queue: Rc<ReleaseQueue>) -> PasRef {
        PasRef {
            raw,
            kind,
            tag,
            queue,
        }
    }

    /// The value and its tag, without giving up the reference.
    #[doc(hidden)]
    pub fn raw(&self) -> (i64, RefKind, u32) {
        (self.raw, self.kind, self.tag)
    }
}

impl Drop for PasRef {
    fn drop(&mut self) {
        self.queue.0.borrow_mut().push((self.raw, self.kind));
    }
}

/// A Pascal callable value (an anonymous method, a procedure reference, a method of an
/// instance) held by Rust, to be called with [`CallCtx::call`]. `S` names its signature for the
/// attributes: a type declared with `#[pascal_callback]` (`PasFn<IntFunc>`); the VM knows the
/// signature from the declaration, so `S` does not change what a call does.
pub struct PasFn<S = ()> {
    handle: PasRef,
    sig: PhantomData<fn() -> S>,
}

impl<S> PasFn<S> {
    fn new(handle: PasRef) -> PasFn<S> {
        PasFn {
            handle,
            sig: PhantomData,
        }
    }
}

/// An instance of an extension class whose value is a `T`, held by Rust: it keeps the instance
/// alive (`docs/design/extensions.md` 3.4).
pub struct PasObj<T> {
    cell: Rc<dyn Any>,
    handle: PasRef,
    ty: PhantomData<T>,
}

impl<T: 'static> PasObj<T> {
    fn cell(&self) -> &RefCell<T> {
        self.cell
            .downcast_ref::<RefCell<T>>()
            .expect("checked when the object was taken")
    }

    /// The value, shared: `EInvalidOperation` while a call has it mutably (a callback that
    /// re-enters the instance; requirements §6).
    pub fn borrow(&self) -> Result<Ref<'_, T>, PasError> {
        self.cell()
            .try_borrow()
            .map_err(|_| PasError::invalid_operation(REENTERED))
    }

    /// The value, mutably: `EInvalidOperation` while another call has it.
    pub fn borrow_mut(&self) -> Result<RefMut<'_, T>, PasError> {
        self.cell()
            .try_borrow_mut()
            .map_err(|_| PasError::invalid_operation(REENTERED))
    }
}

/// The message of a re-entered instance.
const REENTERED: &str = "the extension object is in use by a call in progress";

// ---- Calls ----

/// An argument that is an instance of an extension class: the value it holds, and the instance.
#[doc(hidden)]
pub type ObjectArg = (Rc<dyn Any>, PasRef);

/// What a function sees of the VM while it runs. Implemented by the VM; an extension uses it
/// through [`CallCtx`]. The argument accessors fail with an exception when the argument is not of
/// the asked kind (the declaration and the Rust code disagree).
#[doc(hidden)]
pub trait Vm {
    fn arg_count(&self) -> usize;
    fn arg_int(&self, i: usize) -> Result<i64, PasError>;
    fn arg_float(&self, i: usize) -> Result<f64, PasError>;
    fn arg_str(&self, i: usize) -> Result<PasStr<'_>, PasError>;
    /// The argument as UTF-8: a `UTF8String` or a one-byte `string` as it is, a UTF-16 `string`
    /// converted.
    fn arg_utf8(&self, i: usize) -> Result<Cow<'_, str>, PasError>;
    fn arg_bytes(&self, i: usize) -> Result<Cow<'_, [u8]>, PasError>;
    /// The argument copied into an owned value: an ordinal, a real, a string, a `TBytes`, or a
    /// record or a dynamic array with the values of its fields or elements.
    fn arg_value(&self, i: usize) -> Result<Value, PasError>;
    /// The value an instance of an extension class holds, and the instance, counted (`None`:
    /// `nil`).
    fn arg_object(&mut self, i: usize) -> Result<Option<ObjectArg>, PasError>;
    /// A managed argument, counted (`None`: `nil`).
    fn arg_ref(&mut self, i: usize) -> Result<Option<PasRef>, PasError>;
    fn host(&mut self) -> &mut HostSet;
    fn isolate_memory(&self) -> IsolateMemory;
    fn reset_isolate_peak(&mut self);
    fn isolate_live_types(&self) -> Vec<IsolateTypeUsage>;
    fn isolate_live_block_count(&self) -> usize;
    fn isolate_live_block_bytes(&self) -> usize;
    /// The isolate's changes to the environment variables, and the host they overlay.
    fn env(&mut self) -> (&mut EnvTable, &mut HostSet);
    /// Calls a Pascal callable value with `args` and returns its result: runs the VM until it
    /// returns (`docs/design/extensions.md` 3.9).
    fn call(&mut self, f: &PasRef, args: Vec<Value>) -> Result<Value, PasError>;
    /// The Pascal routines running, innermost first.
    fn frames(&self) -> Vec<Frame>;
}

/// Memory counted toward an isolate's VM heap limit, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsolateMemory {
    pub used: usize,
    pub peak: usize,
    pub limit: Option<usize>,
}

/// The current isolate's live heap blocks with one reported type name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolateTypeUsage {
    /// The type name as it appears in a leak report.
    pub type_name: String,
    pub count: usize,
    /// The size of the blocks as counted toward the isolate's memory limit.
    pub bytes: usize,
}

/// A Pascal routine running at the call of a `native` routine, as a stack trace shows it
/// ([`CallCtx::frames`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The routine's full name (`Unit.Name`, `Unit.TClass.Method`).
    pub function: String,
    /// The path of its source file, as the stack trace shows it.
    pub file: String,
    /// The line it is at: the line of the call it made, or of the call of the `native` routine
    /// for the innermost.
    pub line: u32,
}

/// The context of a call of a `native` routine: its arguments, numbered from 0 (`Self` first
/// for a method), the host, and calls back into Pascal.
pub struct CallCtx<'a> {
    vm: &'a mut dyn Vm,
}

impl<'a> CallCtx<'a> {
    #[doc(hidden)]
    pub fn new(vm: &'a mut dyn Vm) -> CallCtx<'a> {
        CallCtx { vm }
    }

    /// The number of arguments.
    pub fn arg_count(&self) -> usize {
        self.vm.arg_count()
    }

    /// An ordinal argument (an integer, `Char`, an enumeration's ordinal).
    pub fn int(&self, i: usize) -> Result<i64, PasError> {
        self.vm.arg_int(i)
    }

    /// An integer argument converted to `T` (`ERangeError` when it does not fit).
    pub fn int_as<T: TryFrom<i64>>(&self, i: usize) -> Result<T, PasError> {
        let v = self.vm.arg_int(i)?;
        T::try_from(v).map_err(|_| PasError::range_error(format!("{v} is out of range")))
    }

    /// A `Boolean` argument.
    pub fn boolean(&self, i: usize) -> Result<bool, PasError> {
        Ok(self.vm.arg_int(i)? != 0)
    }

    /// A `Char` argument (`ERangeError` for a lone surrogate).
    pub fn char(&self, i: usize) -> Result<char, PasError> {
        let v = self.vm.arg_int(i)?;
        u32::try_from(v)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| PasError::range_error(format!("#{v} is not a character")))
    }

    /// A `Single` or `Double` argument.
    pub fn float(&self, i: usize) -> Result<f64, PasError> {
        self.vm.arg_float(i)
    }

    /// A `string` argument as the VM holds it, without copying.
    pub fn str(&self, i: usize) -> Result<PasStr<'_>, PasError> {
        self.vm.arg_str(i)
    }

    /// A `string` or `UTF8String` argument as UTF-8: borrowed when it is a `UTF8String` or a
    /// one-byte `string`, converted from a UTF-16 `string`.
    pub fn utf8(&self, i: usize) -> Result<Cow<'_, str>, PasError> {
        self.vm.arg_utf8(i)
    }

    /// A `string` or `UTF8String` argument, copied.
    pub fn string(&self, i: usize) -> Result<String, PasError> {
        self.vm.arg_utf8(i).map(Cow::into_owned)
    }

    /// A `TBytes` argument.
    pub fn bytes(&self, i: usize) -> Result<Cow<'_, [u8]>, PasError> {
        self.vm.arg_bytes(i)
    }

    /// An argument copied into an owned value: a record ([`Value::Record`]) or a dynamic array
    /// ([`Value::Array`]) with the values of its fields or elements, or a value of any other kind
    /// but an instance and a callable value (`Value::PasString` for a `string`).
    pub fn value(&self, i: usize) -> Result<Value, PasError> {
        self.vm.arg_value(i)
    }

    /// An instance of the extension class whose value is a `T` (`EAccessViolation` for `nil`).
    pub fn object<T: 'static>(&mut self, i: usize) -> Result<PasObj<T>, PasError> {
        self.object_opt(i)?.ok_or_else(PasError::nil_access)
    }

    /// An instance of the extension class whose value is a `T`, or `None` for `nil`.
    pub fn object_opt<T: 'static>(&mut self, i: usize) -> Result<Option<PasObj<T>>, PasError> {
        let Some((cell, handle)) = self.vm.arg_object(i)? else {
            return Ok(None);
        };
        if !cell.is::<RefCell<T>>() {
            return Err(PasError::new(
                "EInvalidCast",
                "the extension object does not hold the expected Rust type",
            ));
        }
        Ok(Some(PasObj {
            cell,
            handle,
            ty: PhantomData,
        }))
    }

    /// `Self` of a method.
    pub fn this<T: 'static>(&mut self) -> Result<PasObj<T>, PasError> {
        self.object(0)
    }

    /// A callable argument (an anonymous method or a procedural value), or `None` for `nil`.
    pub fn func(&mut self, i: usize) -> Result<Option<PasFn>, PasError> {
        Ok(self.vm.arg_ref(i)?.map(PasFn::new))
    }

    /// A managed argument (an instance, an array, …) held by Rust, or `None` for `nil`.
    pub fn reference(&mut self, i: usize) -> Result<Option<PasRef>, PasError> {
        self.vm.arg_ref(i)
    }

    /// The host of the isolate (`docs/design/extensions.md` 3.6): the capabilities the embedder
    /// granted.
    pub fn host(&mut self) -> &mut HostSet {
        self.vm.host()
    }

    /// The current isolate's usage, peak and optional limit.
    pub fn isolate_memory(&self) -> IsolateMemory {
        self.vm.isolate_memory()
    }

    /// Resets the current isolate's peak to its usage at this call.
    pub fn reset_isolate_peak(&mut self) {
        self.vm.reset_isolate_peak();
    }

    /// Scans the current isolate's heap and groups its live blocks by type, in unspecified order.
    pub fn isolate_live_types(&self) -> Vec<IsolateTypeUsage> {
        self.vm.isolate_live_types()
    }

    /// Scans the current isolate's heap and counts the live blocks included in `isolate_live_types`.
    pub fn isolate_live_block_count(&self) -> usize {
        self.vm.isolate_live_block_count()
    }

    /// Sums the accounted bytes of the live blocks included in `isolate_live_types`.
    pub fn isolate_live_block_bytes(&self) -> usize {
        self.vm.isolate_live_block_bytes()
    }

    /// The environment variable `name` as the isolate sees it: a value the isolate set itself,
    /// else the host's (`None` when unset or without an `Env`; [`EnvTable::get`]).
    pub fn env_var(&mut self, name: &str) -> Result<Option<String>, PasError> {
        let (table, host) = self.vm.env();
        table.get(host, name).map_err(PasError::from)
    }

    /// Sets the environment variable `name` to `value`, or removes it (`None`), for this isolate
    /// only: the process's variables do not change ([`EnvTable::set`]). An invalid name
    /// ([`is_env_name`]) or a value with `#0` is `EArgumentException`.
    pub fn set_env_var(&mut self, name: &str, value: Option<&str>) -> Result<(), PasError> {
        let (table, host) = self.vm.env();
        table.set(host, name, value)
    }

    /// Every environment variable as the isolate sees it ([`EnvTable::vars`]).
    pub fn env_vars(&mut self) -> Result<Vec<(String, String)>, PasError> {
        let (table, host) = self.vm.env();
        table.vars(host).map_err(PasError::from)
    }

    /// The Pascal routines running at this call, innermost first (the one that called the
    /// `native` routine, at the line of that call), as the stack trace of an exception raised
    /// here would show them: the frames of inlined calls are there, those of destructors are not
    /// (`docs/spec/diagnostics.md` chapter 8).
    pub fn frames(&self) -> Vec<Frame> {
        self.vm.frames()
    }

    /// Calls the Pascal callable value `f` with `args` and returns its result
    /// (`docs/design/extensions.md` 3.9). An exception the callback does not handle comes back
    /// as [`PasError::Pascal`]; returning it with `?` raises it on as it is.
    pub fn call<S>(&mut self, f: &PasFn<S>, args: Vec<Value>) -> Result<Value, PasError> {
        self.vm.call(&f.handle, args)
    }
}

/// What the code the attributes generate uses; not a stable API.
#[doc(hidden)]
pub mod __private {
    use super::*;

    /// An argument read as the Rust type of a `native` routine's parameter.
    pub trait FromArg: Sized {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<Self, PasError>;
    }

    macro_rules! int_arg {
        ($($t:ty),*) => {$(
            impl FromArg for $t {
                fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<$t, PasError> {
                    ctx.int_as::<$t>(i)
                }
            }
        )*};
    }
    int_arg!(i8, i16, i32, i64, u8, u16, u32);

    /// `UInt64` is held by its bits.
    impl FromArg for u64 {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<u64, PasError> {
            Ok(ctx.int(i)? as u64)
        }
    }

    impl FromArg for bool {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<bool, PasError> {
            ctx.boolean(i)
        }
    }

    impl FromArg for f64 {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<f64, PasError> {
            ctx.float(i)
        }
    }

    impl FromArg for f32 {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<f32, PasError> {
            Ok(ctx.float(i)? as f32)
        }
    }

    /// A `UCS4Char`: `ERangeError` when it is not a Unicode scalar value.
    impl FromArg for char {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<char, PasError> {
            let v = ctx.int(i)?;
            u32::try_from(v)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| PasError::range_error(format!("{v} is not a character")))
        }
    }

    impl FromArg for PasChar {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<PasChar, PasError> {
            ctx.int_as::<u16>(i).map(PasChar)
        }
    }

    impl FromArg for String {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<String, PasError> {
            ctx.string(i)
        }
    }

    impl FromArg for Utf8String {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<Utf8String, PasError> {
            ctx.string(i).map(Utf8String)
        }
    }

    impl FromArg for PasString {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<PasString, PasError> {
            Ok(ctx.str(i)?.to_pas_string())
        }
    }

    /// A dynamic array: a `TBytes` for `Vec<u8>`, a `TArray<T>` for any other element type.
    impl<T: FromValue> FromArg for Vec<T> {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<Vec<T>, PasError> {
            T::vec_from_arg(ctx, i)
        }
    }

    /// The name of the kind of a value, for the error of a value of another kind than declared.
    fn kind_name(v: &Value) -> &'static str {
        match v {
            Value::Nothing => "no value",
            Value::Int(_) => "an ordinal",
            Value::Float(_) => "a real",
            Value::Str(_) | Value::PasString(_) => "a string",
            Value::Bytes(_) => "a TBytes",
            Value::Record(_) => "a record",
            Value::Array(_) => "a dynamic array",
            Value::Object(_) | Value::Ref(_) => "an instance",
            Value::Nil => "nil",
        }
    }

    /// A field or an element of another kind than its Rust type (the declaration and the
    /// extension disagree).
    pub fn value_mismatch(v: &Value, want: &str) -> PasError {
        PasError::new(
            "EInvalidCast",
            format!(
                "{} is not {want} (the declaration and the extension disagree)",
                kind_name(v)
            ),
        )
    }

    /// A field of a record or an element of a dynamic array read as the Rust type of a record's
    /// field or of a `Vec`'s element (`docs/spec/extensions.md` 7.3).
    pub trait FromValue: Sized {
        fn from_value(v: Value) -> Result<Self, PasError>;

        /// A `Vec` of this type from a dynamic array.
        fn vec_from_value(v: Value) -> Result<Vec<Self>, PasError> {
            match v {
                Value::Array(items) => items.into_iter().map(Self::from_value).collect(),
                Value::Nil => Ok(Vec::new()),
                v => Err(value_mismatch(&v, "a dynamic array")),
            }
        }

        /// A `Vec` of this type from argument `i`.
        fn vec_from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<Vec<Self>, PasError> {
            Self::vec_from_value(ctx.value(i)?)
        }
    }

    /// A Rust value as a field of a record or an element of a dynamic array, or as the result of
    /// a routine that returns a `Vec` (`docs/spec/extensions.md` 7.3).
    pub trait IntoValue: Sized {
        fn into_value(self) -> Value;

        /// A dynamic array of values of this type.
        fn vec_into_value(v: Vec<Self>) -> Value {
            Value::Array(v.into_iter().map(IntoValue::into_value).collect())
        }
    }

    /// The ordinal of a field or an element (an enumeration's, for the code generated for
    /// `#[pascal_enum]`).
    pub fn int_value(v: Value) -> Result<i64, PasError> {
        match v {
            Value::Int(i) => Ok(i),
            v => Err(value_mismatch(&v, "an ordinal")),
        }
    }

    macro_rules! int_conv {
        ($($t:ty),*) => {$(
            impl FromValue for $t {
                fn from_value(v: Value) -> Result<$t, PasError> {
                    let i = int_value(v)?;
                    <$t>::try_from(i).map_err(|_| PasError::range_error(format!("{i} is out of range")))
                }
            }
            impl IntoValue for $t {
                fn into_value(self) -> Value {
                    self.into()
                }
            }
        )*};
    }
    int_conv!(i8, i16, i32, i64, u16, u32);

    /// `TBytes` is one byte per element: read and made without converting each element.
    impl FromValue for u8 {
        fn from_value(v: Value) -> Result<u8, PasError> {
            let i = int_value(v)?;
            u8::try_from(i).map_err(|_| PasError::range_error(format!("{i} is out of range")))
        }

        fn vec_from_value(v: Value) -> Result<Vec<u8>, PasError> {
            match v {
                Value::Bytes(b) => Ok(b),
                Value::Nil => Ok(Vec::new()),
                v => Err(value_mismatch(&v, "a TBytes")),
            }
        }

        fn vec_from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<Vec<u8>, PasError> {
            Ok(ctx.bytes(i)?.into_owned())
        }
    }

    impl IntoValue for u8 {
        fn into_value(self) -> Value {
            self.into()
        }

        fn vec_into_value(v: Vec<u8>) -> Value {
            Value::Bytes(v)
        }
    }

    /// `UInt64` is held by its bits.
    impl FromValue for u64 {
        fn from_value(v: Value) -> Result<u64, PasError> {
            Ok(int_value(v)? as u64)
        }
    }

    impl FromValue for bool {
        fn from_value(v: Value) -> Result<bool, PasError> {
            Ok(int_value(v)? != 0)
        }
    }

    impl FromValue for f64 {
        fn from_value(v: Value) -> Result<f64, PasError> {
            match v {
                Value::Float(f) => Ok(f),
                v => Err(value_mismatch(&v, "a real")),
            }
        }
    }

    impl FromValue for f32 {
        fn from_value(v: Value) -> Result<f32, PasError> {
            f64::from_value(v).map(|f| f as f32)
        }
    }

    /// A `UCS4Char`: `ERangeError` when it is not a Unicode scalar value.
    impl FromValue for char {
        fn from_value(v: Value) -> Result<char, PasError> {
            let i = int_value(v)?;
            u32::try_from(i)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| PasError::range_error(format!("{i} is not a character")))
        }
    }

    impl FromValue for PasChar {
        fn from_value(v: Value) -> Result<PasChar, PasError> {
            u16::from_value(v).map(PasChar)
        }
    }

    impl FromValue for String {
        fn from_value(v: Value) -> Result<String, PasError> {
            match v {
                Value::Str(_) | Value::PasString(_) => Ok(v.into_string().expect("a string value")),
                v => Err(value_mismatch(&v, "a string")),
            }
        }
    }

    impl FromValue for Utf8String {
        fn from_value(v: Value) -> Result<Utf8String, PasError> {
            String::from_value(v).map(Utf8String)
        }
    }

    impl FromValue for PasString {
        fn from_value(v: Value) -> Result<PasString, PasError> {
            match v {
                Value::PasString(s) => Ok(s),
                Value::Str(s) => Ok(PasString::from(s)),
                v => Err(value_mismatch(&v, "a string")),
            }
        }
    }

    macro_rules! into_value {
        ($($t:ty),*) => {$(
            impl IntoValue for $t {
                fn into_value(self) -> Value {
                    self.into()
                }
            }
        )*};
    }
    into_value!(
        u64, bool, f64, f32, char, PasChar, String, Utf8String, PasString
    );

    impl<T: FromValue> FromValue for Vec<T> {
        fn from_value(v: Value) -> Result<Vec<T>, PasError> {
            T::vec_from_value(v)
        }
    }

    impl<T: IntoValue> IntoValue for Vec<T> {
        fn into_value(self) -> Value {
            T::vec_into_value(self)
        }
    }

    /// The fields of a record value, for the code generated for `#[pascal_record]`: `n` values
    /// in the order of the fields.
    pub fn record_fields(v: Value, n: usize) -> Result<std::vec::IntoIter<Value>, PasError> {
        match v {
            Value::Record(f) if f.len() == n => Ok(f.into_iter()),
            v => Err(value_mismatch(&v, &format!("a record of {n} fields"))),
        }
    }

    /// The next field of a record value ([`record_fields`]).
    pub fn field<T: FromValue>(it: &mut std::vec::IntoIter<Value>) -> Result<T, PasError> {
        T::from_value(it.next().expect("as many fields as declared"))
    }

    /// An argument read through its value (a record), for the code generated for
    /// `#[pascal_record]`.
    pub fn value_arg<T: FromValue>(ctx: &mut CallCtx<'_>, i: usize) -> Result<T, PasError> {
        T::from_value(ctx.value(i)?)
    }

    /// An argument `&[T]`, copied into the `Vec` it borrows.
    pub fn slice_arg<T: FromValue>(ctx: &mut CallCtx<'_>, i: usize) -> Result<Vec<T>, PasError> {
        T::vec_from_arg(ctx, i)
    }

    impl<T: 'static> FromArg for PasObj<T> {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<PasObj<T>, PasError> {
            ctx.object(i)
        }
    }

    impl<T: 'static> FromArg for Option<PasObj<T>> {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<Option<PasObj<T>>, PasError> {
            ctx.object_opt(i)
        }
    }

    /// `EAccessViolation` for `nil`.
    impl<S> FromArg for PasFn<S> {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<PasFn<S>, PasError> {
            ctx.reference(i)?
                .map(PasFn::new)
                .ok_or_else(PasError::nil_access)
        }
    }

    impl<S> FromArg for Option<PasFn<S>> {
        fn from_arg(ctx: &mut CallCtx<'_>, i: usize) -> Result<Option<PasFn<S>>, PasError> {
            Ok(ctx.reference(i)?.map(PasFn::new))
        }
    }

    /// The ordinal of an enumeration's value, for the code generated for `#[pascal_enum]`.
    pub fn enum_ordinal(ctx: &mut CallCtx<'_>, i: usize) -> Result<i64, PasError> {
        ctx.int(i)
    }

    /// An ordinal outside the enumeration (the declaration and the extension disagree).
    pub fn enum_out_of_range(name: &str, v: i64) -> PasError {
        PasError::range_error(format!("{v} is not a value of `{name}`"))
    }

    /// The result of a routine: its value, or its error as the exception.
    pub fn ok<T: Into<Value>>(v: T) -> Result<Value, PasError> {
        Ok(v.into())
    }

    /// The result of a routine that returns a `Vec` (or another value converted with
    /// [`IntoValue`]).
    pub fn ok_value<T: IntoValue>(v: T) -> Result<Value, PasError> {
        Ok(v.into_value())
    }

    /// The result of a routine that returns a `Result` of a `Vec`.
    pub fn result_value<T: IntoValue, E: Into<PasError>>(
        r: Result<T, E>,
    ) -> Result<Value, PasError> {
        match r {
            Ok(v) => Ok(v.into_value()),
            Err(e) => Err(e.into()),
        }
    }

    /// The result of a routine that returns a `Result`.
    pub fn result<T: Into<Value>, E: Into<PasError>>(r: Result<T, E>) -> Result<Value, PasError> {
        match r {
            Ok(v) => Ok(v.into()),
            Err(e) => Err(e.into()),
        }
    }

    /// The work of a suspendable routine.
    pub fn future<F>(f: F) -> Result<ExtFuture, PasError>
    where
        F: Future<Output = Result<Value, PasError>> + 'static,
    {
        Ok(Box::pin(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The fingerprint is FNV-1a 64: the empty text is the offset basis, and one byte changes it.
    #[test]
    fn fingerprint_is_fnv1a() {
        assert_eq!(fingerprint(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fingerprint("a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(fingerprint("unit A;"), fingerprint("unit B;"));
    }

    // A string of only ASCII characters is kept one byte per character, any other in UTF-16.
    #[test]
    fn pas_string_picks_the_representation() {
        assert_eq!(PasString::from("abc").as_pas_str().as_ascii(), Some("abc"));
        let s = PasString::from("あい");
        assert_eq!(s.as_pas_str().len(), 2);
        assert_eq!(s.as_pas_str().to_string(), "あい");
        assert!(s.as_pas_str() == *"あい");
    }

    // A lone surrogate reads as U+FFFD; a character past U+FFFF counts two code units.
    #[test]
    fn pas_str_counts_utf16_units() {
        let units = [0xD83D, 0xDE00, 0xD800, 0x41];
        let s = PasStr::Utf16(&units);
        assert_eq!(s.len(), 4);
        assert_eq!(s.chars().collect::<String>(), "😀\u{FFFD}A");
    }

    // A character that takes two UTF-16 code units does not fit in a `Char`.
    #[test]
    fn char_value_range() {
        assert!(matches!(Value::char('あ'), Ok(Value::Int(0x3042))));
        assert!(matches!(Value::char('😀'), Err(PasError::Raise { .. })));
    }

    // Host errors map as `docs/spec/bytecode.md` chapter 4 says.
    #[test]
    fn host_errors_map_to_exceptions() {
        let e = PasError::from(HostError::unsupported("the file system is not available"));
        assert!(
            matches!(e, PasError::Raise { ref class, ref message, extra: ErrorExtra::Host { kind: 5, .. } }
            if class == "ENotSupportedException" && message == "the file system is not available")
        );
        let e = PasError::from(HostError::new(
            HostErrorKind::NoPermission,
            "the file system is not permitted by the host",
        ));
        assert!(
            matches!(e, PasError::Raise { ref class, ref message, extra: ErrorExtra::Host { kind: 6, .. } }
            if class == "ENotSupportedException" && message == "the file system is not permitted by the host")
        );
        let e = PasError::from(HostError::other("disk full").with_os_code(Some(28)));
        assert!(
            matches!(e, PasError::Raise { ref class, extra: ErrorExtra::Host { code: 28, kind: 8 }, .. }
            if class == "EInOutError")
        );
    }

    // Every kind of host error has its own value of `TIOErrorKind`, named after it.
    #[test]
    fn io_error_kinds_match_host_error_kinds() {
        use HostErrorKind as K;
        for (kind, name) in [
            (K::NotFound, "ekNotFound"),
            (K::PermissionDenied, "ekPermissionDenied"),
            (K::IsDirectory, "ekIsDirectory"),
            (K::AlreadyExists, "ekAlreadyExists"),
            (K::Unsupported, "ekUnsupported"),
            (K::NoPermission, "ekNoPermission"),
            (K::Interrupted, "ekInterrupted"),
            (K::Other, "ekOther"),
        ] {
            assert_eq!(
                IO_ERROR_KINDS[io_error_kind(kind) as usize],
                name,
                "{kind:?}"
            );
        }
    }

    // A dropped reference is queued, not let go of.
    #[test]
    fn dropped_refs_are_queued() {
        let q = Rc::new(ReleaseQueue::default());
        drop(PasRef::from_raw(7, RefKind::Object, 0, Rc::clone(&q)));
        assert_eq!(q.take(), vec![(7, RefKind::Object)]);
        assert!(q.is_empty());
    }
}
