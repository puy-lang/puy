//! The list of directives and how each is handled (`docs/spec/directives.md` chapter 2).

/// Families of directives that are warned about and ignored (determines the `notes` of W0004; `docs/spec/diagnostics.md` 4.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreCategory {
    /// Resources and executables
    Resource,
    /// `{$TEXTBLOCK}`
    TextBlock,
    /// `{$B}` / `{$BOOLEVAL}`
    BoolEval,
    /// `{$J}` / `{$WRITEABLECONST}`
    WriteableConst,
    /// `{$RTTI}`, `{$M+}` / `{$TYPEINFO}`, `{$METHODINFO}`
    Rtti,
    /// `{$SCOPEDENUMS}`
    ScopedEnums,
    /// `{$HINTS}`, `{$WARNINGS}`
    HintsWarnings,
    /// Code generation and linking
    Codegen,
    /// For C++Builder
    CppBuilder,
    /// FPC-specific
    Fpc,
    /// `{$PUSH}` / `{$POP}`
    PushPop,
}

impl IgnoreCategory {
    pub fn notes(self) -> &'static [&'static str] {
        match self {
            IgnoreCategory::Resource => &[
                "Puy does not build native executables from directives; executable settings belong in `puy.toml`",
            ],
            IgnoreCategory::TextBlock => {
                &["multi-line strings in Puy always use LF; write `#13#10` explicitly for CR LF"]
            }
            IgnoreCategory::BoolEval => &["boolean operators in Puy always short-circuit"],
            IgnoreCategory::WriteableConst => &["typed constants in Puy are always read-only"],
            IgnoreCategory::Rtti => &["Puy keeps run-time type information for every type"],
            IgnoreCategory::ScopedEnums => {
                &["enumeration values in Puy are always qualified with the type name"]
            }
            IgnoreCategory::HintsWarnings => {
                &["disable individual warnings with `{$WARN <name> OFF}`"]
            }
            IgnoreCategory::Codegen => {
                &["Puy compiles to bytecode for a VM, where this directive has no meaning"]
            }
            IgnoreCategory::CppBuilder => &["Puy does not generate C++ headers"],
            IgnoreCategory::Fpc => &["Puy always uses the Delphi dialect"],
            IgnoreCategory::PushPop => &[
                "Puy always uses the Delphi dialect",
                "to restore a switch later, record it with `{$IFOPT R+}{$DEFINE R_ON}{$ENDIF}` and test `R_ON` afterwards",
            ],
        }
    }

    pub fn is_fpc(self) -> bool {
        matches!(self, IgnoreCategory::Fpc | IgnoreCategory::PushPop)
    }
}

/// How a directive is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handling {
    /// Implemented (per-name handling is in `mod.rs`).
    Implemented,
    /// Silently accepted.
    Silent,
    /// Ignored with warning W0004.
    Ignored(IgnoreCategory),
    /// Handling depends on the argument (decided in `mod.rs`).
    ByArgument,
}

use Handling::*;
use IgnoreCategory::*;

/// Directive names (uppercase) and their handling. Similar-name suggestions use all names in this table.
pub const DIRECTIVES: &[(&str, Handling)] = &[
    // Implemented
    ("DEFINE", Implemented),
    ("UNDEF", Implemented),
    ("IFDEF", Implemented),
    ("IFNDEF", Implemented),
    ("IF", Implemented),
    ("ELSEIF", Implemented),
    ("ELSE", Implemented),
    ("ENDIF", Implemented),
    ("IFEND", Implemented),
    ("I", ByArgument),
    ("INCLUDE", Implemented),
    ("Q", Implemented),
    ("OVERFLOWCHECKS", Implemented),
    ("R", ByArgument),
    ("RANGECHECKS", Implemented),
    ("RESOURCE", Ignored(Resource)),
    ("C", Implemented),
    ("ASSERTIONS", Implemented),
    ("WARN", Implemented),
    ("MESSAGE", Implemented),
    ("IFOPT", Implemented),
    ("POINTERS", ByArgument),
    ("POINTERMATH", ByArgument),
    // Silently accepted (those that depend on the argument are ByArgument)
    ("REGION", Silent),
    ("ENDREGION", Silent),
    ("H", ByArgument),
    ("LONGSTRINGS", ByArgument),
    ("ZEROBASEDSTRINGS", ByArgument),
    ("IOCHECKS", ByArgument),
    ("LEGACYIFEND", Silent),
    ("INLINE", Silent),
    ("MODE", ByArgument),
    // Warned about and ignored: resources and executables
    ("RESOURCERESERVE", Ignored(Resource)),
    ("APPTYPE", Ignored(Resource)),
    ("SETPEFLAGS", Ignored(Resource)),
    ("SETPEOPTFLAGS", Ignored(Resource)),
    ("SETPEOSVERSION", Ignored(Resource)),
    ("SETPESUBSYSVERSION", Ignored(Resource)),
    ("SETPEUSERVERSION", Ignored(Resource)),
    ("DYNAMICBASE", Ignored(Resource)),
    ("HIGHENTROPYVA", Ignored(Resource)),
    ("LARGEADDRESSAWARE", Ignored(Resource)),
    ("NXCOMPAT", Ignored(Resource)),
    ("TSAWARE", Ignored(Resource)),
    ("IMAGEBASE", Ignored(Resource)),
    ("DESCRIPTION", Ignored(Resource)),
    ("LIBPREFIX", Ignored(Resource)),
    ("LIBSUFFIX", Ignored(Resource)),
    ("LIBVERSION", Ignored(Resource)),
    ("E", Ignored(Resource)),
    ("EXTENSION", Ignored(Resource)),
    // Directives that change meaning but are ignored
    ("TEXTBLOCK", Ignored(TextBlock)),
    ("B", Ignored(BoolEval)),
    ("BOOLEVAL", Ignored(BoolEval)),
    ("J", Ignored(WriteableConst)),
    ("WRITEABLECONST", Ignored(WriteableConst)),
    ("RTTI", Ignored(Rtti)),
    ("M", ByArgument),
    ("TYPEINFO", Ignored(Rtti)),
    ("METHODINFO", Ignored(Rtti)),
    ("SCOPEDENUMS", Ignored(ScopedEnums)),
    ("HINTS", Ignored(HintsWarnings)),
    ("WARNINGS", Ignored(HintsWarnings)),
    // Code generation and linking
    ("A", Ignored(Codegen)),
    ("ALIGN", Ignored(Codegen)),
    ("CODEALIGN", Ignored(Codegen)),
    ("O", Ignored(Codegen)),
    ("OPTIMIZATION", Ignored(Codegen)),
    ("D", Ignored(Codegen)),
    ("DEBUGINFO", Ignored(Codegen)),
    ("L", Ignored(Codegen)),
    ("LOCALSYMBOLS", Ignored(Codegen)),
    ("LINK", Ignored(Codegen)),
    ("W", Ignored(Codegen)),
    ("STACKFRAMES", Ignored(Codegen)),
    ("Y", Ignored(Codegen)),
    ("YD", Ignored(Codegen)),
    ("REFERENCEINFO", Ignored(Codegen)),
    ("DEFINITIONINFO", Ignored(Codegen)),
    ("Z", Ignored(Codegen)),
    ("MINENUMSIZE", Ignored(Codegen)),
    ("MINSTACKSIZE", Ignored(Codegen)),
    ("MAXSTACKSIZE", Ignored(Codegen)),
    ("G", Ignored(Codegen)),
    ("IMPORTEDDATA", Ignored(Codegen)),
    ("X", Ignored(Codegen)),
    ("EXTENDEDSYNTAX", Ignored(Codegen)),
    ("P", Ignored(Codegen)),
    ("OPENSTRINGS", Ignored(Codegen)),
    ("T", Ignored(Codegen)),
    ("TYPEDADDRESS", Ignored(Codegen)),
    ("U", Ignored(Codegen)),
    ("SAFEDIVIDE", Ignored(Codegen)),
    ("V", Ignored(Codegen)),
    ("VARSTRINGCHECKS", Ignored(Codegen)),
    ("EXCESSPRECISION", Ignored(Codegen)),
    ("FINITEFLOAT", Ignored(Codegen)),
    ("REALCOMPATIBILITY", Ignored(Codegen)),
    ("HIGHCHARUNICODE", Ignored(Codegen)),
    ("EXTENDEDCOMPATIBILITY", Ignored(Codegen)),
    ("STRONGLINKTYPES", Ignored(Codegen)),
    ("WEAKLINKRTTI", Ignored(Codegen)),
    ("IMPLICITBUILD", Ignored(Codegen)),
    ("RUNONLY", Ignored(Codegen)),
    ("DENYPACKAGEUNIT", Ignored(Codegen)),
    ("WEAKPACKAGEUNIT", Ignored(Codegen)),
    ("OBJEXPORTALL", Ignored(Codegen)),
    // For C++Builder
    ("EXTERNALSYM", Ignored(CppBuilder)),
    ("HPPEMIT", Ignored(CppBuilder)),
    ("NODEFINE", Ignored(CppBuilder)),
    ("NOINCLUDE", Ignored(CppBuilder)),
    ("OBJTYPENAME", Ignored(CppBuilder)),
    // FPC-specific (those treated as the corresponding kind of `{$MESSAGE}` are Implemented)
    ("MODESWITCH", Ignored(Fpc)),
    ("PUSH", Ignored(PushPop)),
    ("POP", Ignored(PushPop)),
    ("PACKRECORDS", Ignored(Fpc)),
    ("PACKENUM", Ignored(Fpc)),
    ("PACKSET", Ignored(Fpc)),
    ("BITPACKING", Ignored(Fpc)),
    ("INTERFACES", Ignored(Fpc)),
    ("COPERATORS", Ignored(Fpc)),
    ("ASMMODE", Ignored(Fpc)),
    ("SMARTLINK", Ignored(Fpc)),
    ("FPUTYPE", Ignored(Fpc)),
    ("CODEPAGE", Ignored(Fpc)),
    ("NOTES", Ignored(Fpc)),
    ("MACRO", Ignored(Fpc)),
    ("GOTO", Ignored(Fpc)),
    ("LINKLIB", Ignored(Fpc)),
    ("LINKFRAMEWORK", Ignored(Fpc)),
    ("CHECKPOINTER", Ignored(Fpc)),
    ("OBJECTCHECKS", Ignored(Fpc)),
    ("CHECKLOWADDRLOADS", Ignored(Fpc)),
    ("CHECKCASECOVERAGE", Ignored(Fpc)),
    ("IMPLICITEXCEPTIONS", Ignored(Fpc)),
    ("IEEEERRORS", Ignored(Fpc)),
    ("SAFEFPUEXCEPTIONS", Ignored(Fpc)),
    ("MAXFPUREGISTERS", Ignored(Fpc)),
    ("MINFPCONSTPREC", Ignored(Fpc)),
    ("SATURATION", Ignored(Fpc)),
    ("CALLING", Ignored(Fpc)),
    ("SYSCALL", Ignored(Fpc)),
    ("THREADING", Ignored(Fpc)),
    ("TARGET", Ignored(Fpc)),
    ("VARPROPSETTER", Ignored(Fpc)),
    ("WAIT", Ignored(Fpc)),
    ("STATIC", Ignored(Fpc)),
    ("PIC", Ignored(Fpc)),
    ("PROFILE", Ignored(Fpc)),
    ("MEMORY", Ignored(Fpc)),
    ("SCREENNAME", Ignored(Fpc)),
    ("APPID", Ignored(Fpc)),
    ("APPNAME", Ignored(Fpc)),
    ("VERSION", Ignored(Fpc)),
    ("COPYRIGHT", Ignored(Fpc)),
    ("NAMESPACE", Ignored(Fpc)),
    ("UNITPATH", Ignored(Fpc)),
    ("INCLUDEPATH", Ignored(Fpc)),
    ("LIBRARYPATH", Ignored(Fpc)),
    ("OBJECTPATH", Ignored(Fpc)),
    ("HUGECODE", Ignored(Fpc)),
    ("HUGEPOINTERS", Ignored(Fpc)),
    ("EXTENDEDSYM", Ignored(Fpc)),
    ("STOP", Ignored(Fpc)),
    ("WARNING", Implemented),
    ("ERROR", Implemented),
    ("FATAL", Implemented),
    ("HINT", Implemented),
    ("NOTE", Implemented),
    ("INFO", Implemented),
];

pub fn lookup(upper_name: &str) -> Option<Handling> {
    DIRECTIVES
        .iter()
        .find(|(n, _)| *n == upper_name)
        .map(|(_, h)| *h)
}

pub fn names() -> impl Iterator<Item = &'static str> {
    DIRECTIVES.iter().map(|(n, _)| *n)
}

/// Symbols determined by the implementation (3.3).
pub const BUILTIN_SYMBOLS: &[&str] = &["PUY", "UNICODE", "CONDITIONALEXPRESSIONS", "CPU64BITS"];

/// OS and CPU symbols (always false; W0007 is issued on every use).
pub const PLATFORM_SYMBOLS: &[&str] = &[
    "MSWINDOWS",
    "WIN32",
    "WIN64",
    "POSIX",
    "LINUX",
    "MACOS",
    "DARWIN",
    "ANDROID",
    "IOS",
    "CPUX86",
    "CPUX64",
    "CPUARM",
    "CPUARM64",
    "CPU32BITS",
];

/// Whether this is a compiler-identification symbol (`FPC`, `DCC`, `VER` + digits).
pub fn is_compiler_symbol(upper: &str) -> bool {
    upper == "FPC"
        || upper == "DCC"
        || (upper.len() > 3
            && upper.starts_with("VER")
            && upper[3..].bytes().all(|b| b.is_ascii_digit()))
}

pub fn is_platform_symbol(upper: &str) -> bool {
    PLATFORM_SYMBOLS.contains(&upper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_names() {
        let mut names: Vec<_> = names().collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(n, names.len());
    }

    #[test]
    fn compiler_symbols() {
        assert!(is_compiler_symbol("VER350"));
        assert!(is_compiler_symbol("FPC"));
        assert!(!is_compiler_symbol("VER"));
        assert!(!is_compiler_symbol("VERSION"));
        assert!(is_platform_symbol("LINUX"));
    }
}
