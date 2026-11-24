use super::*;
use crate::directives::{self, BuildConfig, ProcessContext};
use crate::source::{MemorySourceProvider, SourceFile, SourceMap};

struct Parsed {
    cst: String,
    diags: Vec<Diagnostic>,
    root: SyntaxNode,
}

fn parse_src(src: &[u8]) -> Parsed {
    let mut sources = SourceMap::new();
    let file = sources.add(SourceFile::new("t.pas", src.to_vec()));
    let mut sink = DiagnosticSink::new();
    let lexed = lexer::lex(file, sources.get(file), &mut sink, false);
    let invalid_utf8 = lexed.is_invalid_utf8();
    let config = BuildConfig::default();
    let mut provider = MemorySourceProvider::new();
    let dir = {
        let mut cx = ProcessContext {
            config: &config,
            provider: &mut provider,
            sources: &mut sources,
            sink: &mut sink,
            include_positions: Vec::new(),
        };
        directives::process(file, lexed.tokens, &mut cx)
    };
    let input = ParseInput {
        file,
        bytes: src,
        tokens: &dir.tokens,
        directives: &dir,
        invalid_utf8,
    };
    let out = parse(&input, &mut sink);
    Parsed {
        cst: dump_cst(&out.root),
        diags: sink.into_sorted(),
        root: out.root,
    }
}

fn codes(p: &Parsed) -> Vec<String> {
    p.diags
        .iter()
        .map(|d| d.code.unwrap().to_string())
        .collect()
}

fn msgs(p: &Parsed) -> Vec<String> {
    p.diags.iter().map(|d| d.message.clone()).collect()
}

fn assert_clean(src: &str) -> Parsed {
    let p = parse_src(src.as_bytes());
    assert!(p.diags.is_empty(), "{src}\n{:?}\n{}", p.diags, p.cst);
    assert_eq!(p.root.text().to_string(), src, "lossless");
    p
}

#[test]
fn spec_example_cst() {
    let src = "program Hello;\nbegin\n  WriteLn('');\nend.";
    let p = assert_clean(src);
    let expected = "\
SOURCE_FILE@0..40
  PROGRAM@0..40
    PROGRAM_HEADER@0..14
      PROGRAM_KW@0..7 \"program\"
      WHITESPACE@7..8 \" \"
      IDENT@8..13 \"Hello\"
      SEMICOLON@13..14 \";\"
    NEWLINE@14..15 \"\\n\"
    COMPOUND_STMT@15..39
      BEGIN_KW@15..20 \"begin\"
      NEWLINE@20..21 \"\\n\"
      WHITESPACE@21..23 \"  \"
      EXPR_STMT@23..34
        CALL_EXPR@23..34
          NAME_REF@23..30
            IDENT@23..30 \"WriteLn\"
          ARG_LIST@30..34
            L_PAREN@30..31 \"(\"
            ARG@31..33
              LITERAL@31..33
                STRING_LIT@31..33 \"''\"
            R_PAREN@33..34 \")\"
      SEMICOLON@34..35 \";\"
      NEWLINE@35..36 \"\\n\"
      END_KW@36..39 \"end\"
    DOT@39..40 \".\"
    EOF@40..40 \"\"
";
    assert_eq!(p.cst, expected);
}

#[test]
fn boundary_trees() {
    let p = assert_clean("");
    assert_eq!(
        p.cst,
        "SOURCE_FILE@0..0\n  SCRIPT@0..0\n    EOF@0..0 \"\"\n"
    );
    let p = parse_src(b"\xEF\xBB\xBF");
    assert_eq!(
        p.cst,
        "SOURCE_FILE@0..3\n  BOM@0..3 \"\\u{feff}\"\n  SCRIPT@3..3\n    EOF@3..3 \"\"\n"
    );
    let p = assert_clean("#!/usr/bin/env puy\n");
    assert_eq!(
        p.cst,
        "SOURCE_FILE@0..19\n  SHEBANG@0..18 \"#!/usr/bin/env puy\"\n  NEWLINE@18..19 \"\\n\"\n  SCRIPT@19..19\n    EOF@19..19 \"\"\n"
    );
    let p = parse_src(b"{ a\xFF");
    assert_eq!(
        p.cst,
        "SOURCE_FILE@0..4\n  ERROR@0..4\n    ERROR@0..4 \"{ a\\xff\"\n  EOF@4..4 \"\"\n"
    );
    assert_eq!(codes(&p), ["E0001"]);
    let p = assert_clean("X := 1");
    assert!(p.cst.contains("ASSIGN_STMT@0..6"));
    let p = assert_clean("X := 1\r");
    assert!(p.cst.contains("NEWLINE@6..7"));
}

#[test]
fn script_items() {
    let p = assert_clean(
        "uses System.SysUtils as SU;\nconst N = 1;\nvar X := N;\nvar Y: Integer;\nvar A, B: Integer;\nprocedure P; begin end;\nP;\nWriteLn(X)",
    );
    let script = p.root.children().next().unwrap();
    let kinds: Vec<_> = script.children().map(|n| n.kind()).collect();
    assert_eq!(
        kinds,
        [
            USES_CLAUSE,
            CONST_SECTION,
            INLINE_VAR_DECL,
            VAR_SECTION,
            VAR_SECTION,
            ROUTINE_DECL,
            EXPR_STMT,
            EXPR_STMT
        ]
    );
    let p = parse_src(b"var X;");
    assert_eq!(codes(&p), ["E0721"]);
    let p = parse_src(b"var X: Integer = 1; X := 2;");
    assert!(p.diags.is_empty(), "{:?}", p.diags);
    let script = p.root.children().next().unwrap();
    let kinds: Vec<_> = script.children().map(|n| n.kind()).collect();
    assert_eq!(kinds, [VAR_SECTION, ASSIGN_STMT]);
}

#[test]
fn script_dot_at_end() {
    let p = parse_src(b"WriteLn(1);\nend.\n");
    // `end` cannot start a statement, so E0402; `.` gives E0407
    assert_eq!(codes(&p), ["E0402", "E0407"]);
    assert_eq!(p.diags[1].fixes.len(), 1);
    let p = parse_src(b"WriteLn(1); . WriteLn(2)");
    assert_eq!(codes(&p), ["E0407"]);
    assert!(p.diags[0].fixes.is_empty());
}

#[test]
fn program_end_errors() {
    let p = parse_src(b"program P;\nbegin\nend;\n");
    assert_eq!(codes(&p), ["E0406"]);
    assert_eq!(msgs(&p)[0], "expected `.` after the final `end`, found `;`");
    assert_eq!(p.diags[0].fixes[0].edits[0].replacement, ".");
    let p = parse_src(b"program P;\nbegin\nend");
    assert_eq!(codes(&p), ["E0406"]);
    assert_eq!(msgs(&p)[0], "missing `.` after the final `end`");
    let p = parse_src(b"program P;\nprocedure Q;\nbegin\nend.\nbegin\nend.");
    assert_eq!(codes(&p), ["E0407"]);
    assert!(p.cst.contains("ERROR@"));
    let p = parse_src(b"program P;\nbegin\n  X := 1\n");
    // E0406 is not reported because it is at the same position
    assert_eq!(codes(&p), ["E0400"]);
    assert_eq!(msgs(&p)[0], "expected `end`, found end of file");
    assert_eq!(p.diags[0].secondary[0].label, "block starts here");
    assert_eq!(p.diags[0].fixes[0].edits[0].replacement, " end");
    let p = parse_src(b"program P;\nbegin\n  begin\n  X := 1.");
    assert_eq!(codes(&p), ["E0400"]);
    assert_eq!(p.diags[0].fixes[0].edits[0].replacement, " end end");
    let p = parse_src(b"program P;\nbegin\n  repeat\n  X := 1.");
    assert_eq!(msgs(&p)[0], "expected `until`, found `.`");
    assert!(p.diags[0].fixes.is_empty());
    assert_eq!(
        p.diags[0].notes[0],
        "write `until` followed by the loop condition"
    );
}

#[test]
fn missing_tokens_are_supplied() {
    let p = parse_src(b"X := 1\nWriteLn(X);");
    assert_eq!(codes(&p), ["E0400"]);
    let d = &p.diags[0];
    assert_eq!(d.message, "expected `;`, found `WriteLn`");
    assert_eq!(
        d.primary.as_ref().unwrap().span.range,
        TextRange::empty(6.into())
    );
    assert_eq!(d.secondary[0].label, "found here");
    assert_eq!(d.fixes[0].edits[0].replacement, ";");
    let p = parse_src(b"if X > 0 WriteLn('a');");
    assert_eq!(msgs(&p), ["expected `then`, found `WriteLn`"]);
    assert_eq!(p.diags[0].fixes[0].edits[0].replacement, " then");
    let p = parse_src(b"if (X > 0)WriteLn('a');");
    assert_eq!(p.diags[0].fixes[0].edits[0].replacement, " then ");
    let p = parse_src(b"if X > 0then WriteLn('a');");
    // `0then` is E0004, so `then` is not found: it is silently supplied after the fill-in
    assert_eq!(codes(&p), ["E0004"]);
    let p = parse_src(b"program P;\nvar X: Integer\n  Y: Integer;\nbegin end.");
    assert_eq!(msgs(&p), ["expected `;`, found `Y`"]);
}

#[test]
fn skip_to_sync() {
    let p = parse_src(b"if X := 1 then WriteLn(X);");
    assert_eq!(msgs(&p), ["expected `then`, found `:=`"]);
    assert!(p.diags[0].fixes.is_empty());
    assert!(p.cst.contains("ERROR@5..9"), "{}", p.cst);
    assert!(p.cst.contains("THEN_KW"));
    let p = parse_src(b"WriteLn 'a';\nX := 1;");
    assert_eq!(msgs(&p), ["expected `;`, found `'a'`"]);
    assert!(p.cst.contains("ASSIGN_STMT"));
}

#[test]
fn lexer_error_hole() {
    let p = parse_src(b"S := 'abc;\nWriteLn(S);");
    assert_eq!(codes(&p), ["E0007"]);
    assert!(p.cst.contains("ASSIGN_STMT@0..10"), "{}", p.cst);
    assert!(
        p.cst.contains("ERROR@5..10\n        ERROR@5..10"),
        "{}",
        p.cst
    );
    let p = parse_src(b"S := 'abc\n+ T;");
    assert_eq!(codes(&p), ["E0007"]);
    let p = parse_src(b"X := \"abc\";");
    assert_eq!(codes(&p), ["E0003", "E0003"]);
}

#[test]
fn expressions() {
    let p = assert_clean("X := -A * B + C shl 1 = not D and (E or F) <> S[I].X(1, 2:3:4);");
    assert!(p.cst.contains("UNARY_EXPR"));
    // Unary - is at level 1: (-A) * B
    let p = assert_clean("X := -A * B;");
    let assign = p
        .root
        .descendants()
        .find(|n| n.kind() == ASSIGN_STMT)
        .unwrap();
    let rhs = assign.children().nth(1).unwrap();
    assert_eq!(rhs.kind(), BINARY_EXPR);
    assert_eq!(rhs.children().next().unwrap().kind(), UNARY_EXPR);
    // Left-associative: ((a + b) + c)
    let p = assert_clean("X := a + b + c;");
    let assign = p
        .root
        .descendants()
        .find(|n| n.kind() == ASSIGN_STMT)
        .unwrap();
    let rhs = assign.children().nth(1).unwrap();
    assert_eq!(rhs.children().next().unwrap().kind(), BINARY_EXPR);
    assert_eq!(rhs.children().nth(1).unwrap().kind(), NAME_REF);
    let p = parse_src(b"X := @Y^;");
    assert_eq!(codes(&p), ["E3004", "E3004"]);
    // Array constructors (types.md chapter 4), with the ranges of set constructors (M2b)
    let p = assert_clean("X := [1, 2];");
    assert!(p.cst.contains("ARRAY_CTOR@5..11"), "{}", p.cst);
    let p = assert_clean("X := ['a'..'z', B];");
    assert!(p.cst.contains("INDEX_RANGE@6..14"), "{}", p.cst);
    assert_clean("Format('%d', [X]);");
    assert_clean("X := [];");
    let p = parse_src(b"X := ;");
    assert_eq!(msgs(&p), ["expected an expression"]);
    assert_eq!(
        p.diags[0].primary.as_ref().unwrap().span.range,
        TextRange::empty(4.into())
    );
    let p = parse_src(b"WriteLn(,);");
    assert_eq!(codes(&p), ["E0403", "E0403"]);
    let p = parse_src(b"X := string(C);");
    assert!(p.diags.is_empty());
    let p = parse_src(b"1 + 2;");
    assert_eq!(codes(&p), ["E0402"]);
    assert_eq!(
        p.diags[0].primary.as_ref().unwrap().span.range,
        TextRange::new(0.into(), 5.into())
    );
    let p = parse_src(b"S[1];");
    assert_eq!(codes(&p), ["E0402"]);
    let p = assert_clean("X;\nSU.X;\nF(1);");
    assert_eq!(p.root.children().next().unwrap().children().count(), 3);
}

#[test]
fn statements() {
    assert_clean("if A then B else if C then D else E;");
    assert_clean("while X do begin Y; end;");
    assert_clean("repeat X; Y until Z;");
    assert_clean("for I := 1 to 10 do X;");
    assert_clean("for var I: Integer := 10 downto 1 do begin end;");
    let p = parse_src(b"for var I := 1 to 3 do ;");
    assert_eq!(codes(&p), ["W0018"]);
    let p = parse_src(b"if X then;");
    assert_eq!(codes(&p), ["W0018"]);
    assert_eq!(msgs(&p)[0], "empty statement after `then`");
    let p = parse_src(b"while X do;");
    assert_eq!(codes(&p), ["W0018"]);
    let p = parse_src(b"begin;; end;");
    assert!(p.diags.is_empty());
    let p = parse_src(b"begin if X then Y; else Z; end;");
    assert_eq!(codes(&p), ["E0410"]);
    assert!(p.cst.contains("IF_STMT@6..25"), "{}", p.cst);
    assert_clean("for I in L do X;");
    assert_clean("for var S: string in Names do WriteLn(S);");
    let p = parse_src(b"for var C in do X;");
    assert_eq!(codes(&p), ["E0403"]);
    assert!(p.cst.contains("DO_KW"), "{}", p.cst);
    let p = parse_src(b"begin ) end;");
    assert_eq!(codes(&p), ["E0402"]);
    assert_clean("try X; except on E: Exception do Y; end;\nZ;");
    let p = parse_src(b"with X do begin A := 1; end;\nZ;");
    assert_eq!(codes(&p), ["E3000"]);
    let p = assert_clean("case X of 1: Y; else Z; end;\nW;");
    assert_eq!(
        p.root
            .children()
            .next()
            .unwrap()
            .children()
            .filter(|n| n.kind() == EXPR_STMT)
            .count(),
        1
    );
}

#[test]
fn declarations() {
    assert_clean(
        "program P;\nconst\n  A = 1;\n  B: Int64 = 2;\nvar\n  X, Y: Integer;\n  S: string = 'a';\nbegin\nend.",
    );
    let p = parse_src(b"program P;\nvar A, B: Integer = 0;\nbegin end.");
    assert_eq!(codes(&p), ["E0409"]);
    let p = parse_src(b"program P;\nvar I: UInt32 absolute F;\nbegin end.");
    assert_eq!(codes(&p), ["E3005"]);
    assert!(p.cst.contains("ABSOLUTE_KW"));
    // A set type is read here; its base type is checked by semantic analysis (E0737)
    let p = parse_src(b"program P;\nvar A: set of Integer;\nbegin end.");
    assert!(codes(&p).is_empty());
    assert!(p.cst.contains("SET_TYPE"));
    let p = parse_src(b"program P;\nvar S: string[20];\nbegin end.");
    assert_eq!(codes(&p), ["E3003"]);
    let p = parse_src(b"program P;\nvar X: ;\nbegin end.");
    assert_eq!(codes(&p), ["E0404"]);
    let p = parse_src(b"program P;\nvar : Integer;\nbegin end.");
    assert_eq!(codes(&p), ["E0405"]);
    let p = parse_src(b"program P;\ntype\n  TFoo = class(TObject)\n  private\n    FX: Integer;\n  public\n    procedure Bar;\n  end;\n  TRec = record X: Integer; end;\nvar X: Integer;\nbegin end.");
    // A class body is read as a whole (`docs/spec/objects.md` chapter 1)
    assert!(codes(&p).is_empty());
    assert!(p.cst.contains("CLASS_BODY"));
    assert!(p.cst.contains("METHOD_DECL"));
    assert!(p.cst.contains("VAR_SECTION"));
    let p = parse_src(b"program P;\nlabel L;\nbegin end.");
    assert_eq!(codes(&p), ["E3001"]);
}

#[test]
fn routines() {
    assert_clean(
        "procedure P(const A: Integer; var B: string; out C: Int64); forward;\nprocedure P(const A: Integer; var B: string; out C: Int64);\nvar X: Integer = 1;\nbegin\nend;\nfunction F(): Integer; inline;\nbegin\n  Result := 1;\nend;",
    );
    let p = parse_src(b"procedure P(var Buf);\nbegin end;");
    assert_eq!(codes(&p), ["E3005"]);
    let p = parse_src(b"procedure P(Buf);\nbegin end;");
    assert_eq!(codes(&p), ["E0400"]);
    // A default value is a DEFAULT_VALUE node (semantics.md 4.8)
    let p = parse_src(b"procedure P(X: Integer = 1);\nbegin end;");
    assert!(codes(&p).is_empty());
    assert!(p.cst.contains("DEFAULT_VALUE"));
    // Missing begin in a program
    let p = parse_src(b"program P;\nprocedure Q;\nWriteLn('x');\nend;\nbegin\nend.");
    assert_eq!(msgs(&p), ["expected `begin`, found `WriteLn`"]);
    assert!(p.cst.contains("ROUTINE_BODY"));
    // No body in a script
    let p = parse_src(b"procedure Q;\nWriteLn('x');\nend;");
    assert_eq!(codes(&p), ["E0411", "E0402"]);
    assert_eq!(
        p.diags[0].notes,
        [
            "write the body, or add `forward;` if the body comes later",
            "a body starts with `begin`"
        ]
    );
    // The forward fix is offered only when the same name follows (a `procedure` right after the heading would be nested,
    // so put a statement in between to make a declaration without a body)
    let p = parse_src(b"procedure Q;\nX;\nprocedure Q;\nbegin end;");
    assert_eq!(codes(&p), ["E0411"]);
    assert_eq!(p.diags[0].fixes[0].edits[0].replacement, " forward;");
    assert_eq!(
        p.diags[0].fixes[0].edits[0].range,
        TextRange::empty(12.into())
    );
    let p = parse_src(b"procedure Q;\nX;\nprocedure R;\nbegin end;");
    assert_eq!(codes(&p), ["E0411"]);
    assert!(p.diags[0].fixes.is_empty());
    // Nested routines are read as declarations of the body
    let p = parse_src(b"procedure Outer;\nvar X: Integer;\n  procedure Inner;\n  begin\n  end;\nbegin\n  Inner;\nend;");
    assert!(p.diags.is_empty(), "{:?}", p.diags);
    assert!(
        p.cst
            .contains("      ROUTINE_BODY@17..85\n        VAR_SECTION@17..32"),
        "{}",
        p.cst
    );
    assert!(p.cst.contains("        ROUTINE_DECL@35..66"), "{}", p.cst);
    // Directives
    let p = parse_src(b"procedure P; stdcall; deprecated 'x'; external 'a' name 'b'; forward;\nX;");
    assert!(p.diags.is_empty(), "{:?}", p.diags);
    assert!(p.cst.contains("STDCALL_KW"));
    assert!(p.cst.contains("DEPRECATED_KW"));
    // A method implementation heading is parsed normally
    assert_clean("procedure TForm1.Click;\nbegin\nend;");
    let p = parse_src(b"procedure P; 123");
    assert_eq!(codes(&p), ["E0411", "E0401"]);
}

#[test]
fn uses_clauses() {
    assert_clean("uses System.SysUtils, Foo as F;");
    let p = parse_src(b"uses SysUtils in 'SysUtils.pas';");
    assert_eq!(codes(&p), ["W0006"]);
    assert_eq!(
        p.diags[0].fixes[0].edits[0].range,
        TextRange::new(13.into(), 31.into())
    );
    let p = parse_src(b"program P;\nvar X: Integer;\nuses System.SysUtils;\nbegin end.");
    assert_eq!(codes(&p), ["E0414"]);
    let f = &p.diags[0].fixes[0];
    assert_eq!(f.edits.len(), 2);
    // Placed on a new line after the `program` heading
    assert_eq!(f.edits[0].replacement, "\nuses System.SysUtils;");
    assert_eq!(f.edits[0].range, TextRange::empty(10.into()));
    let p = parse_src(b"uses A;\nX;\nuses B, A;");
    assert_eq!(codes(&p), ["E0414"]);
    assert_eq!(p.diags[0].secondary[0].label, "first `uses` here");
    let f = &p.diags[0].fixes[0];
    assert_eq!(f.edits[0].replacement, ", B");
    assert_eq!(f.edits[0].range, TextRange::empty(6.into()));
    let p = parse_src(b"uses A;\n{$IFDEF PUY}\nuses B;\n{$ENDIF}");
    assert_eq!(codes(&p), ["E0414"]);
    assert!(p.diags[0].fixes.is_empty());
}

#[test]
fn directive_placement() {
    let p = parse_src(b"X := 1 {$IFDEF PUY} + 2 {$ELSE} + 3 {$ENDIF};");
    assert_eq!(codes(&p), ["W0005"]);
    assert_eq!(
        p.diags[0].primary.as_ref().unwrap().span.range,
        TextRange::new(7.into(), 19.into())
    );
    let p = parse_src(b"X := 1 {$R-} + 2;");
    assert_eq!(codes(&p), ["W0005"]);
    assert_eq!(
        p.diags[0].notes[0],
        "Puy applies a switch from the next declaration or statement, not from this point"
    );
    assert_clean(
        "{$IFDEF PUY}\nX := 1;\n{$ENDIF}\n{$R-}\nvar {$IFDEF X} A: Integer; {$ENDIF} B: Integer;\nprocedure P({$IFDEF X}A: Integer{$ENDIF}); {$IFDEF X} inline; {$ENDIF}\nbegin {$R+} end;",
    );
    let p = parse_src(b"{$IFDEF X}\nX := 1\n");
    // The inactive region runs to the end of the file, so later syntax errors are suppressed
    assert_eq!(codes(&p), ["E0203"]);
}

#[test]
fn nesting_limit() {
    let ok = format!("X := {}1{};", "(".repeat(256), ")".repeat(256));
    assert_clean(&ok);
    let bad = format!("X := {}1{};", "(".repeat(257), ")".repeat(257));
    let p = parse_src(bad.as_bytes());
    assert_eq!(codes(&p), ["E0413"]);
    // The left-hand chain is not counted
    let long = format!("X := 1{};", " + 1".repeat(300));
    assert_clean(&long);
    let elifs = format!("if A then B{} else Z;", " else if A then B".repeat(300));
    assert_clean(&elifs);
    let unary = format!("X := {}1;", "-".repeat(257));
    assert_eq!(codes(&parse_src(unary.as_bytes())), ["E0413"]);
}

// A chain of headings without bodies becomes nested routines, so nesting depth also gives E0413 at 256.
// Without a limit, a long generated sequence of declarations overflows the Rust stack
#[test]
fn nested_routine_limit() {
    let chain = |n: usize| format!("{}{}", "procedure P;\n".repeat(n), "begin end;\n".repeat(n));
    assert_eq!(
        codes(&parse_src(chain(256).as_bytes())),
        Vec::<String>::new()
    );
    // Past the limit, the rest is skipped so that diagnostics such as the `begin` at the end of the file do not cascade
    assert_eq!(codes(&parse_src(chain(257).as_bytes())), ["E0413"]);
    assert_eq!(codes(&parse_src(chain(20000).as_bytes())), ["E0413"]);
}

// A file starting with `unit` is read as a unit (`docs/spec/units.md` chapter 1; M1 made it E3200).
#[test]
fn unit_file() {
    let p = parse_src(b"unit Foo;\ninterface\nimplementation\nend.");
    assert_eq!(codes(&p), Vec::<String>::new());
    assert!(
        p.cst
            .starts_with("SOURCE_FILE@0..39\n  UNIT@0..39\n    UNIT_HEADER@0..9\n"),
        "{}",
        p.cst
    );
}

#[test]
fn trivia_placement_at_lca() {
    let p = assert_clean("program P;\n// c\nbegin\nend. // tail\n");
    // A leading comment is a child of PROGRAM (between PROGRAM_HEADER and COMPOUND_STMT)
    assert!(p.cst.contains("    NEWLINE@10..11 \"\\n\"\n    LINE_COMMENT@11..15 \"// c\"\n    NEWLINE@15..16 \"\\n\"\n    COMPOUND_STMT"), "{}", p.cst);
    // A trailing comment is inside PROGRAM, before EOF
    assert!(p.cst.contains("    DOT@25..26 \".\"\n    WHITESPACE@26..27 \" \"\n    LINE_COMMENT@27..34 \"// tail\"\n    NEWLINE@34..35 \"\\n\"\n    EOF@35..35 \"\"\n"), "{}", p.cst);
    let p = assert_clean("// head\nX := 1;");
    assert!(p.cst.starts_with("SOURCE_FILE@0..15\n  LINE_COMMENT@0..7 \"// head\"\n  NEWLINE@7..8 \"\\n\"\n  SCRIPT@8..15\n"), "{}", p.cst);
}

#[test]
fn lossless_under_errors() {
    for src in [
        "program P; var X: begin end.",
        "if then else;",
        "begin begin begin",
        "procedure (X: Integer); begin end;",
        "X := (1 + ;",
        "var X := [1,2;",
        "for := 1 to do X;",
        "type T = record end",
        "X := 1 2 3;",
        "uses ;",
        "program ;",
        "end end end.",
    ] {
        let p = parse_src(src.as_bytes());
        assert_eq!(p.root.text().to_string(), src, "lossless: {src}");
        assert!(!p.diags.is_empty(), "{src}");
    }
}

#[test]
// `TArray<T>` in type positions (a variable, a parameter, a return type, nested) and open array
// parameters read as types, `array of const` parameters among them (types.md chapter 1, 2.4,
// rtti.md 4.1).
fn generic_and_open_array_types() {
    let p = assert_clean(
        "program P;\nfunction F(A: array of Integer; const B: TArray<TArray<string>>): TArray<Integer>; begin end;\nvar X: TArray<Integer>;\nbegin end.",
    );
    assert_eq!(p.cst.matches("GENERIC_TYPE_REF@").count(), 4, "{}", p.cst);
    assert!(p.cst.contains("ARRAY_TYPE@"), "{}", p.cst);
    let p =
        assert_clean("program P;\nprocedure Q(const A: array of const); begin end;\nbegin end.");
    assert!(p.cst.contains("ARRAY_TYPE@"), "{}", p.cst);
    assert!(p.cst.contains("CONST_KW@"), "{}", p.cst);
}

#[test]
fn slice_index_items() {
    // A `..` inside an index makes a SLICE node spanning the bounds that are written
    // (types.md 8.7); the other items of the index stay plain expressions.
    let p = assert_clean("X := S[1..2];");
    assert!(p.cst.contains("SLICE@7..11"), "{}", p.cst);
    let p = assert_clean("X := S[..];");
    assert!(p.cst.contains("SLICE@7..9"), "{}", p.cst);
    let p = assert_clean("X := A[I, ..H];");
    assert!(p.cst.contains("SLICE@10..13"), "{}", p.cst);
    let p = assert_clean("X := A[L.., J];");
    assert!(p.cst.contains("SLICE@7..10"), "{}", p.cst);
}

#[test]
// A `>=` closing type arguments or type parameters is split into a `>` and an `=` holding one
// character each, so the CST still gives back the source text (generics.md 1.3, 1.5).
fn split_ge_keeps_the_text() {
    let p = assert_clean("const C: TArray<Integer>= nil;");
    assert!(p.cst.contains("GT@23..24 \">\""), "{}", p.cst);
    assert!(p.cst.contains("EQ@24..25 \"=\""), "{}", p.cst);
    let p = assert_clean("type TBox<T>= record end;");
    assert!(p.cst.contains("GT@11..12 \">\""), "{}", p.cst);
    assert!(p.cst.contains("EQ@12..13 \"=\""), "{}", p.cst);
    let p = assert_clean("var X: TList<TList<Integer>>= nil;");
    assert!(p.cst.contains("GT@26..27 \">\""), "{}", p.cst);
    assert!(p.cst.contains("GT@27..28 \">\""), "{}", p.cst);
    assert!(p.cst.contains("EQ@28..29 \"=\""), "{}", p.cst);
}

#[test]
// Generic declarations, headings and type arguments in type positions read without errors and
// keep the source text (generics.md 1.1 to 1.3).
fn generic_declarations_are_lossless() {
    for src in [
        "type TPair<K, V> = record Key: K; Value: V; end;",
        "type TOwned<K; V: class, constructor; W: IFoo<K>> = class end;",
        "type TStack<T> = class procedure Push(const Item: T); function Map<U>: TArray<U>; end;",
        "procedure TStack<T>.Push(const Item: T); begin end;",
        "function TList<T>.Map<U>(const F: TFunc<T, U>): TArray<U>; begin end;",
        "procedure TOuter<T>.TInner.Run; begin end;",
        "function Max<T>(const A, B: T): T; begin end;",
        "type TIntList = class(TList<Integer>) end;",
        "var D: TDictionary<string, TList<Integer>>;",
    ] {
        assert_clean(src);
    }
}

#[test]
// Comments and line breaks inside type arguments in an expression are trivia: the `<` still
// starts TYPE_ARGS and the CST keeps the text (generics.md 1.4).
fn expr_type_args_with_trivia() {
    let src = "X := Max<{a} Integer,\n  TList<string> // b\n>(1);";
    let p = assert_clean(src);
    assert_eq!(p.root.text().to_string(), src);
    assert_eq!(p.cst.matches("TYPE_ARGS@").count(), 1, "{}", p.cst);
    assert_eq!(p.cst.matches("GENERIC_TYPE_REF@").count(), 1, "{}", p.cst);
}

#[test]
// A token that is not part of the form of `TypeArgs` before the `>`, or a token after the `>`
// that can start an expression, keeps `<` a comparison (generics.md 1.4).
fn expr_lt_stays_comparison() {
    for (idx, src) in [
        "X := A < B;",
        "X := A < B > C;",
        "X := A < B > -C;",
        "X := A < B > [C];",
        "X := A < 1 > (C);",
        "X := A < B.C(1) > (C);",
        "F(A < B, C > D);",
        "X := A < B >= (C);",
    ]
    .iter()
    .enumerate()
    {
        let p = assert_clean(src);
        assert!(!p.cst.contains("TYPE_ARGS@"), "{idx}: {}", p.cst);
    }
}

#[test]
// A lexical error inside what would be type arguments keeps `<` a comparison: the error is
// reported once by the lexer and the type arguments are not read around it.
fn expr_type_args_not_read_over_lex_error() {
    let src = "X := A < B ? > (C);";
    let p = parse_src(src.as_bytes());
    assert_eq!(p.root.text().to_string(), src);
    assert!(!p.cst.contains("TYPE_ARGS@"), "{}", p.cst);
}

#[test]
fn trimmed_range_skips_leading_trivia_of_the_node() {
    // A node whose first tokens are trivia (the root, where a leading comment is attached)
    // starts at its first non-trivia token
    let p = assert_clean("// head\nX := 1;\n");
    assert_eq!(
        crate::syntax::ast::trimmed_range(&p.root).start(),
        TextSize::from(8),
        "{}",
        p.cst
    );
}

#[test]
fn trimmed_range_skips_trailing_trivia_inside_nested_nodes() {
    // An anonymous method cut off before its body ends with a space and an empty
    // ROUTINE_BODY, so the trimmed range steps back over both to `)`, also for the statement
    // that holds it
    let p = parse_src(b"R := procedure(X: Integer) inline;\n");
    let anon = p
        .root
        .descendants()
        .find(|n| n.kind() == SyntaxKind::ANON_ROUTINE)
        .expect("an anonymous method");
    assert_eq!(
        anon.text_range(),
        TextRange::new(5.into(), 27.into()),
        "{}",
        p.cst
    );
    assert_eq!(
        crate::syntax::ast::trimmed_range(&anon),
        TextRange::new(5.into(), 26.into())
    );
    let assign = anon.parent().expect("the assignment");
    assert_eq!(
        crate::syntax::ast::trimmed_range(&assign),
        TextRange::new(0.into(), 26.into())
    );
}

#[test]
fn trimmed_range_of_a_node_without_trivia_at_the_ends_is_its_range() {
    // Trivia between the tokens (a comment inside parentheses) does not narrow the range
    let p = assert_clean("X := ( { a } 1 + 2 // c\n);\n");
    let paren = p
        .root
        .descendants()
        .find(|n| n.kind() == SyntaxKind::PAREN_EXPR)
        .expect("a parenthesized expression");
    assert_eq!(
        crate::syntax::ast::trimmed_range(&paren),
        TextRange::new(5.into(), 25.into())
    );
}

#[test]
fn nodes_of_kind_returns_nodes_in_preorder() {
    // The nodes come outer before inner and in file order, as a cursor walk returns them
    let p = assert_clean("program P;\nbegin\n  X := (1 + (2 * 3));\n  Y := (4);\nend.\n");
    let found: Vec<TextRange> = crate::syntax::nodes_of_kind(&p.root, SyntaxKind::PAREN_EXPR)
        .iter()
        .map(|n| n.text_range())
        .collect();
    assert_eq!(
        found,
        [
            TextRange::new(24.into(), 37.into()),
            TextRange::new(29.into(), 36.into()),
            TextRange::new(46.into(), 49.into()),
        ],
        "{}",
        p.cst
    );
}

#[test]
fn has_node_of_kind_sees_the_node_and_its_descendants() {
    // The node itself counts, a nested node counts, and a kind outside the subtree does not
    let p = assert_clean("program P;\nbegin\n  X := (1 + 2);\n  Y := (4);\nend.\n");
    let parens = crate::syntax::nodes_of_kind(&p.root, SyntaxKind::PAREN_EXPR);
    assert!(crate::syntax::has_node_of_kind(
        &p.root,
        SyntaxKind::BINARY_EXPR
    ));
    assert!(!crate::syntax::has_node_of_kind(
        &p.root,
        SyntaxKind::ERROR_NODE
    ));
    assert!(crate::syntax::has_node_of_kind(
        &parens[1],
        SyntaxKind::PAREN_EXPR
    ));
    assert!(!crate::syntax::has_node_of_kind(
        &parens[1],
        SyntaxKind::BINARY_EXPR
    ));
}
