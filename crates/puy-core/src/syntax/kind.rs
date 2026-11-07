//! `SyntaxKind`: token and node kinds (`docs/spec/grammar.md` chapter 2).
//!
//! The order follows the spec; kinds added in M2 or later go at the end of each group. Numeric values are not stable
//! (dumps show only the names).

/// Token and node kinds. Maps to rowan's `SyntaxKind` (`u16`).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // ---- Trivia ----
    WHITESPACE,
    NEWLINE,
    LINE_COMMENT,
    DOC_COMMENT,
    BLOCK_COMMENT,
    DIRECTIVE,
    INACTIVE,
    SHEBANG,
    BOM,

    // ---- Identifiers and literals ----
    IDENT,
    INT_LIT,
    FLOAT_LIT,
    STRING_LIT,

    // ---- Symbols ----
    PLUS,
    MINUS,
    STAR,
    SLASH,
    EQ,
    NE,
    LT,
    GT,
    LE,
    GE,
    ASSIGN,
    COLON,
    SEMICOLON,
    COMMA,
    DOT,
    DOTDOT,
    L_PAREN,
    R_PAREN,
    L_BRACK,
    R_BRACK,
    CARET,
    AT,

    // ---- Reserved words (lexical.md 3.3; 64 of them) ----
    AND_KW,
    ARRAY_KW,
    AS_KW,
    ASM_KW,
    BEGIN_KW,
    CASE_KW,
    CLASS_KW,
    CONST_KW,
    CONSTRUCTOR_KW,
    DESTRUCTOR_KW,
    DISPINTERFACE_KW,
    DIV_KW,
    DO_KW,
    DOWNTO_KW,
    ELSE_KW,
    END_KW,
    EXCEPT_KW,
    EXPORTS_KW,
    FILE_KW,
    FINALIZATION_KW,
    FINALLY_KW,
    FOR_KW,
    FUNCTION_KW,
    GOTO_KW,
    IF_KW,
    IMPLEMENTATION_KW,
    IN_KW,
    INHERITED_KW,
    INITIALIZATION_KW,
    INLINE_KW,
    INTERFACE_KW,
    IS_KW,
    LABEL_KW,
    LIBRARY_KW,
    MOD_KW,
    NIL_KW,
    NOT_KW,
    OBJECT_KW,
    OF_KW,
    OR_KW,
    PACKED_KW,
    PROCEDURE_KW,
    PROGRAM_KW,
    PROPERTY_KW,
    RAISE_KW,
    RECORD_KW,
    REPEAT_KW,
    RESOURCESTRING_KW,
    SET_KW,
    SHL_KW,
    SHR_KW,
    STRING_KW,
    THEN_KW,
    THREADVAR_KW,
    TO_KW,
    TRY_KW,
    TYPE_KW,
    UNIT_KW,
    UNTIL_KW,
    USES_KW,
    VAR_KW,
    WHILE_KW,
    WITH_KW,
    XOR_KW,

    // ---- Context-sensitive directives (the parser re-tags them from IDENT) ----
    ABSOLUTE_KW,
    ABSTRACT_KW,
    AUTOMATED_KW,
    CDECL_KW,
    DEFAULT_KW,
    DEPRECATED_KW,
    DYNAMIC_KW,
    EXPERIMENTAL_KW,
    EXPORT_KW,
    EXTERNAL_KW,
    FAR_KW,
    FINAL_KW,
    FORWARD_KW,
    HELPER_KW,
    IMPLEMENTS_KW,
    INDEX_KW,
    LOCAL_KW,
    MESSAGE_KW,
    NAME_KW,
    NEAR_KW,
    NODEFAULT_KW,
    ON_KW,
    OPERATOR_KW,
    OUT_KW,
    OVERLOAD_KW,
    OVERRIDE_KW,
    PASCAL_KW,
    PLATFORM_KW,
    PRIVATE_KW,
    PROTECTED_KW,
    PUBLIC_KW,
    PUBLISHED_KW,
    READ_KW,
    READONLY_KW,
    REFERENCE_KW,
    REGISTER_KW,
    REINTRODUCE_KW,
    SAFECALL_KW,
    SEALED_KW,
    STATIC_KW,
    STDCALL_KW,
    STORED_KW,
    STRICT_KW,
    UNSAFE_KW,
    VARARGS_KW,
    VIRTUAL_KW,
    WINAPI_KW,
    WRITE_KW,
    WRITEONLY_KW,

    // ---- Other tokens ----
    /// Characters that are lexical errors (a token). Shown in dumps under the same name as the `ERROR` node.
    ERROR,
    EOF,

    // ---- Nodes ----
    SOURCE_FILE,
    PROGRAM,
    PROGRAM_HEADER,
    PROGRAM_PARAMS,
    SCRIPT,
    USES_CLAUSE,
    USES_ITEM,
    QUALIFIED_NAME,
    CONST_SECTION,
    CONST_DECL,
    VAR_SECTION,
    VAR_DECL,
    IDENT_LIST,
    TYPE_REF,
    ROUTINE_DECL,
    ROUTINE_HEADER,
    PARAM_LIST,
    PARAM_GROUP,
    RETURN_TYPE,
    ROUTINE_DIRECTIVE,
    ROUTINE_BODY,
    COMPOUND_STMT,
    ASSIGN_STMT,
    EXPR_STMT,
    INLINE_VAR_DECL,
    INLINE_CONST_DECL,
    IF_STMT,
    ELSE_BRANCH,
    WHILE_STMT,
    REPEAT_STMT,
    FOR_STMT,
    FOR_VAR,
    LITERAL,
    NAME_REF,
    PAREN_EXPR,
    UNARY_EXPR,
    BINARY_EXPR,
    CALL_EXPR,
    ARG_LIST,
    ARG,
    INDEX_EXPR,
    FIELD_EXPR,
    DEREF_EXPR,
    NIL_EXPR,
    INHERITED_EXPR,
    /// Node wrapping tokens skipped by error recovery.
    ERROR_NODE,
    #[doc(hidden)]
    __LAST,
}

use SyntaxKind::*;

impl SyntaxKind {
    /// Name shown in dumps. The `ERROR_NODE` node is shown as `ERROR`, the same as the token.
    pub fn name(self) -> &'static str {
        if self == ERROR_NODE {
            return "ERROR";
        }
        // The `Debug` output is the variant name itself
        KIND_NAMES[self as usize]
    }

    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            WHITESPACE
                | NEWLINE
                | LINE_COMMENT
                | DOC_COMMENT
                | BLOCK_COMMENT
                | DIRECTIVE
                | INACTIVE
                | SHEBANG
                | BOM
        )
    }

    /// What the parser treats like trivia (trivia and lexical `ERROR` tokens).
    pub fn is_skipped_by_parser(self) -> bool {
        self.is_trivia() || self == ERROR
    }

    pub fn is_reserved_word(self) -> bool {
        (AND_KW..=XOR_KW).contains(&self)
    }

    pub fn is_node(self) -> bool {
        self >= SOURCE_FILE && self < __LAST
    }

    pub fn from_raw(raw: u16) -> SyntaxKind {
        assert!(raw < __LAST as u16);
        // SAFETY: repr(u16), and the range has been checked
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw) }
    }

    /// Table of reserved words (lowercase). An identifier spelling without `&` that matches one of these has that kind.
    pub fn reserved_word(lower: &str) -> Option<SyntaxKind> {
        RESERVED_WORDS
            .iter()
            .find(|(w, _)| *w == lower)
            .map(|(_, k)| *k)
    }

    /// Table of context-sensitive directives (lowercase). Used by the parser when re-tagging from `IDENT`.
    pub fn contextual_directive(lower: &str) -> Option<SyntaxKind> {
        CONTEXTUAL_DIRECTIVES
            .iter()
            .find(|(w, _)| *w == lower)
            .map(|(_, k)| *k)
    }

    /// Spelling of reserved-word and symbol tokens (used in the `expected `…`` of diagnostics).
    pub fn token_text(self) -> Option<&'static str> {
        if let Some((w, _)) = RESERVED_WORDS.iter().find(|(_, k)| *k == self) {
            return Some(w);
        }
        if let Some((w, _)) = CONTEXTUAL_DIRECTIVES.iter().find(|(_, k)| *k == self) {
            return Some(w);
        }
        Some(match self {
            PLUS => "+",
            MINUS => "-",
            STAR => "*",
            SLASH => "/",
            EQ => "=",
            NE => "<>",
            LT => "<",
            GT => ">",
            LE => "<=",
            GE => ">=",
            ASSIGN => ":=",
            COLON => ":",
            SEMICOLON => ";",
            COMMA => ",",
            DOT => ".",
            DOTDOT => "..",
            L_PAREN => "(",
            R_PAREN => ")",
            L_BRACK => "[",
            R_BRACK => "]",
            CARET => "^",
            AT => "@",
            _ => return None,
        })
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

macro_rules! kind_names {
    ($($name:ident),* $(,)?) => {
        const KIND_NAMES: &[&str] = &[$(stringify!($name)),*];
    };
}

kind_names![
    WHITESPACE,
    NEWLINE,
    LINE_COMMENT,
    DOC_COMMENT,
    BLOCK_COMMENT,
    DIRECTIVE,
    INACTIVE,
    SHEBANG,
    BOM,
    IDENT,
    INT_LIT,
    FLOAT_LIT,
    STRING_LIT,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    EQ,
    NE,
    LT,
    GT,
    LE,
    GE,
    ASSIGN,
    COLON,
    SEMICOLON,
    COMMA,
    DOT,
    DOTDOT,
    L_PAREN,
    R_PAREN,
    L_BRACK,
    R_BRACK,
    CARET,
    AT,
    AND_KW,
    ARRAY_KW,
    AS_KW,
    ASM_KW,
    BEGIN_KW,
    CASE_KW,
    CLASS_KW,
    CONST_KW,
    CONSTRUCTOR_KW,
    DESTRUCTOR_KW,
    DISPINTERFACE_KW,
    DIV_KW,
    DO_KW,
    DOWNTO_KW,
    ELSE_KW,
    END_KW,
    EXCEPT_KW,
    EXPORTS_KW,
    FILE_KW,
    FINALIZATION_KW,
    FINALLY_KW,
    FOR_KW,
    FUNCTION_KW,
    GOTO_KW,
    IF_KW,
    IMPLEMENTATION_KW,
    IN_KW,
    INHERITED_KW,
    INITIALIZATION_KW,
    INLINE_KW,
    INTERFACE_KW,
    IS_KW,
    LABEL_KW,
    LIBRARY_KW,
    MOD_KW,
    NIL_KW,
    NOT_KW,
    OBJECT_KW,
    OF_KW,
    OR_KW,
    PACKED_KW,
    PROCEDURE_KW,
    PROGRAM_KW,
    PROPERTY_KW,
    RAISE_KW,
    RECORD_KW,
    REPEAT_KW,
    RESOURCESTRING_KW,
    SET_KW,
    SHL_KW,
    SHR_KW,
    STRING_KW,
    THEN_KW,
    THREADVAR_KW,
    TO_KW,
    TRY_KW,
    TYPE_KW,
    UNIT_KW,
    UNTIL_KW,
    USES_KW,
    VAR_KW,
    WHILE_KW,
    WITH_KW,
    XOR_KW,
    ABSOLUTE_KW,
    ABSTRACT_KW,
    AUTOMATED_KW,
    CDECL_KW,
    DEFAULT_KW,
    DEPRECATED_KW,
    DYNAMIC_KW,
    EXPERIMENTAL_KW,
    EXPORT_KW,
    EXTERNAL_KW,
    FAR_KW,
    FINAL_KW,
    FORWARD_KW,
    HELPER_KW,
    IMPLEMENTS_KW,
    INDEX_KW,
    LOCAL_KW,
    MESSAGE_KW,
    NAME_KW,
    NEAR_KW,
    NODEFAULT_KW,
    ON_KW,
    OPERATOR_KW,
    OUT_KW,
    OVERLOAD_KW,
    OVERRIDE_KW,
    PASCAL_KW,
    PLATFORM_KW,
    PRIVATE_KW,
    PROTECTED_KW,
    PUBLIC_KW,
    PUBLISHED_KW,
    READ_KW,
    READONLY_KW,
    REFERENCE_KW,
    REGISTER_KW,
    REINTRODUCE_KW,
    SAFECALL_KW,
    SEALED_KW,
    STATIC_KW,
    STDCALL_KW,
    STORED_KW,
    STRICT_KW,
    UNSAFE_KW,
    VARARGS_KW,
    VIRTUAL_KW,
    WINAPI_KW,
    WRITE_KW,
    WRITEONLY_KW,
    ERROR,
    EOF,
    SOURCE_FILE,
    PROGRAM,
    PROGRAM_HEADER,
    PROGRAM_PARAMS,
    SCRIPT,
    USES_CLAUSE,
    USES_ITEM,
    QUALIFIED_NAME,
    CONST_SECTION,
    CONST_DECL,
    VAR_SECTION,
    VAR_DECL,
    IDENT_LIST,
    TYPE_REF,
    ROUTINE_DECL,
    ROUTINE_HEADER,
    PARAM_LIST,
    PARAM_GROUP,
    RETURN_TYPE,
    ROUTINE_DIRECTIVE,
    ROUTINE_BODY,
    COMPOUND_STMT,
    ASSIGN_STMT,
    EXPR_STMT,
    INLINE_VAR_DECL,
    INLINE_CONST_DECL,
    IF_STMT,
    ELSE_BRANCH,
    WHILE_STMT,
    REPEAT_STMT,
    FOR_STMT,
    FOR_VAR,
    LITERAL,
    NAME_REF,
    PAREN_EXPR,
    UNARY_EXPR,
    BINARY_EXPR,
    CALL_EXPR,
    ARG_LIST,
    ARG,
    INDEX_EXPR,
    FIELD_EXPR,
    DEREF_EXPR,
    NIL_EXPR,
    INHERITED_EXPR,
    ERROR_NODE,
];

const RESERVED_WORDS: &[(&str, SyntaxKind)] = &[
    ("and", AND_KW),
    ("array", ARRAY_KW),
    ("as", AS_KW),
    ("asm", ASM_KW),
    ("begin", BEGIN_KW),
    ("case", CASE_KW),
    ("class", CLASS_KW),
    ("const", CONST_KW),
    ("constructor", CONSTRUCTOR_KW),
    ("destructor", DESTRUCTOR_KW),
    ("dispinterface", DISPINTERFACE_KW),
    ("div", DIV_KW),
    ("do", DO_KW),
    ("downto", DOWNTO_KW),
    ("else", ELSE_KW),
    ("end", END_KW),
    ("except", EXCEPT_KW),
    ("exports", EXPORTS_KW),
    ("file", FILE_KW),
    ("finalization", FINALIZATION_KW),
    ("finally", FINALLY_KW),
    ("for", FOR_KW),
    ("function", FUNCTION_KW),
    ("goto", GOTO_KW),
    ("if", IF_KW),
    ("implementation", IMPLEMENTATION_KW),
    ("in", IN_KW),
    ("inherited", INHERITED_KW),
    ("initialization", INITIALIZATION_KW),
    ("inline", INLINE_KW),
    ("interface", INTERFACE_KW),
    ("is", IS_KW),
    ("label", LABEL_KW),
    ("library", LIBRARY_KW),
    ("mod", MOD_KW),
    ("nil", NIL_KW),
    ("not", NOT_KW),
    ("object", OBJECT_KW),
    ("of", OF_KW),
    ("or", OR_KW),
    ("packed", PACKED_KW),
    ("procedure", PROCEDURE_KW),
    ("program", PROGRAM_KW),
    ("property", PROPERTY_KW),
    ("raise", RAISE_KW),
    ("record", RECORD_KW),
    ("repeat", REPEAT_KW),
    ("resourcestring", RESOURCESTRING_KW),
    ("set", SET_KW),
    ("shl", SHL_KW),
    ("shr", SHR_KW),
    ("string", STRING_KW),
    ("then", THEN_KW),
    ("threadvar", THREADVAR_KW),
    ("to", TO_KW),
    ("try", TRY_KW),
    ("type", TYPE_KW),
    ("unit", UNIT_KW),
    ("until", UNTIL_KW),
    ("uses", USES_KW),
    ("var", VAR_KW),
    ("while", WHILE_KW),
    ("with", WITH_KW),
    ("xor", XOR_KW),
];

const CONTEXTUAL_DIRECTIVES: &[(&str, SyntaxKind)] = &[
    ("absolute", ABSOLUTE_KW),
    ("abstract", ABSTRACT_KW),
    ("automated", AUTOMATED_KW),
    ("cdecl", CDECL_KW),
    ("default", DEFAULT_KW),
    ("deprecated", DEPRECATED_KW),
    ("dynamic", DYNAMIC_KW),
    ("experimental", EXPERIMENTAL_KW),
    ("export", EXPORT_KW),
    ("external", EXTERNAL_KW),
    ("far", FAR_KW),
    ("final", FINAL_KW),
    ("forward", FORWARD_KW),
    ("helper", HELPER_KW),
    ("implements", IMPLEMENTS_KW),
    ("index", INDEX_KW),
    ("local", LOCAL_KW),
    ("message", MESSAGE_KW),
    ("name", NAME_KW),
    ("near", NEAR_KW),
    ("nodefault", NODEFAULT_KW),
    ("on", ON_KW),
    ("operator", OPERATOR_KW),
    ("out", OUT_KW),
    ("overload", OVERLOAD_KW),
    ("override", OVERRIDE_KW),
    ("pascal", PASCAL_KW),
    ("platform", PLATFORM_KW),
    ("private", PRIVATE_KW),
    ("protected", PROTECTED_KW),
    ("public", PUBLIC_KW),
    ("published", PUBLISHED_KW),
    ("read", READ_KW),
    ("readonly", READONLY_KW),
    ("reference", REFERENCE_KW),
    ("register", REGISTER_KW),
    ("reintroduce", REINTRODUCE_KW),
    ("safecall", SAFECALL_KW),
    ("sealed", SEALED_KW),
    ("static", STATIC_KW),
    ("stdcall", STDCALL_KW),
    ("stored", STORED_KW),
    ("strict", STRICT_KW),
    ("unsafe", UNSAFE_KW),
    ("varargs", VARARGS_KW),
    ("virtual", VIRTUAL_KW),
    ("winapi", WINAPI_KW),
    ("write", WRITE_KW),
    ("writeonly", WRITEONLY_KW),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_variants() {
        assert_eq!(KIND_NAMES.len(), __LAST as usize);
        assert_eq!(WHITESPACE.name(), "WHITESPACE");
        assert_eq!(XOR_KW.name(), "XOR_KW");
        assert_eq!(ERROR_NODE.name(), "ERROR");
        assert_eq!(INHERITED_EXPR.name(), "INHERITED_EXPR");
        assert_eq!(format!("{:?}", INHERITED_EXPR), "INHERITED_EXPR");
    }

    #[test]
    fn reserved_words_count() {
        assert_eq!(RESERVED_WORDS.len(), 64);
        assert_eq!(SyntaxKind::reserved_word("begin"), Some(BEGIN_KW));
        assert_eq!(SyntaxKind::reserved_word("forward"), None);
        assert_eq!(
            SyntaxKind::contextual_directive("forward"),
            Some(FORWARD_KW)
        );
        for (i, (_, k)) in RESERVED_WORDS.iter().enumerate() {
            assert_eq!(*k as usize, AND_KW as usize + i);
        }
    }

    #[test]
    fn from_raw_roundtrip() {
        for raw in 0..(__LAST as u16) {
            assert_eq!(SyntaxKind::from_raw(raw) as u16, raw);
        }
    }
}
