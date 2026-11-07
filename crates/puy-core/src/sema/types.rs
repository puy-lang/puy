//! M1 types and constant values (`docs/spec/semantics.md` chapter 2).

use std::fmt;

/// M1 types. Aliases (`LongInt`, `NativeInt`, etc.) resolve to the same type.
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

    /// Ordinal types (M1: integers, `Boolean`, `Char`).
    pub fn is_ordinal(self) -> bool {
        self.is_integer() || matches!(self, Ty::Boolean | Ty::Char)
    }

    pub fn is_error(self) -> bool {
        self == Ty::Error
    }

    /// Type name (for diagnostics; `string` is lowercase).
    pub fn name(self) -> &'static str {
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
            "cardinal" | "longword" => Ty::Cardinal,
            "int64" | "nativeint" => Ty::Int64,
            "uint64" | "nativeuint" => Ty::UInt64,
            "boolean" => Ty::Boolean,
            "char" => Ty::Char,
            "string" => Ty::String,
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

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A constant value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Const {
    /// Integer (the type is carried by the expression).
    Int(i128),
    Bool(bool),
    /// A single UTF-16 code unit.
    Char(u16),
    /// A sequence of UTF-16 code units.
    Str(Vec<u16>),
}

impl Const {
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
            Const::Char(c) => Some(vec![*c]),
            _ => None,
        }
    }

    /// Human-readable form (for the E0714 message).
    pub fn display(&self) -> String {
        match self {
            Const::Int(v) => v.to_string(),
            Const::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            Const::Char(c) => format!("#{c}"),
            Const::Str(s) => format!("'{}'", String::from_utf16_lossy(s)),
        }
    }
}

/// Maximum string length (in code units; 3.5).
pub const MAX_STRING_LEN: usize = i32::MAX as usize;
