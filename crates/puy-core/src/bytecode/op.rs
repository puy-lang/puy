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
    /// `A B` (`B` to `B+4`; `HashFind`)
    ABRange5,
    /// `A B C` (`B`, `B+1` are consecutive registers, `C` an immediate: a slot number;
    /// `HashLookupInt`, `HashLookupStr`)
    ABRange2Slot,
    /// `A B C` (`B` is the format, `B+1 ..= B+2C` the `C` pairs of kind and value; `Format`)
    ABPairs,
    /// `A B C` (`C` is the number of registers `B ..= B+C-1`, at least 2; `ConcatStrN`)
    ABList,
    /// `A B sC` (compare-and-branch)
    ABsC,
    /// `A B sC` (`sC` is an immediate: `AddIntI` and the other integer operations with an
    /// immediate)
    ABsCImm,
    /// `A sB sC` (`sB` is an immediate, `sC` the offset: compare-and-branch with an immediate)
    AsBsC,
    /// `A sBx` (immediate: `LoadInt`)
    ASBxImm,
    /// `A Bx` (table index)
    ABx,
    /// `A sBx` (jump)
    ASBx,
    /// `sBx` (jump)
    SBx,
    /// `A B C` (`C` is an immediate: a field number; `FieldGet`, `ExcGet`, `ExcInfo`, `ObjGet`)
    ABSlot,
    /// `A B C` (`B` is an immediate: a field number; `FieldSet`, `ExcSet`, `ObjSet`)
    ASlotC,
    /// `A B C` as [`Shape::ASlotC`], followed by a type word holding a type index
    /// (`RefFieldSet`; `docs/spec/bytecode.md` 1.2)
    ASlotCType,
    /// `A B C` (`C` is a constant pool index; `ChkIndex`, `ChkRange`)
    ABk,
    /// `A B C` (`C` is a type index; `EnumToStr`)
    ABType,
    /// `A B C` (`C` is the length of the path in `A+1 ..= A+C`; `Lend`)
    ABPath,
    /// `A B C` as [`Shape::ABPath`], followed by a type word holding the type of the flat place
    /// (`LendPlace`; `docs/spec/bytecode.md` 1.2, 2.8)
    ABPathType,
    /// `A B C` (`A`, `A+1` are the cursor written, `B` and `C` registers; `ElemAt`)
    ACursorBC,
    /// `A B C` (`A` is a register, `B`, `B+1` the cursor, `C` an immediate: a slot number in the
    /// flat place; `SlotGet`)
    ABCursorSlot,
    /// `A B C` (`A`, `A+1` are the cursor, `B` an immediate: a slot number in the flat place, `C`
    /// a register; `SlotSet`)
    ACursorSlotC,
    /// `A B` (`A` is a register, `B`, `B+1` the cursor), followed by a type word (`AggLoad`)
    ABCursorType,
    /// `A B` (`A`, `A+1` are the cursor, `B` a register), followed by a type word (`AggStore`)
    ACursorBType,
    /// `A B` (`A`, `A+1` are registers; `B` is the number of `Jmp`s that follow; `EndFinally`)
    AJumps,
    /// `A B C` (`B` is a register of the frame `C` levels out, `C` an immediate; `OuterRef`)
    AOuter,
    /// `A B C` (`C` is the number of an intrinsic, whose shape says what `A` and `B` are; `Intrinsic`)
    Intrinsic,
}

impl Shape {
    /// The second register of the cursor (`docs/spec/bytecode.md` 2.8) among the operands `a` and
    /// `b` of an instruction of this shape, or `None` for a shape without a cursor. The short form
    /// holds a cursor only when this register fits in 8 bits (1.2): the fast path of the VM reads
    /// it in the window of 256 registers.
    pub fn cursor_end(self, a: u32, b: u32) -> Option<u32> {
        match self {
            Shape::ACursorBC | Shape::ACursorSlotC | Shape::ACursorBType => Some(a + 1),
            Shape::ABCursorSlot | Shape::ABCursorType => Some(b + 1),
            _ => Option::None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpInfo {
    /// The instruction's number: 0 to 255 in the `op` of its word, 256 to 511 after `Ext`
    /// (`docs/spec/bytecode.md` 1.2), or the number of an intrinsic (`C` of `Intrinsic`; 2.19)
    pub code: u16,
    pub name: &'static str,
    pub shape: Shape,
}

macro_rules! ops {
    ($t:ty; $($konst:ident = $code:expr, $name:literal, $shape:ident;)*) => {
        $(pub const $konst: $t = $code;)*
        pub const TABLE: &[OpInfo] = &[$(OpInfo { code: $code, name: $name, shape: Shape::$shape }),*];
    };
    // The instructions 0 to 255 also as the `op` of their word (`word`)
    ($t:ty, word; $($konst:ident = $code:expr, $name:literal, $shape:ident;)*) => {
        ops!($t; $($konst = $code, $name, $shape;)*);

        /// The instructions numbered 0 to 255 as the 8-bit `op` of their word
        /// (`bytecode::word_op`). The VM dispatches on these: with the `u16` numbers, the
        /// execution loop held the number in a machine register of its own and its code moved,
        /// which slowed it down by about 10% (M4-109; `docs/notes/benchmark-m4c.md`).
        pub mod word {
            $(pub const $konst: u8 = $code;)*
        }
    };
}

/// The number an instruction after the prefix `Ext` starts from (`docs/spec/bytecode.md` 1.2): the
/// `op` of its word plus this.
pub const EXT_BASE: u16 = 256;

/// One past the largest instruction number (`Ext` with `op` 255).
pub const LIMIT: u16 = 512;

ops! {
    u16, word;
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
    NEW = 13, "New", ABx;
    OBJ_GET = 14, "ObjGet", ABSlot;
    OBJ_GET_REF = 15, "ObjGetRef", ABSlot;
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
    OBJ_SET = 44, "ObjSet", ASlotC;
    OBJ_SET_REF = 45, "ObjSetRef", ASlotC;
    CALL_VIRTUAL = 46, "CallVirtual", ABx;
    CALL_METHOD = 47, "CallMethod", ABx;
    EQ_INT = 48, "EqInt", ABC;
    LT_INT = 49, "LtInt", ABC;
    LE_INT = 50, "LeInt", ABC;
    LT_UINT = 51, "LtUInt", ABC;
    LE_UINT = 52, "LeUInt", ABC;
    NOT_BOOL = 53, "NotBool", AB;
    ASSIGN_OBJ = 54, "AssignObj", AB;
    ADD_REF_OBJ = 55, "AddRefObj", A;
    RELEASE_OBJ = 56, "ReleaseObj", A;
    LOAD_WEAK = 57, "LoadWeak", AB;
    IS_CLASS = 58, "IsClass", ABx;
    AS_CLASS = 59, "AsClass", ABx;
    CALL_VIRTUAL_CLASS = 60, "CallVirtualClass", ABx;
    CALL_INTF = 61, "CallIntf", ABx;
    CALL_INDIRECT = 62, "CallIndirect", AB;
    CALL_CLOSURE = 63, "CallClosure", AB;
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
    APPEND_STR = 75, "AppendStr", ABC;
    APPEND_U8 = 76, "AppendU8", ABC;
    CELL_GET = 77, "CellGet", AB;
    CELL_GET_REF = 78, "CellGetRef", AB;
    CELL_SET = 79, "CellSet", AB;
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
    // `docs/spec/bytecode.md` 2.23 (the units of the standard library only)
    HASH_FIND = 90, "HashFind", ABRange5;
    STR_COPY = 92, "StrCopy", ABRange3;
    STR_POS = 93, "StrPos", ABRange2;
    STR_UPPER_ASCII = 94, "StrUpperAscii", AB;
    STR_LOWER_ASCII = 95, "StrLowerAscii", AB;
    // `docs/spec/bytecode.md` 2.23 (the units of the standard library only)
    HASH_LOOKUP_INT = 96, "HashLookupInt", ABRange2Slot;
    STR_PAD_LEFT = 97, "StrPadLeft", ABC;
    HASH_LOOKUP_STR = 98, "HashLookupStr", ABRange2Slot;
    STR_TO_INT_IO = 99, "StrToIntIO", ABCImm;
    UP_CASE_CHAR = 100, "UpCaseChar", AB;
    CAP_GET = 101, "CapGet", ABSlot;
    MUL_INT_I = 102, "MulIntI", ABsCImm;
    DIV_INT_I = 103, "DivIntI", ABsCImm;
    MOD_INT_I = 104, "ModIntI", ABsCImm;
    AND_INT_I = 105, "AndIntI", ABsCImm;
    OR_INT_I = 106, "OrIntI", ABsCImm;
    XOR_INT_I = 107, "XorIntI", ABsCImm;
    SHL_INT_I = 108, "ShlIntI", ABsCImm;
    SHR_INT_I = 109, "ShrIntI", ABsCImm;
    SHL_INT32_I = 110, "ShlInt32I", ABsCImm;
    SHR_INT32_I = 111, "ShrInt32I", ABsCImm;
    CALL = 112, "Call", ABx;
    CALL_HOST = 113, "CallHost", ABx;
    RET = 114, "Ret", A;
    RET_VOID = 115, "RetVoid", None;
    HALT = 116, "Halt", A;
    CELL_SET_REF = 117, "CellSetRef", AB;
    JMP_IF_EQ_INT_I = 118, "JmpIfEqIntI", AsBsC;
    JMP_IF_NE_INT_I = 119, "JmpIfNeIntI", AsBsC;
    JMP_IF_LT_INT_I = 120, "JmpIfLtIntI", AsBsC;
    JMP_IF_LE_INT_I = 121, "JmpIfLeIntI", AsBsC;
    JMP_IF_GT_INT_I = 122, "JmpIfGtIntI", AsBsC;
    JMP_IF_GE_INT_I = 123, "JmpIfGeIntI", AsBsC;
    FOR_LOOP_UP = 124, "ForLoopUp", ABsC;
    FOR_LOOP_DOWN = 125, "ForLoopDown", ABsC;
    CONCAT_STR_N = 126, "ConcatStrN", ABList;
    // `docs/spec/bytecode.md` 2.8 (flat places)
    ELEM_AT = 127, "ElemAt", ACursorBC;
    ASSIGN_REF = 128, "AssignRef", AB;
    MOVE_REF = 129, "MoveRef", AB;
    ADD_REF = 130, "AddRef", A;
    RELEASE = 131, "Release", A;
    STORE_GLOBAL_REF = 132, "StoreGlobalRef", ABx;
    STORE_REF_REF = 133, "StoreRefRef", AB;
    CLEAR_REF = 134, "ClearRef", A;
    UNLEND = 135, "Unlend", None;
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
    LEND = 154, "Lend", ABPath;
    LOAD_GLOBAL_REF = 155, "LoadGlobalRef", ABx;
    LOAD_REF_REF = 156, "LoadRefRef", AB;
    UNIQUE_VAL = 158, "UniqueVal", A;
    MATERIALIZE = 159, "Materialize", ABx;
    REF_FIELD_GET = 160, "RefFieldGet", ABSlot;
    RAISE = 161, "Raise", A;
    RAISE_OUTER = 162, "RaiseOuter", AB;
    END_FINALLY = 163, "EndFinally", AJumps;
    REF_FIELD_GET_REF = 164, "RefFieldGetRef", ABSlot;
    REF_FIELD_SET = 165, "RefFieldSet", ASlotCType;
    EXC_INFO = 166, "ExcInfo", ABSlot;
    REF_FIELD_SET_REF = 167, "RefFieldSetRef", ASlotCType;
    LEND_PLACE = 168, "LendPlace", ABPathType;
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
    // `docs/spec/bytecode.md` 2.9 (the elements of a dynamic array of bytes)
    BYTES_GET = 216, "BytesGet", ABC;
    BYTES_SET = 217, "BytesSet", ABC;
    // `docs/spec/equality.md` 8.1
    HASH_INT = 224, "HashInt", AB;
    HASH_F64 = 225, "HashF64", AB;
    HASH_STR = 226, "HashStr", AB;
    HASH_MIX = 227, "HashMix", ABC;
    KEY_EQ_F64 = 228, "KeyEqF64", ABC;
    AGG_LOAD = 229, "AggLoad", ABCursorType;
    CALL_EXT = 230, "CallExt", ABx;
    U8_LEN = 232, "U8Len", AB;
    U8_INDEX = 233, "U8Index", ABC;
    CONCAT_U8 = 234, "ConcatU8", ABC;
    CMP_U8 = 235, "CmpU8", ABC;
    EQ_U8 = 236, "EqU8", ABC;
    ADD_INT32 = 239, "AddInt32", ABC;
    SUB_INT32 = 240, "SubInt32", ABC;
    MUL_INT32 = 241, "MulInt32", ABC;
    ADD_INT32_I = 242, "AddInt32I", ABsCImm;
    MUL_INT32_I = 243, "MulInt32I", ABsCImm;
    ARR_CONCAT = 244, "ArrConcat", ABx;
    ARR_INSERT = 245, "ArrInsert", ABx;
    ARR_INSERT_ARR = 246, "ArrInsertArr", ABx;
    ARR_DELETE = 247, "ArrDelete", ABx;
    FORMAT = 248, "Format", ABPairs;
    SLOT_GET = 249, "SlotGet", ABCursorSlot;
    SLOT_GET_REF = 250, "SlotGetRef", ABCursorSlot;
    SLOT_SET = 251, "SlotSet", ACursorSlotC;
    SLOT_SET_REF = 252, "SlotSetRef", ACursorSlotC;
    AGG_STORE = 253, "AggStore", ACursorBType;
    // The prefix of the instructions numbered 256 to 511 (`docs/spec/bytecode.md` 1.2)
    EXT = 254, "Ext", None;
    INTRINSIC = 255, "Intrinsic", Intrinsic;
}

/// The intrinsics: operations `Intrinsic A B C` selects by `C` (`docs/spec/bytecode.md` 2.19).
/// Each has the shape of the instruction it was before (only `None`, `A`, `AB` and the register
/// ranges, so that `C` is free for the number).
pub mod intr {
    use super::{OpInfo, Shape};

    ops! {
        u8;
        INT_TO_HEX = 0, "IntToHex", ABRange2;
        STR_TO_BOOL = 1, "StrToBool", AB;
        BOOL_STR = 2, "BoolStr", ABRange2;
        STR_TRIM_LEFT = 3, "StrTrimLeft", AB;
        STR_TRIM_RIGHT = 4, "StrTrimRight", AB;
        SAME_TEXT = 5, "SameText", ABRange2;
        COMPARE_TEXT = 6, "CompareText", ABRange2;
        STR_REPLACE = 7, "StrReplace", ABRange4;
        STR_OF_CHAR = 8, "StrOfChar", ABRange2;
        QUOTED_STR = 9, "QuotedStr", AB;
        TRY_STR_TO_INT = 10, "TryStrToInt", ABRange2;
        ARG_COUNT = 11, "ArgCount", A;
        ARG_GET = 12, "ArgGet", AB;
        READ_WORD = 13, "ReadWord", A;
        READ_REST = 14, "ReadRest", A;
        READ_CHAR = 15, "ReadChar", A;
        READ_LN_BEGIN = 16, "ReadLnBegin", None;
        READ_LN_END = 17, "ReadLnEnd", None;
        IN_EOF = 18, "InEof", A;
        IN_EOLN = 19, "InEoln", A;
        FLUSH_OUT = 20, "FlushOut", None;
        F64_TO_STR = 21, "F64ToStr", AB;
        F32_TO_STR = 22, "F32ToStr", AB;
        CURR_TO_STR = 23, "CurrToStr", AB;
        F64_TO_FIXED = 24, "F64ToFixed", ABRange2;
        CURR_TO_FIXED = 25, "CurrToFixed", ABRange2;
        STR_TO_F64 = 26, "StrToF64", AB;
        U8_SET_LEN = 27, "U8SetLen", AB;
        STR_INSERT = 28, "StrInsert", ABRange2;
        STR_DELETE = 29, "StrDelete", ABRange2;
        U8_INSERT = 30, "U8Insert", ABRange2;
        U8_DELETE = 31, "U8Delete", ABRange2;
        STR_UPPER = 32, "StrUpper", ABRange2;
        STR_LOWER = 33, "StrLower", ABRange2;
        ANSI_SAME_TEXT = 34, "AnsiSameText", ABRange3;
        ANSI_COMPARE_TEXT = 35, "AnsiCompareText", ABRange3;
        STR_TO_F64_IO = 36, "StrToF64IO", AB;
        STR_TO_F32_IO = 37, "StrToF32IO", AB;
        STR_TO_CURR_IO = 38, "StrToCurrIO", AB;
        // `docs/spec/objects.md` 14.3
        NEW_OF = 39, "NewOf", AB;
        CLASS_TYPE = 40, "ClassType", AB;
        CLASS_NAME = 41, "ClassName", AB;
        CLASS_REF_NAME = 42, "ClassRefName", AB;
        CLASS_PARENT = 43, "ClassParent", AB;
        CLASS_INHERITS = 44, "ClassInherits", ABRange2;
        REF_COUNT = 45, "RefCount", AB;
        FREE = 46, "Free", A;
        DESTROY = 47, "Destroy", A;
        DESTROY_FAILED = 48, "DestroyFailed", A;
        DTOR_ENTER = 49, "DtorEnter", AB;
        // `docs/spec/objects.md` 14.7
        IS_INTF = 50, "IsIntf", AB;
        AS_INTF = 51, "AsIntf", AB;
        CLASS_SUPPORTS = 52, "ClassSupports", ABRange2;
        EXCEPT_OBJECT = 53, "ExceptObject", A;
        NEW_CELL = 54, "NewCell", A;
        NEW_CELL_REF = 55, "NewCellRef", A;
        NEW_CELL_OBJ = 56, "NewCellObj", A;
        CELL_REF = 57, "CellRef", AB;
        MAKE_CLOSURE = 58, "MakeClosure", AB;
        CLOSURE_WITH = 59, "ClosureWith", ABRange2;
        // `docs/spec/objects.md` 14.9
        METHOD_REF = 60, "MethodRef", ABRange2;
        VIRTUAL_REF = 61, "VirtualRef", ABRange2;
        CLASS_METHOD_REF = 62, "ClassMethodRef", ABRange2;
        CLASS_VIRTUAL_REF = 63, "ClassVirtualRef", ABRange2;
        ROUTINE_REF = 64, "RoutineRef", AB;
        CALLABLE_ASSIGNED = 65, "CallableAssigned", AB;
        CALLABLE_EQUAL = 66, "CallableEqual", ABRange2;
        AS_CLASS_REF = 67, "AsClassRef", AB;
        // `docs/spec/equality.md` 8.1
        OBJ_ALIVE = 68, "ObjAlive", AB;
        HASH_UTF8 = 69, "HashUtf8", AB;
        HASH_SET_BIG = 70, "HashSetBig", AB;
        CALLABLE_KEY_EQ = 71, "CallableKeyEq", ABRange2;
        CALLABLE_HASH = 72, "CallableHash", AB;
        // `docs/spec/bytecode.md` 2.22 (the units of the standard library only)
        ARRAY_MOVE_RANGE = 73, "ArrayMoveRange", ABRange3;
        IS_REF_OBJ = 74, "IsRefObj", AB;
        IS_OVERRIDDEN = 75, "IsOverridden", ABRange3;
        // `docs/spec/bytecode.md` 2.8: a lent flat place taken out into a block
        PLACE_BOX = 76, "PlaceBox", AB;
        // Instructions moved here to free their numbers (M4-109; `docs/spec/bytecode.md` 2.19)
        STR_TO_INT = 77, "StrToInt", AB;
        STR_TO_INT64 = 78, "StrToInt64", AB;
        STR_TRIM = 79, "StrTrim", AB;
        STR_TO_INT_DEF = 80, "StrToIntDef", ABRange2;
        STR_SET_LEN = 81, "StrSetLen", AB;
        NEG_CURR = 82, "NegCurr", AB;
        INT_TO_CURR = 83, "IntToCurr", AB;
        UINT_TO_CURR = 84, "UIntToCurr", AB;
        F64_TO_CURR = 85, "F64ToCurr", AB;
        CURR_TO_F64 = 86, "CurrToF64", AB;
        CURR_TO_F32 = 87, "CurrToF32", AB;
        TRUNC_CURR = 88, "TruncCurr", AB;
        ROUND_CURR = 89, "RoundCurr", AB;
        STR_TO_U8 = 90, "StrToU8", AB;
        U8_TO_STR = 91, "U8ToStr", AB;
        U8_COPY = 92, "U8Copy", ABRange3;
        U8_POS = 93, "U8Pos", ABRange2;
        SLICE_RANGE = 94, "SliceRange", ABRange3;
        // `docs/spec/bytecode.md` 2.23 (the units of the standard library only)
        HASH_RELINK = 95, "HashRelink", ABRange3;
        // `docs/spec/bytecode.md` 2.22 (the units of the standard library only)
        ARRAY_SORT_RANGE = 96, "ArraySortRange", ABRange3;
        // `docs/spec/bytecode.md` 2.22 (the floating-point functions)
        MATH_F64 = 97, "MathF64", ABRange2;
        MATH_F64X2 = 98, "MathF64x2", ABRange3;
        // `docs/spec/bytecode.md` 2.22 (the units of the standard library only)
        UNICODE_CATEGORY = 101, "UnicodeCategory", AB;
        CODE_POINT_CLASS = 102, "CodePointClass", AB;
        NUMERIC_VALUE = 103, "NumericValue", AB;
        CHAR_CLASS = 105, "CharClass", AB;
        // `docs/spec/bytecode.md` 2.22 (the units of the standard library only)
        BYTES_COPY = 109, "BytesCopy", ABRange4;
        BYTES_FILL = 110, "BytesFill", ABRange3;
        // `docs/spec/bytecode.md` 2.22 (the units of the standard library only)
        CONSOLE_READ = 113, "ConsoleRead", ABRange3;
        CONSOLE_WRITE = 114, "ConsoleWrite", ABRange3;
        CONSOLE_WRITE_ERROR = 115, "ConsoleWriteError", ABRange3;
        // `docs/spec/bytecode.md` 2.22 (the units of the standard library only)
        CLOCK_NOW = 117, "ClockNow", A;
        LOCAL_OFFSET = 118, "LocalOffset", AB;
        CONVERT_ERROR = 119, "ConvertError", ABRange2;
        CLOCK_MONO = 120, "ClockMono", A;
        // `docs/spec/bytecode.md` 2.24 (`TValue`; the part of `System` written in Pascal, and
        // `Format` of an `array of const`)
        FORMAT_VALUES = 121, "FormatValues", ABRange2;
        VALUE_TO_STRING = 122, "ValueToString", ABRange3;
        VALUE_READ = 123, "ValueRead", ABRange4;
        VALUE_MATCH = 124, "ValueMatch", ABRange4;
        VALUE_READ_STRING = 125, "ValueReadString", ABRange3;
        TYPE_KIND_OF = 126, "TypeKindOf", AB;
        VALUE_OF_OBJECT = 127, "ValueOfObject", ABRange2;
        VALUE_ARRAY = 128, "ValueArray", ABRange5;
        // `docs/spec/bytecode.md` 2.24 (the type information section; the units of the
        // standard library only)
        RTTI_TYPE_FIELD = 129, "RttiTypeField", ABRange3;
        RTTI_MEMBER_FIELD = 130, "RttiMemberField", ABRange3;
        RTTI_NAME = 131, "RttiName", AB;
        RTTI_FIND_TYPE = 132, "RttiFindType", ABRange2;
        // `docs/spec/bytecode.md` 2.24 (`System.Rtti`: the fields and properties read and
        // written, the class of a class reference)
        RTTI_FIELD_GET = 133, "RttiFieldGet", ABRange3;
        RTTI_FIELD_SET = 134, "RttiFieldSet", ABRange4;
        RTTI_CLASS_TYPE = 135, "RttiClassType", ABRange2;
        // `docs/spec/bytecode.md` 2.24 (`TRttiMethod.Invoke`, the properties read and written by
        // their methods, the attributes)
        RTTI_METHOD_REF = 141, "RttiMethodRef", ABRange2;
        RTTI_BRIDGE = 142, "RttiBridge", ABRange3;
        RTTI_ATTR_FUNC = 143, "RttiAttrFunc", AB;
        RTTI_PROP_REF = 144, "RttiPropRef", ABRange3;
        // `docs/spec/bytecode.md` 2.19 (the generated routines of the tests of classes)
        ADD_SUPPRESSED = 145, "AddSuppressed", AB;
        // `docs/spec/bytecode.md` 2.10 (`Assert` under `{$C+}`; `docs/spec/semantics.md` 5.4)
        ASSERT_VALUE = 147, "AssertValue", ABRange3;
        // `docs/spec/bytecode.md` 2.22 (the virtual clock of `Puy.Testing` and `Sleep`)
        CLOCK_CONTROL = 151, "ClockControl", ABRange2;
        CLOCK_SLEEP = 152, "ClockSleep", AB;
        // `docs/spec/bytecode.md` 2.24 (`TRttiMethod.Invoke`'s `var` arguments written back)
        RTTI_ARG_SET = 153, "RttiArgSet", ABRange3;
        // `docs/spec/bytecode.md` 2.22 (`System` finalization)
        REQUEST_LEAK_REPORT = 154, "RequestLeakReport", None;
    }

    /// Looks up the information of an intrinsic.
    pub fn info(id: u32) -> Option<&'static OpInfo> {
        TABLE.iter().find(|i| u32::from(i.code) == id)
    }
}

/// The name and shape of the instruction `opc` with the third operand `c`: an intrinsic's own for
/// `Intrinsic` (`docs/spec/bytecode.md` 2.19), the opcode's otherwise.
pub fn resolve(opc: u16, c: u32) -> Option<&'static OpInfo> {
    if opc == INTRINSIC {
        intr::info(c)
    } else {
        info(opc)
    }
}

/// Looks up the information of an instruction number (0 to 511; 1.2).
pub fn info(code: u16) -> Option<&'static OpInfo> {
    #[cfg(test)]
    if code == TEST_EXT {
        return Some(&TEST_EXT_INFO);
    }
    LOOKUP
        .get(usize::from(code))
        .copied()
        .flatten()
        .map(|i| &TABLE[i])
}

static LOOKUP: [Option<usize>; LIMIT as usize] = {
    let mut t = [None; LIMIT as usize];
    let mut i = 0;
    while i < TABLE.len() {
        t[TABLE[i].code as usize] = Some(i);
        i += 1;
    }
    t
};

/// An instruction past 255 for the unit tests of the `Ext` path (`docs/spec/bytecode.md` 1.2;
/// no instruction uses `Ext` yet): `TestExt A B C` is `rA := rB - rC` (wrapping), so that each of
/// its three registers shows in the result. Its word holds the `op` of `AddInt`, so that a decoder
/// that drops `Ext` runs another instruction.
#[cfg(test)]
pub const TEST_EXT: u16 = EXT_BASE + ADD_INT;

#[cfg(test)]
static TEST_EXT_INFO: OpInfo = OpInfo {
    code: TEST_EXT,
    name: "TestExt",
    shape: Shape::ABC,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_works() {
        assert_eq!(info(CALL).unwrap().name, "Call");
        assert_eq!(info(CELL_SET_REF).unwrap().name, "CellSetRef");
        assert_eq!(info(CAP_GET).unwrap().name, "CapGet");
        // The numbers of the exception instructions replaced by the class ones
        // (`docs/spec/objects.md` 14.8) went to the fields read through a reference slot
        // (`docs/spec/bytecode.md` 2.8), except 168
        for (code, name) in [
            (160, "RefFieldGet"),
            (164, "RefFieldGetRef"),
            (165, "RefFieldSet"),
            (167, "RefFieldSetRef"),
        ] {
            assert_eq!(info(code).map(|i| i.name), Some(name), "{code}");
        }
        // The rest of the numbers left (M4-63)
        for (code, name) in [
            (127, "ElemAt"),
            (168, "LendPlace"),
            (229, "AggLoad"),
            (249, "SlotGet"),
            (253, "AggStore"),
        ] {
            assert_eq!(info(code).map(|i| i.name), Some(name), "{code}");
        }
        for (i, a) in TABLE.iter().enumerate() {
            for b in &TABLE[i + 1..] {
                assert_ne!(a.code, b.code);
            }
        }
    }

    // The numbers freed by moving rare instructions to `Intrinsic` (M4-109;
    // `docs/spec/bytecode.md` 2.19) stay free but 90, 96 and 98 (the hash table lookups; 2.23;
    // 91 was `HashFindKey` until M4-132), 216, 217 (the elements of a dynamic array of bytes;
    // 2.9) and 230 (`CallExt`; M5-7), `Ext` took one of them, and no instruction is numbered past
    // 255 yet besides the one of the unit tests.
    #[test]
    fn freed_numbers_and_ext() {
        for code in [91, 157, 218, 219, 220, 221, 222, 223, 231, 237, 238] {
            assert!(info(code).is_none(), "{code}");
        }
        assert_eq!(info(216).map(|i| i.name), Some("BytesGet"));
        assert_eq!(info(217).map(|i| i.name), Some("BytesSet"));
        assert_eq!(info(90).map(|i| i.name), Some("HashFind"));
        assert_eq!(info(96).map(|i| i.name), Some("HashLookupInt"));
        assert_eq!(info(98).map(|i| i.name), Some("HashLookupStr"));
        assert_eq!(info(230).map(|i| i.name), Some("CallExt"));
        assert_eq!(info(254).map(|i| i.name), Some("Ext"));
        assert_eq!(info(TEST_EXT).map(|i| i.name), Some("TestExt"));
        for code in EXT_BASE..LIMIT {
            assert!(code == TEST_EXT || info(code).is_none(), "{code}");
        }
        assert!(info(LIMIT).is_none());
        assert!(TABLE.iter().all(|i| i.code < EXT_BASE));
    }

    // Each intrinsic's number is unique and increasing, so `resolve` finds it by `C`, and an
    // unknown number (one not assigned yet) resolves to nothing.
    #[test]
    fn intrinsics_resolve_by_number() {
        for (i, x) in intr::TABLE.iter().enumerate() {
            if let Some(next) = intr::TABLE.get(i + 1) {
                assert!(x.code < next.code, "{}", x.name);
            }
            assert!(
                matches!(
                    x.shape,
                    Shape::None
                        | Shape::A
                        | Shape::AB
                        | Shape::ABRange2
                        | Shape::ABRange3
                        | Shape::ABRange4
                        | Shape::ABRange5
                ),
                "{}",
                x.name
            );
        }
        assert_eq!(resolve(INTRINSIC, 0).unwrap().name, "IntToHex");
        assert_eq!(resolve(INTRINSIC, 38).unwrap().name, "StrToCurrIO");
        assert_eq!(resolve(INTRINSIC, 45).unwrap().name, "RefCount");
        assert_eq!(resolve(INTRINSIC, 50).unwrap().name, "IsIntf");
        assert_eq!(resolve(INTRINSIC, 72).unwrap().name, "CallableHash");
        assert_eq!(resolve(INTRINSIC, 74).unwrap().name, "IsRefObj");
        assert_eq!(resolve(INTRINSIC, 75).unwrap().name, "IsOverridden");
        assert_eq!(resolve(INTRINSIC, 76).unwrap().name, "PlaceBox");
        assert_eq!(resolve(INTRINSIC, 77).unwrap().name, "StrToInt");
        assert_eq!(resolve(INTRINSIC, 94).unwrap().name, "SliceRange");
        assert_eq!(resolve(INTRINSIC, 95).unwrap().name, "HashRelink");
        assert_eq!(resolve(INTRINSIC, 96).unwrap().name, "ArraySortRange");
        assert_eq!(resolve(INTRINSIC, 97).unwrap().name, "MathF64");
        assert_eq!(resolve(INTRINSIC, 98).unwrap().name, "MathF64x2");
        assert!(resolve(INTRINSIC, 99).is_none());
        assert_eq!(resolve(INTRINSIC, 101).unwrap().name, "UnicodeCategory");
        assert_eq!(resolve(INTRINSIC, 103).unwrap().name, "NumericValue");
        assert_eq!(resolve(INTRINSIC, 105).unwrap().name, "CharClass");
        assert!(resolve(INTRINSIC, 106).is_none());
        assert_eq!(resolve(INTRINSIC, 109).unwrap().name, "BytesCopy");
        assert_eq!(resolve(INTRINSIC, 110).unwrap().name, "BytesFill");
        assert!(resolve(INTRINSIC, 111).is_none());
        assert_eq!(resolve(INTRINSIC, 113).unwrap().name, "ConsoleRead");
        assert_eq!(resolve(INTRINSIC, 114).unwrap().name, "ConsoleWrite");
        assert_eq!(resolve(INTRINSIC, 115).unwrap().name, "ConsoleWriteError");
        assert!(resolve(INTRINSIC, 116).is_none());
        assert_eq!(resolve(INTRINSIC, 117).unwrap().name, "ClockNow");
        assert_eq!(resolve(INTRINSIC, 118).unwrap().name, "LocalOffset");
        assert_eq!(resolve(INTRINSIC, 119).unwrap().name, "ConvertError");
        assert_eq!(resolve(INTRINSIC, 120).unwrap().name, "ClockMono");
        assert_eq!(resolve(INTRINSIC, 121).unwrap().name, "FormatValues");
        assert_eq!(resolve(INTRINSIC, 128).unwrap().name, "ValueArray");
        assert_eq!(resolve(INTRINSIC, 151).unwrap().name, "ClockControl");
        assert_eq!(resolve(INTRINSIC, 152).unwrap().name, "ClockSleep");
        assert_eq!(resolve(INTRINSIC, 53).unwrap().name, "ExceptObject");
        assert_eq!(resolve(INTRINSIC, 46).unwrap().name, "Free");
        assert_eq!(resolve(CALL, 0).unwrap().name, "Call");
    }
}
