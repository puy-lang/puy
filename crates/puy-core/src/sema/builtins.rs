//! Tables of the built-ins in `System` and `System.SysUtils` (`docs/spec/semantics.md` chapters 5 and 8).

use super::hir::Builtin;
use super::types::{Ty, TypeId};
use crate::diagnostics::Code;

/// Built-in procedures (used as statements).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinProc {
    Write,
    WriteLn,
    ReadLn,
    /// `Read` (`docs/spec/semantics.md` 6.2).
    Read,
    /// `Flush(Output)` / `Flush(ErrOutput)` (`docs/spec/semantics.md` 6.1).
    Flush,
    Inc,
    Dec,
    Assert,
    Halt,
    Exit,
    Break,
    Continue,
    /// `SetLength` (`docs/spec/types.md` 2.3, chapter 7).
    SetLength,
    /// `Include` / `Exclude` (`docs/spec/types.md` 8.3).
    Include,
    Exclude,
    /// `Insert` / `Delete` of a string or a dynamic array (`docs/spec/semantics.md` 5.3,
    /// `docs/spec/types.md` 8.6).
    Insert,
    Delete,
}

/// The special variables `Input`, `Output` and `ErrOutput` of `System` (`docs/spec/semantics.md`
/// 6.1). They are only allowed as the first argument of the standard I/O routines (E0740).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdFile {
    Input,
    Output,
    ErrOutput,
}

impl StdFile {
    pub fn name(self) -> &'static str {
        match self {
            StdFile::Input => "Input",
            StdFile::Output => "Output",
            StdFile::ErrOutput => "ErrOutput",
        }
    }
}

/// A name in `System`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemName {
    Type(Ty),
    True,
    False,
    Func(Builtin),
    Proc(BuiltinProc),
    /// `ExitCode` (`docs/spec/semantics.md` 5.3): the `System` global variable.
    ExitCode,
    /// `MaxInt`: the `Integer` constant 2147483647.
    MaxInt,
    /// `sLineBreak`: the `string` constant `#10` (`docs/spec/semantics.md` 5.3, 6.1).
    LineBreak,
    /// `Input` / `Output` / `ErrOutput` (`docs/spec/semantics.md` 6.1).
    StdFile(StdFile),
    /// Reserved for M2 or later (the planned milestone).
    Reserved(&'static str),
    /// A removed name (with its code).
    Deleted(Code),
}

pub fn lookup_system(lower: &str) -> Option<SystemName> {
    if let Some(t) = Ty::from_name(lower).or_else(|| builtin_exception(lower)) {
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
        "low" => SystemName::Func(Builtin::Low),
        "high" => SystemName::Func(Builtin::High),
        "default" => SystemName::Func(Builtin::Default),
        "write" => SystemName::Proc(BuiltinProc::Write),
        "writeln" => SystemName::Proc(BuiltinProc::WriteLn),
        "readln" => SystemName::Proc(BuiltinProc::ReadLn),
        "read" => SystemName::Proc(BuiltinProc::Read),
        "flush" => SystemName::Proc(BuiltinProc::Flush),
        "eof" => SystemName::Func(Builtin::Eof),
        "eoln" => SystemName::Func(Builtin::Eoln),
        "input" => SystemName::StdFile(StdFile::Input),
        "output" => SystemName::StdFile(StdFile::Output),
        "erroutput" => SystemName::StdFile(StdFile::ErrOutput),
        "slinebreak" => SystemName::LineBreak,
        "inc" => SystemName::Proc(BuiltinProc::Inc),
        "dec" => SystemName::Proc(BuiltinProc::Dec),
        "assert" => SystemName::Proc(BuiltinProc::Assert),
        "halt" => SystemName::Proc(BuiltinProc::Halt),
        "exit" => SystemName::Proc(BuiltinProc::Exit),
        "break" => SystemName::Proc(BuiltinProc::Break),
        "continue" => SystemName::Proc(BuiltinProc::Continue),
        "setlength" => SystemName::Proc(BuiltinProc::SetLength),
        "include" => SystemName::Proc(BuiltinProc::Include),
        "exclude" => SystemName::Proc(BuiltinProc::Exclude),
        "assigned" => SystemName::Func(Builtin::Assigned),
        "succ" => SystemName::Func(Builtin::Succ),
        "pred" => SystemName::Func(Builtin::Pred),
        "sqrt" => SystemName::Func(Builtin::Sqrt),
        "sqr" => SystemName::Func(Builtin::Sqr),
        "pi" => SystemName::Func(Builtin::Pi),
        "trunc" => SystemName::Func(Builtin::Trunc),
        "round" => SystemName::Func(Builtin::Round),
        "odd" => SystemName::Func(Builtin::Odd),
        "upcase" => SystemName::Func(Builtin::UpCase),
        "sizeof" => SystemName::Func(Builtin::SizeOf),
        "concat" => SystemName::Func(Builtin::Concat),
        "insert" => SystemName::Proc(BuiltinProc::Insert),
        "delete" => SystemName::Proc(BuiltinProc::Delete),
        "exitcode" => SystemName::ExitCode,
        "maxint" => SystemName::MaxInt,
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

/// A built-in exception class (`docs/spec/exceptions.md` chapter 2; in `System`, and also named
/// through `System.SysUtils`).
pub fn builtin_exception(lower: &str) -> Option<Ty> {
    crate::bytecode::BUILTIN_EXCEPTIONS
        .iter()
        .find(|(_, name, _, _)| name.eq_ignore_ascii_case(lower))
        .map(|(i, ..)| Ty::Exception(TypeId(*i)))
}

/// A name in `System.SysUtils`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysUtilsName {
    Func(Builtin),
    /// `TReplaceFlag` / `TReplaceFlags` of `StringReplace` (`docs/spec/semantics.md` 5.3).
    ReplaceFlag,
    ReplaceFlags,
    /// `TCaseMapping` / `TCaseFold` of the `Ansi*` case routines (`docs/spec/semantics.md` 5.3).
    CaseMapping,
    CaseFold,
}

/// Names in `System.SysUtils`.
pub fn lookup_sysutils(lower: &str) -> Option<SysUtilsName> {
    match lower {
        "treplaceflag" => return Some(SysUtilsName::ReplaceFlag),
        "treplaceflags" => return Some(SysUtilsName::ReplaceFlags),
        "tcasemapping" => return Some(SysUtilsName::CaseMapping),
        "tcasefold" => return Some(SysUtilsName::CaseFold),
        _ => {}
    }
    lookup_sysutils_func(lower).map(SysUtilsName::Func)
}

/// Routines in `System.SysUtils`.
fn lookup_sysutils_func(lower: &str) -> Option<Builtin> {
    Some(match lower {
        "inttostr" => Builtin::IntToStr,
        "strtoint" => Builtin::StrToInt,
        "strtoint64" => Builtin::StrToInt64,
        "strtointdef" => Builtin::StrToIntDef,
        "uppercase" => Builtin::UpperCase,
        "lowercase" => Builtin::LowerCase,
        "trim" => Builtin::Trim,
        "getenvironmentvariable" => Builtin::GetEnvironmentVariable,
        "floattostr" => Builtin::FloatToStr,
        "strtofloat" => Builtin::StrToFloat,
        "format" => Builtin::Format,
        "inttohex" => Builtin::IntToHex,
        "strtobool" => Builtin::StrToBool,
        "booltostr" => Builtin::BoolToStr,
        "trimleft" => Builtin::TrimLeft,
        "trimright" => Builtin::TrimRight,
        "sametext" => Builtin::SameText,
        "comparetext" => Builtin::CompareText,
        "comparestr" => Builtin::CompareStr,
        "stringreplace" => Builtin::StringReplace,
        "stringofchar" => Builtin::StringOfChar,
        "quotedstr" => Builtin::QuotedStr,
        "trystrtoint" => Builtin::TryStrToInt,
        "ansiuppercase" => Builtin::AnsiUpperCase,
        "ansilowercase" => Builtin::AnsiLowerCase,
        "ansisametext" => Builtin::AnsiSameText,
        "ansicomparetext" => Builtin::AnsiCompareText,
        _ => return None,
    })
}

/// Names reserved for M2 or later (the table in chapter 5).
fn reserved_milestone(lower: &str) -> Option<&'static str> {
    const M2: &[&str] = &["exceptobject"];
    const M3: &[&str] = &["tobject", "trefobject"];
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
        Code::E(3019) => "assign `nil` instead; reference counted objects are freed automatically",
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
        Builtin::Default => (&[("T", Ty::Error)], Ty::Error),
        Builtin::Low | Builtin::High => (&[("X", Ty::Error)], Ty::Error),
        Builtin::Assigned => (&[("X", Ty::Error)], Ty::Boolean),
        Builtin::Succ | Builtin::Pred => (&[("X", Ty::Error)], Ty::Error),
        Builtin::Sqrt => (&[("X", Ty::Double)], Ty::Double),
        Builtin::Sqr => (&[("X", Ty::Error)], Ty::Error),
        Builtin::Pi => (&[], Ty::Double),
        Builtin::Trunc | Builtin::Round => (&[("X", Ty::Error)], Ty::Int64),
        Builtin::FloatToStr => (&[("Value", Ty::Error)], Ty::String),
        Builtin::StrToFloat => (&[("S", Ty::String)], Ty::Double),
        Builtin::Odd => (&[("X", Ty::Error)], Ty::Boolean),
        Builtin::UpCase => (&[("Ch", Ty::Char)], Ty::Char),
        Builtin::SizeOf => (&[("X", Ty::Error)], Ty::Integer),
        Builtin::Concat => (&[("S", Ty::Error)], Ty::Error),
        Builtin::Format => (&[("Fmt", Ty::String), ("Args", Ty::Error)], Ty::String),
        Builtin::IntToHex => (&[("Value", Ty::Error), ("Digits", Ty::Integer)], Ty::String),
        Builtin::StrToBool => (&[("S", Ty::String)], Ty::Boolean),
        Builtin::BoolToStr => (
            &[("B", Ty::Boolean), ("UseBoolStrs", Ty::Boolean)],
            Ty::String,
        ),
        Builtin::TrimLeft | Builtin::TrimRight | Builtin::QuotedStr => {
            (&[("S", Ty::String)], Ty::String)
        }
        Builtin::SameText => (&[("S1", Ty::String), ("S2", Ty::String)], Ty::Boolean),
        Builtin::CompareText | Builtin::CompareStr => {
            (&[("S1", Ty::String), ("S2", Ty::String)], Ty::Integer)
        }
        Builtin::StringReplace => (
            &[
                ("S", Ty::String),
                ("OldPattern", Ty::String),
                ("NewPattern", Ty::String),
                ("Flags", Ty::Error),
            ],
            Ty::String,
        ),
        Builtin::StringOfChar => (&[("Ch", Ty::Char), ("Count", Ty::Integer)], Ty::String),
        Builtin::TryStrToInt => (&[("S", Ty::String), ("Value", Ty::Integer)], Ty::Boolean),
        // The last parameter is `TCaseMapping` / `TCaseFold` (in the type table only once used)
        Builtin::AnsiUpperCase | Builtin::AnsiLowerCase => {
            (&[("S", Ty::String), ("Mapping", Ty::Error)], Ty::String)
        }
        Builtin::AnsiSameText => (
            &[("S1", Ty::String), ("S2", Ty::String), ("Fold", Ty::Error)],
            Ty::Boolean,
        ),
        Builtin::AnsiCompareText => (
            &[("S1", Ty::String), ("S2", Ty::String), ("Fold", Ty::Error)],
            Ty::Integer,
        ),
        // `Eof(Input)` / `Eoln(Input)`: the optional `Input` is removed before the call is lowered
        Builtin::Eof | Builtin::Eoln => (&[], Ty::Boolean),
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
        Builtin::Default => "Default",
        Builtin::Low => "Low",
        Builtin::High => "High",
        Builtin::Assigned => "Assigned",
        Builtin::Succ => "Succ",
        Builtin::Pred => "Pred",
        Builtin::Sqrt => "Sqrt",
        Builtin::Sqr => "Sqr",
        Builtin::Pi => "Pi",
        Builtin::Trunc => "Trunc",
        Builtin::Round => "Round",
        Builtin::FloatToStr => "FloatToStr",
        Builtin::StrToFloat => "StrToFloat",
        Builtin::Odd => "Odd",
        Builtin::UpCase => "UpCase",
        Builtin::SizeOf => "SizeOf",
        Builtin::Concat => "Concat",
        Builtin::Format => "Format",
        Builtin::IntToHex => "IntToHex",
        Builtin::StrToBool => "StrToBool",
        Builtin::BoolToStr => "BoolToStr",
        Builtin::TrimLeft => "TrimLeft",
        Builtin::TrimRight => "TrimRight",
        Builtin::SameText => "SameText",
        Builtin::CompareText => "CompareText",
        Builtin::CompareStr => "CompareStr",
        Builtin::StringReplace => "StringReplace",
        Builtin::StringOfChar => "StringOfChar",
        Builtin::QuotedStr => "QuotedStr",
        Builtin::TryStrToInt => "TryStrToInt",
        Builtin::AnsiUpperCase => "AnsiUpperCase",
        Builtin::AnsiLowerCase => "AnsiLowerCase",
        Builtin::AnsiSameText => "AnsiSameText",
        Builtin::AnsiCompareText => "AnsiCompareText",
        Builtin::Eof => "Eof",
        Builtin::Eoln => "Eoln",
    }
}

pub fn proc_name(p: BuiltinProc) -> &'static str {
    match p {
        BuiltinProc::Write => "Write",
        BuiltinProc::WriteLn => "WriteLn",
        BuiltinProc::ReadLn => "ReadLn",
        BuiltinProc::Read => "Read",
        BuiltinProc::Flush => "Flush",
        BuiltinProc::Inc => "Inc",
        BuiltinProc::Dec => "Dec",
        BuiltinProc::Assert => "Assert",
        BuiltinProc::Halt => "Halt",
        BuiltinProc::Exit => "Exit",
        BuiltinProc::Break => "Break",
        BuiltinProc::Continue => "Continue",
        BuiltinProc::SetLength => "SetLength",
        BuiltinProc::Include => "Include",
        BuiltinProc::Exclude => "Exclude",
        BuiltinProc::Insert => "Insert",
        BuiltinProc::Delete => "Delete",
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
