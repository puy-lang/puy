//! The instruction set (`docs/spec/bytecode.md` chapter 2).

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
    /// `A B` (`B`, `B+1` are consecutive registers)
    ABRange2,
    /// `A B` (`B` to `B+2`)
    ABRange3,
    /// `A B` (`B` to `B+3`; `StrReplace`)
    ABRange4,
    /// `A B C` (`B` is the format, `B+1 ..= B+2C` the `C` pairs of kind and value; `Format`)
    ABPairs,
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
    /// `A B C` (`C` is an immediate: a field number; `FieldGet`, `ExcGet`, `ExcInfo`)
    ABSlot,
    /// `A B C` (`B` is an immediate: a field number; `FieldSet`, `ExcSet`)
    ASlotC,
    /// `A B C` (`C` is a constant pool index; `ChkIndex`, `ChkRange`)
    ABk,
    /// `A B C` (`C` is a type index; `EnumToStr`)
    ABType,
    /// `A B C` (`C` is the length of the path in `A+1 ..= A+C`; `MakeCell`)
    ABPath,
    /// `A B` (`A`, `A+1` are registers; `B` is the number of `Jmp`s that follow; `EndFinally`)
    AJumps,
    /// `A B C` (`B` is a register of the frame `C` levels out, `C` an immediate; `OuterRef`)
    AOuter,
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
    OUTER_REF = 12, "OuterRef", AOuter;
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
    UP_CASE_CHAR = 100, "UpCaseChar", AB;
    INT_TO_HEX = 101, "IntToHex", ABC;
    STR_TO_BOOL = 102, "StrToBool", AB;
    BOOL_STR = 103, "BoolStr", ABC;
    STR_TRIM_LEFT = 104, "StrTrimLeft", AB;
    STR_TRIM_RIGHT = 105, "StrTrimRight", AB;
    SAME_TEXT = 106, "SameText", ABC;
    COMPARE_TEXT = 107, "CompareText", ABC;
    STR_REPLACE = 108, "StrReplace", ABRange4;
    STR_OF_CHAR = 109, "StrOfChar", ABC;
    QUOTED_STR = 110, "QuotedStr", AB;
    TRY_STR_TO_INT = 111, "TryStrToInt", ABC;
    CALL = 112, "Call", ABx;
    CALL_HOST = 113, "CallHost", ABx;
    RET = 114, "Ret", A;
    RET_VOID = 115, "RetVoid", None;
    HALT = 116, "Halt", A;
    ARG_COUNT = 118, "ArgCount", A;
    ARG_GET = 119, "ArgGet", AB;
    READ_WORD = 120, "ReadWord", A;
    READ_REST = 121, "ReadRest", A;
    READ_CHAR = 122, "ReadChar", A;
    READ_LN_BEGIN = 123, "ReadLnBegin", None;
    READ_LN_END = 124, "ReadLnEnd", None;
    IN_EOF = 125, "InEof", A;
    IN_EOLN = 126, "InEoln", A;
    FLUSH_OUT = 127, "FlushOut", None;
    ASSIGN_REF = 128, "AssignRef", AB;
    MOVE_REF = 129, "MoveRef", AB;
    ADD_REF = 130, "AddRef", A;
    RELEASE = 131, "Release", A;
    STORE_GLOBAL_REF = 132, "StoreGlobalRef", ABx;
    STORE_REF_REF = 133, "StoreRefRef", AB;
    CLEAR_REF = 134, "ClearRef", A;
    UNIQUE = 136, "Unique", ABx;
    VAL_GET = 137, "ValGet", ABC;
    VAL_GET_REF = 138, "ValGetRef", ABC;
    VAL_SET = 139, "ValSet", ABC;
    VAL_SET_REF = 140, "ValSetRef", ABC;
    FIELD_GET = 141, "FieldGet", ABSlot;
    FIELD_GET_REF = 142, "FieldGetRef", ABSlot;
    FIELD_SET = 143, "FieldSet", ASlotC;
    FIELD_SET_REF = 144, "FieldSetRef", ASlotC;
    CHK_INDEX = 145, "ChkIndex", ABk;
    DYN_GET = 146, "DynGet", ABC;
    DYN_GET_REF = 147, "DynGetRef", ABC;
    DYN_SET = 148, "DynSet", ABC;
    DYN_SET_REF = 149, "DynSetRef", ABC;
    ARR_LEN = 150, "ArrLen", AB;
    SET_LEN = 151, "SetLen", ABx;
    NEW_ARR = 152, "NewArr", ABx;
    ARR_COPY = 153, "ArrCopy", ABx;
    MAKE_CELL = 154, "MakeCell", ABPath;
    LOAD_GLOBAL_REF = 155, "LoadGlobalRef", ABx;
    LOAD_REF_REF = 156, "LoadRefRef", AB;
    STR_SET_LEN = 157, "StrSetLen", AB;
    UNIQUE_VAL = 158, "UniqueVal", A;
    MATERIALIZE = 159, "Materialize", ABx;
    NEW_EXC = 160, "NewExc", ABx;
    RAISE = 161, "Raise", A;
    RAISE_OUTER = 162, "RaiseOuter", AB;
    END_FINALLY = 163, "EndFinally", AJumps;
    EXC_CLASS = 164, "ExcClass", AB;
    CLASS_IS = 165, "ClassIs", ABx;
    EXC_INFO = 166, "ExcInfo", ABSlot;
    EXC_GET = 167, "ExcGet", ABSlot;
    EXC_SET = 168, "ExcSet", ASlotC;
    CHK_RANGE = 169, "ChkRange", ABk;
    ENUM_TO_STR = 170, "EnumToStr", ABType;
    SET_IN = 171, "SetIn", ABC;
    SET_RANGE = 172, "SetRange", ABC;
    SET_NEXT = 173, "SetNext", ABC;
    CHK_SET = 174, "ChkSet", ABk;
    SET_IN_BIG = 175, "SetInBig", ABC;
    SET_INCL = 176, "SetIncl", ABC;
    SET_EXCL = 177, "SetExcl", ABC;
    SET_UNION = 178, "SetUnion", AB;
    SET_DIFF = 179, "SetDiff", AB;
    SET_INTER = 180, "SetInter", AB;
    SET_EQ = 181, "SetEq", ABC;
    SET_SUB = 182, "SetSub", ABC;
    SET_NEXT_BIG = 183, "SetNextBig", ABC;
    CHK_SET_BIG = 184, "ChkSetBig", ABk;
    SET_UNION_BITS = 185, "SetUnionBits", AB;
    SET_BITS = 186, "SetBits", AB;
    ADD_F64 = 187, "AddF64", ABC;
    SUB_F64 = 188, "SubF64", ABC;
    MUL_F64 = 189, "MulF64", ABC;
    DIV_F64 = 190, "DivF64", ABC;
    NEG_F64 = 191, "NegF64", AB;
    EQ_F64 = 192, "EqF64", ABC;
    LT_F64 = 193, "LtF64", ABC;
    LE_F64 = 194, "LeF64", ABC;
    ADD_F32 = 195, "AddF32", ABC;
    SUB_F32 = 196, "SubF32", ABC;
    MUL_F32 = 197, "MulF32", ABC;
    DIV_F32 = 198, "DivF32", ABC;
    NEG_F32 = 199, "NegF32", AB;
    EQ_F32 = 200, "EqF32", ABC;
    LT_F32 = 201, "LtF32", ABC;
    LE_F32 = 202, "LeF32", ABC;
    INT_TO_F64 = 203, "IntToF64", AB;
    UINT_TO_F64 = 204, "UIntToF64", AB;
    INT_TO_F32 = 205, "IntToF32", AB;
    UINT_TO_F32 = 206, "UIntToF32", AB;
    F64_TO_F32 = 207, "F64ToF32", AB;
    F32_TO_F64 = 208, "F32ToF64", AB;
    TRUNC_F64 = 209, "TruncF64", AB;
    ROUND_F64 = 210, "RoundF64", AB;
    SQRT_F64 = 211, "SqrtF64", AB;
    ADD_CURR = 212, "AddCurr", ABC;
    SUB_CURR = 213, "SubCurr", ABC;
    MUL_CURR = 214, "MulCurr", ABC;
    DIV_CURR = 215, "DivCurr", ABC;
    NEG_CURR = 216, "NegCurr", AB;
    INT_TO_CURR = 217, "IntToCurr", AB;
    UINT_TO_CURR = 218, "UIntToCurr", AB;
    F64_TO_CURR = 219, "F64ToCurr", AB;
    CURR_TO_F64 = 220, "CurrToF64", AB;
    CURR_TO_F32 = 221, "CurrToF32", AB;
    TRUNC_CURR = 222, "TruncCurr", AB;
    ROUND_CURR = 223, "RoundCurr", AB;
    F64_TO_STR = 224, "F64ToStr", AB;
    F32_TO_STR = 225, "F32ToStr", AB;
    CURR_TO_STR = 226, "CurrToStr", AB;
    F64_TO_FIXED = 227, "F64ToFixed", ABC;
    CURR_TO_FIXED = 228, "CurrToFixed", ABC;
    STR_TO_F64 = 229, "StrToF64", AB;
    STR_TO_U8 = 230, "StrToU8", AB;
    U8_TO_STR = 231, "U8ToStr", AB;
    U8_LEN = 232, "U8Len", AB;
    U8_INDEX = 233, "U8Index", ABC;
    CONCAT_U8 = 234, "ConcatU8", ABC;
    CMP_U8 = 235, "CmpU8", ABC;
    EQ_U8 = 236, "EqU8", ABC;
    U8_COPY = 237, "U8Copy", ABRange3;
    U8_POS = 238, "U8Pos", ABRange2;
    U8_SET_LEN = 239, "U8SetLen", AB;
    STR_INSERT = 240, "StrInsert", ABRange2;
    STR_DELETE = 241, "StrDelete", ABRange2;
    U8_INSERT = 242, "U8Insert", ABRange2;
    U8_DELETE = 243, "U8Delete", ABRange2;
    ARR_CONCAT = 244, "ArrConcat", ABx;
    ARR_INSERT = 245, "ArrInsert", ABx;
    ARR_INSERT_ARR = 246, "ArrInsertArr", ABx;
    ARR_DELETE = 247, "ArrDelete", ABx;
    FORMAT = 248, "Format", ABPairs;
    STR_UPPER = 249, "StrUpper", ABC;
    STR_LOWER = 250, "StrLower", ABC;
    ANSI_SAME_TEXT = 251, "AnsiSameText", ABRange3;
    ANSI_COMPARE_TEXT = 252, "AnsiCompareText", ABRange3;
    STR_TO_FLOAT_IO = 253, "StrToFloatIO", ABCImm;
    SLICE_RANGE = 254, "SliceRange", ABRange3;
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
        assert!(info(255).is_none());
        assert!(info(117).is_none());
        for (i, a) in TABLE.iter().enumerate() {
            for b in &TABLE[i + 1..] {
                assert_ne!(a.code, b.code);
            }
        }
    }
}
