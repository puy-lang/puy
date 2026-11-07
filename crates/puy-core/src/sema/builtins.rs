//! Tables of the built-ins in `System` and `System.SysUtils` (`docs/spec/semantics.md` chapters 5 and 8).

use super::hir::Builtin;
use super::types::Ty;
use crate::diagnostics::Code;

/// Built-in procedures (used as statements).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinProc {
    Write,
    WriteLn,
    ReadLn,
    Inc,
    Dec,
    Assert,
    Halt,
    Exit,
    Break,
    Continue,
}

/// A name in `System`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemName {
    Type(Ty),
    True,
    False,
    Func(Builtin),
    Proc(BuiltinProc),
    /// Reserved for M2 or later (the planned milestone).
    Reserved(&'static str),
    /// A removed name (with its code).
    Deleted(Code),
}

pub fn lookup_system(lower: &str) -> Option<SystemName> {
    if let Some(t) = Ty::from_name(lower) {
        return Some(SystemName::Type(t));
    }
    Some(match lower {
        "true" => SystemName::True,
        "false" => SystemName::False,
        "length" => SystemName::Func(Builtin::Length),
        "copy" => SystemName::Func(Builtin::Copy),
        "pos" => SystemName::Func(Builtin::Pos),
        "ord" => SystemName::Func(Builtin::Ord),
        "chr" => SystemName::Func(Builtin::Chr),
        "abs" => SystemName::Func(Builtin::Abs),
        "paramcount" => SystemName::Func(Builtin::ParamCount),
        "paramstr" => SystemName::Func(Builtin::ParamStr),
        "write" => SystemName::Proc(BuiltinProc::Write),
        "writeln" => SystemName::Proc(BuiltinProc::WriteLn),
        "readln" => SystemName::Proc(BuiltinProc::ReadLn),
        "inc" => SystemName::Proc(BuiltinProc::Inc),
        "dec" => SystemName::Proc(BuiltinProc::Dec),
        "assert" => SystemName::Proc(BuiltinProc::Assert),
        "halt" => SystemName::Proc(BuiltinProc::Halt),
        "exit" => SystemName::Proc(BuiltinProc::Exit),
        "break" => SystemName::Proc(BuiltinProc::Break),
        "continue" => SystemName::Proc(BuiltinProc::Continue),
        _ => {
            if let Some(m) = reserved_milestone(lower) {
                return Some(SystemName::Reserved(m));
            }
            if let Some(c) = deleted_code(lower) {
                return Some(SystemName::Deleted(c));
            }
            return None;
        }
    })
}

/// Names in `System.SysUtils` (within M1).
pub fn lookup_sysutils(lower: &str) -> Option<Builtin> {
    Some(match lower {
        "inttostr" => Builtin::IntToStr,
        "strtoint" => Builtin::StrToInt,
        "strtoint64" => Builtin::StrToInt64,
        "strtointdef" => Builtin::StrToIntDef,
        "uppercase" => Builtin::UpperCase,
        "lowercase" => Builtin::LowerCase,
        "trim" => Builtin::Trim,
        "getenvironmentvariable" => Builtin::GetEnvironmentVariable,
        _ => return None,
    })
}

/// Names reserved for M2 or later (the table in chapter 5).
fn reserved_milestone(lower: &str) -> Option<&'static str> {
    const M2: &[&str] = &[
        "low",
        "high",
        "sizeof",
        "succ",
        "pred",
        "round",
        "trunc",
        "odd",
        "sqr",
        "sqrt",
        "pi",
        "maxint",
        "exitcode",
        "setlength",
        "insert",
        "delete",
        "concat",
        "upcase",
        "include",
        "exclude",
        "input",
        "output",
        "erroutput",
        "eof",
        "eoln",
        "read",
        "flush",
        "slinebreak",
        "exception",
        "format",
        "inttohex",
        "strtobool",
        "booltostr",
        "trystrtoint",
        "trimleft",
        "trimright",
        "sametext",
        "comparestr",
        "comparetext",
        "stringreplace",
        "stringofchar",
        "quotedstr",
        "floattostr",
        "strtofloat",
        "single",
        "double",
        "extended",
        "currency",
        "real",
        "utf8string",
        "widechar",
        "bytebool",
        "wordbool",
        "longbool",
        "ucs4char",
        "default",
        "exceptobject",
        "erangeerror",
        "eintoverflow",
        "edivbyzero",
        "econverterror",
        "einouterror",
        "eassertionfailed",
        "estackoverflow",
        "eoutofmemory",
        "enotsupported",
    ];
    const M3: &[&str] = &["assigned"];
    const M5: &[&str] = &[
        "random",
        "randomize",
        "randseed",
        "now",
        "date",
        "time",
        "sleep",
    ];
    if M2.contains(&lower) {
        Some("M2")
    } else if M3.contains(&lower) {
        Some("M3")
    } else if M5.contains(&lower) {
        Some("M5")
    } else {
        None
    }
}

/// Removed names (the table in chapter 8).
fn deleted_code(lower: &str) -> Option<Code> {
    Some(match lower {
        "shortstring" | "ansistring" | "widestring" | "rawbytestring" | "pchar" | "pansichar"
        | "pwidechar" | "ansichar" => Code::E(3003),
        "pointer" | "new" | "dispose" | "getmem" | "freemem" => Code::E(3004),
        "text" | "textfile" | "assignfile" | "closefile" | "reset" | "rewrite" | "append"
        | "blockread" | "blockwrite" | "ioresult" => Code::E(3006),
        "variant" | "olevariant" => Code::E(3007),
        "real48" | "comp" => Code::E(3013),
        "tvarrec" => Code::E(3015),
        "acquireexceptionobject" | "releaseexceptionobject" => Code::E(3016),
        _ => return None,
    })
}

/// `notes` for removed features (`docs/spec/diagnostics.md` 4.10).
pub fn deleted_note(code: Code) -> &'static str {
    match code {
        Code::E(3000) => "use an inline variable (`var R := …;`) and qualify the fields with it",
        Code::E(3001) => "use `Break`, `Continue`, `Exit` or an exception instead",
        Code::E(3002) => "use a `record` or a `class` instead",
        Code::E(3003) => {
            "use `string`; for bytes use `TBytes`, and for UTF-8 data use `UTF8String`"
        }
        Code::E(3004) => "use a class, a dynamic array or a `var` parameter instead",
        Code::E(3006) => "use streams or `TFile` instead",
        Code::E(3007) => "Puy has no `Variant`; use a specific type",
        Code::E(3010) => "Puy does not support COM",
        Code::E(3011) => "Puy has no DLLs; extend it with the Rust extension mechanism instead",
        Code::E(3013) => "use `Double` or `Int64` instead",
        Code::E(3015) => "use `TValue` instead",
        Code::E(3016) => {
            "store a reference to the exception in a variable instead; exception objects are reference counted"
        }
        _ => "",
    }
}

/// Parameters (name and type) and return type of a built-in function.
pub fn builtin_signature(func: Builtin) -> (&'static [(&'static str, Ty)], Ty) {
    match func {
        Builtin::Length => (&[("S", Ty::String)], Ty::Integer),
        Builtin::Copy => (
            &[
                ("S", Ty::String),
                ("Index", Ty::Integer),
                ("Count", Ty::Integer),
            ],
            Ty::String,
        ),
        Builtin::Pos => (&[("Sub", Ty::String), ("S", Ty::String)], Ty::Integer),
        Builtin::Ord => (&[("X", Ty::Error)], Ty::Integer),
        Builtin::Chr => (&[("X", Ty::Word)], Ty::Char),
        Builtin::Abs => (&[("X", Ty::Error)], Ty::Error),
        Builtin::ParamCount => (&[], Ty::Integer),
        Builtin::ParamStr => (&[("I", Ty::Integer)], Ty::String),
        Builtin::IntToStr => (&[("X", Ty::Int64)], Ty::String),
        Builtin::UIntToStr => (&[("X", Ty::UInt64)], Ty::String),
        Builtin::StrToInt => (&[("S", Ty::String)], Ty::Integer),
        Builtin::StrToInt64 => (&[("S", Ty::String)], Ty::Int64),
        Builtin::StrToIntDef => (&[("S", Ty::String), ("Default", Ty::Integer)], Ty::Integer),
        Builtin::UpperCase | Builtin::LowerCase | Builtin::Trim => {
            (&[("S", Ty::String)], Ty::String)
        }
        Builtin::GetEnvironmentVariable => (&[("Name", Ty::String)], Ty::String),
    }
}

/// Display name.
pub fn builtin_name(func: Builtin) -> &'static str {
    match func {
        Builtin::Length => "Length",
        Builtin::Copy => "Copy",
        Builtin::Pos => "Pos",
        Builtin::Ord => "Ord",
        Builtin::Chr => "Chr",
        Builtin::Abs => "Abs",
        Builtin::ParamCount => "ParamCount",
        Builtin::ParamStr => "ParamStr",
        Builtin::IntToStr | Builtin::UIntToStr => "IntToStr",
        Builtin::StrToInt => "StrToInt",
        Builtin::StrToInt64 => "StrToInt64",
        Builtin::StrToIntDef => "StrToIntDef",
        Builtin::UpperCase => "UpperCase",
        Builtin::LowerCase => "LowerCase",
        Builtin::Trim => "Trim",
        Builtin::GetEnvironmentVariable => "GetEnvironmentVariable",
    }
}

pub fn proc_name(p: BuiltinProc) -> &'static str {
    match p {
        BuiltinProc::Write => "Write",
        BuiltinProc::WriteLn => "WriteLn",
        BuiltinProc::ReadLn => "ReadLn",
        BuiltinProc::Inc => "Inc",
        BuiltinProc::Dec => "Dec",
        BuiltinProc::Assert => "Assert",
        BuiltinProc::Halt => "Halt",
        BuiltinProc::Exit => "Exit",
        BuiltinProc::Break => "Break",
        BuiltinProc::Continue => "Continue",
    }
}

/// Whether this is the name of a Delphi OS / GUI unit (the `notes` variant of E2500).
pub fn is_delphi_os_unit(lower: &str) -> bool {
    matches!(lower, "windows" | "messages" | "shellapi")
        || [
            "winapi.",
            "vcl.",
            "fmx.",
            "posix.",
            "macapi.",
            "androidapi.",
            "iosapi.",
        ]
        .iter()
        .any(|p| lower.starts_with(p))
}
