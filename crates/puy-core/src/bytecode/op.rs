//! The M1 instruction set (`docs/spec/bytecode.md` chapter 2).

/// Operand shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// No operands
    None,
    /// `A` (register)
    A,
    /// `A B` (two registers)
    AB,
    /// `A B C` (three registers)
    ABC,
    /// `A B C` (`C` is an immediate: the width)
    ABCImm,
    /// `A B` (`B` is an immediate: the `RaiseError` kind)
    ABImm,
    /// `A B` (`B`, `B+1` are consecutive registers)
    ABRange2,
    /// `A B` (`B` to `B+2`)
    ABRange3,
    /// `A B sC` (compare-and-branch)
    ABsC,
    /// `A B sC` (`sC` is an immediate: `AddIntI`)
    ABsCImm,
    /// `A sBx` (immediate: `LoadInt`)
    ASBxImm,
    /// `A Bx` (table index)
    ABx,
    /// `A sBx` (jump)
    ASBx,
    /// `sBx` (jump)
    SBx,
}

#[derive(Debug, Clone, Copy)]
pub struct OpInfo {
    pub code: u8,
    pub name: &'static str,
    pub shape: Shape,
}

macro_rules! ops {
    ($($konst:ident = $code:expr, $name:literal, $shape:ident;)*) => {
        $(pub const $konst: u8 = $code;)*
        pub const TABLE: &[OpInfo] = &[$(OpInfo { code: $code, name: $name, shape: Shape::$shape }),*];
    };
}

ops! {
    NOP = 0, "Nop", None;
    WIDE = 1, "Wide", None;
    MOVE = 2, "Move", AB;
    LOAD_ZERO = 3, "LoadZero", A;
    LOAD_INT = 4, "LoadInt", ASBxImm;
    LOAD_CONST = 5, "LoadConst", ABx;
    LOAD_GLOBAL = 6, "LoadGlobal", ABx;
    STORE_GLOBAL = 7, "StoreGlobal", ABx;
    MAKE_REF = 8, "MakeRef", AB;
    MAKE_GLOBAL_REF = 9, "MakeGlobalRef", ABx;
    LOAD_REF = 10, "LoadRef", AB;
    STORE_REF = 11, "StoreRef", AB;
    ADD_INT = 16, "AddInt", ABC;
    SUB_INT = 17, "SubInt", ABC;
    MUL_INT = 18, "MulInt", ABC;
    DIV_INT = 19, "DivInt", ABC;
    MOD_INT = 20, "ModInt", ABC;
    DIV_UINT = 21, "DivUInt", ABC;
    MOD_UINT = 22, "ModUInt", ABC;
    NEG_INT = 23, "NegInt", AB;
    AND_INT = 24, "AndInt", ABC;
    OR_INT = 25, "OrInt", ABC;
    XOR_INT = 26, "XorInt", ABC;
    NOT_INT = 27, "NotInt", AB;
    SHL_INT = 28, "ShlInt", ABC;
    SHR_INT = 29, "ShrInt", ABC;
    SHL_INT32 = 30, "ShlInt32", ABC;
    SHR_INT32 = 31, "ShrInt32", ABC;
    ADD_INT_I = 32, "AddIntI", ABsCImm;
    ADD_INT_CHK = 33, "AddIntChk", ABC;
    SUB_INT_CHK = 34, "SubIntChk", ABC;
    MUL_INT_CHK = 35, "MulIntChk", ABC;
    NEG_INT_CHK = 36, "NegIntChk", AB;
    DIV_INT_CHK = 37, "DivIntChk", ABC;
    ADD_UINT_CHK = 38, "AddUIntChk", ABC;
    SUB_UINT_CHK = 39, "SubUIntChk", ABC;
    MUL_UINT_CHK = 40, "MulUIntChk", ABC;
    NARROW = 41, "Narrow", ABCImm;
    NARROW_OVF = 42, "NarrowOvf", ABCImm;
    NARROW_RNG = 43, "NarrowRng", ABCImm;
    EQ_INT = 48, "EqInt", ABC;
    LT_INT = 49, "LtInt", ABC;
    LE_INT = 50, "LeInt", ABC;
    LT_UINT = 51, "LtUInt", ABC;
    LE_UINT = 52, "LeUInt", ABC;
    NOT_BOOL = 53, "NotBool", AB;
    JMP = 64, "Jmp", SBx;
    JMP_IF_TRUE = 65, "JmpIfTrue", ASBx;
    JMP_IF_FALSE = 66, "JmpIfFalse", ASBx;
    JMP_IF_EQ_INT = 67, "JmpIfEqInt", ABsC;
    JMP_IF_NE_INT = 68, "JmpIfNeInt", ABsC;
    JMP_IF_LT_INT = 69, "JmpIfLtInt", ABsC;
    JMP_IF_LE_INT = 70, "JmpIfLeInt", ABsC;
    JMP_IF_LT_UINT = 71, "JmpIfLtUInt", ABsC;
    JMP_IF_LE_UINT = 72, "JmpIfLeUInt", ABsC;
    JMP_IF_EQ_STR = 73, "JmpIfEqStr", ABsC;
    JMP_IF_NE_STR = 74, "JmpIfNeStr", ABsC;
    CONCAT_STR = 80, "ConcatStr", ABC;
    STR_LEN = 81, "StrLen", AB;
    STR_INDEX = 82, "StrIndex", ABC;
    STR_SET_INDEX = 83, "StrSetIndex", ABC;
    CMP_STR = 84, "CmpStr", ABC;
    EQ_STR = 85, "EqStr", ABC;
    STR_FROM_CHAR = 86, "StrFromChar", AB;
    INT_TO_STR = 87, "IntToStr", AB;
    UINT_TO_STR = 88, "UIntToStr", AB;
    BOOL_TO_STR = 89, "BoolToStr", AB;
    STR_TO_INT = 90, "StrToInt", AB;
    STR_TO_INT64 = 91, "StrToInt64", AB;
    STR_COPY = 92, "StrCopy", ABRange3;
    STR_POS = 93, "StrPos", ABRange2;
    STR_UPPER_ASCII = 94, "StrUpperAscii", AB;
    STR_LOWER_ASCII = 95, "StrLowerAscii", AB;
    STR_TRIM = 96, "StrTrim", AB;
    STR_PAD_LEFT = 97, "StrPadLeft", ABC;
    STR_TO_INT_DEF = 98, "StrToIntDef", ABRange2;
    STR_TO_INT_IO = 99, "StrToIntIO", ABCImm;
    CALL = 112, "Call", ABx;
    CALL_HOST = 113, "CallHost", ABx;
    RET = 114, "Ret", A;
    RET_VOID = 115, "RetVoid", None;
    HALT = 116, "Halt", A;
    RAISE_ERROR = 117, "RaiseError", ABImm;
    ARG_COUNT = 118, "ArgCount", A;
    ARG_GET = 119, "ArgGet", AB;
}

/// Looks up the information of an opcode.
pub fn info(code: u8) -> Option<&'static OpInfo> {
    LOOKUP[code as usize].map(|i| &TABLE[i])
}

static LOOKUP: [Option<usize>; 256] = {
    let mut t = [None; 256];
    let mut i = 0;
    while i < TABLE.len() {
        t[TABLE[i].code as usize] = Some(i);
        i += 1;
    }
    t
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_works() {
        assert_eq!(info(CALL).unwrap().name, "Call");
        assert!(info(200).is_none());
        for (i, a) in TABLE.iter().enumerate() {
            for b in &TABLE[i + 1..] {
                assert_ne!(a.code, b.code);
            }
        }
    }
}
