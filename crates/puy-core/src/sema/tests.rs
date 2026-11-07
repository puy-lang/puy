use crate::diagnostics::Diagnostic;
use crate::driver::{CompileOptions, compile};
use crate::source::MemorySourceProvider;

fn check(src: &str) -> Vec<Diagnostic> {
    let mut provider = MemorySourceProvider::new();
    let out = compile(
        "t.pas",
        src.as_bytes().to_vec(),
        &mut provider,
        &CompileOptions::default(),
    );
    out.diagnostics
}

/// Codes excluding W0001 (the unused-variable warning is covered by separate tests).
fn codes(src: &str) -> Vec<String> {
    check(src)
        .iter()
        .filter(|d| d.code != Some(crate::diagnostics::Code::W(1)))
        .map(|d| d.code.unwrap().to_string())
        .collect()
}

fn all_codes(src: &str) -> Vec<String> {
    check(src)
        .iter()
        .map(|d| d.code.unwrap().to_string())
        .collect()
}

fn msgs(src: &str) -> Vec<String> {
    check(src)
        .iter()
        .filter(|d| d.code != Some(crate::diagnostics::Code::W(1)))
        .map(|d| d.message.clone())
        .collect()
}

fn find(src: &str, code: &str) -> Diagnostic {
    check(src)
        .into_iter()
        .find(|d| d.code.unwrap().to_string() == code)
        .unwrap_or_else(|| panic!("{code} not found in {src}"))
}

fn assert_clean(src: &str) {
    let d: Vec<_> = check(src)
        .into_iter()
        .filter(|d| d.code != Some(crate::diagnostics::Code::W(1)))
        .collect();
    assert!(d.is_empty(), "{src}\n{d:#?}");
}

#[test]
fn hello_and_basics() {
    assert_clean("program Hello;\nbegin\n  WriteLn('Hello, Puy!');\nend.");
    assert_clean("WriteLn('x')");
    assert_clean("var X := 1;\nvar S := 'ab';\nvar C := 'a';\nWriteLn(X, S, C, X:5, True);");
    assert_clean(
        "program P;\nvar\n  A: Integer = 1;\n  B: Byte;\nbegin\n  B := 2;\n  WriteLn(A + B, A div B, A mod B, A shl 1, -A, not A, A and B);\nend.",
    );
    assert_clean(
        "program P;\nfunction Square(X: Integer): Integer;\nbegin\n  Result := X * X;\nend;\nbegin\n  WriteLn(Square(3));\nend.",
    );
}

#[test]
fn undeclared_and_types() {
    assert_eq!(msgs("WriteLn(Foo)"), ["undeclared identifier `Foo`"]);
    assert_eq!(codes("Foo(1)"), ["E0700"]);
    assert_eq!(codes("var X: Integer := 'ab';"), ["E0702"]);
    assert_eq!(
        msgs("var X: Integer := 'ab';"),
        ["type mismatch: expected `Integer`, found `string`"]
    );
    assert_eq!(codes("var X := 'ab' + 1;"), ["E0703"]);
    assert_eq!(
        msgs("var X := 'ab' + 1;"),
        ["operator `+` cannot be applied to `string` and `Integer`"]
    );
    assert_eq!(codes("var X := True + 1;"), ["E0703"]);
    assert_eq!(codes("var X := not 'a';"), ["E0703"]);
    assert_eq!(codes("var B: Byte := 300;"), ["E0714"]);
    assert_eq!(
        msgs("var B: Byte := 300;"),
        ["constant value 300 is out of range for `Byte` (0..255)"]
    );
    assert_eq!(codes("var X := 1 div 0;"), ["E0715"]);
    assert_eq!(codes("var N := 1; var X := N div 0;"), ["E0715"]);
    assert_eq!(
        codes("var U: UInt64 := 1; var I: Integer := 1; var X := U + I;"),
        ["E0716"]
    );
    assert_clean("var U: UInt64 := 1; var B: Byte := 1; var X := U + B;");
    assert_eq!(codes("var X: string := 1;"), ["E0702"]);
    assert_clean("var S: string := 'a'; var C: Char := 'b'; S := C; S := S + C; C := S[1];");
    assert_eq!(codes("var C: Char := 'ab';"), ["E0702"]);
    assert_eq!(codes("var X := Integer;"), ["E0708"]);
    assert_eq!(codes("var X := System;"), ["E0708"]);
    assert_eq!(codes("var X: Foo;"), ["E0700"]);
    assert_eq!(codes("var X: Foo; X := 1; WriteLn(X + 'a');"), ["E0700"]);
    assert_eq!(codes("var S := 'ab'; WriteLn(S[1, 2]);"), ["E0705"]);
    assert_eq!(codes("var N := 1; WriteLn(N[1]);"), ["E0720"]);
    assert_eq!(codes("var S := 'ab'; WriteLn(S['a']);"), ["E0719"]);
    assert_eq!(codes("var X := 1.5;"), ["E3200"]);
    assert_eq!(codes("var X := 1 / 2;"), ["E3200"]);
    assert_eq!(codes("var X := Low(Integer);"), ["E3200"]);
    assert_eq!(
        msgs("var X := Low(Integer);"),
        ["`Low` is not supported yet (planned for M2)"]
    );
    assert_eq!(codes("var P: Pointer;"), ["E3004"]);
    assert_eq!(codes("var S: AnsiString;"), ["E3003"]);
    assert_eq!(codes("var I := 1; WriteLn(I.ToString);"), ["E3200"]);
}

#[test]
fn integer_rules() {
    assert_clean("var C: Cardinal := 1; var X := C + 1; var Y: Cardinal := X;");
    assert_clean("const N = -2147483648; var X: Integer := N;");
    assert_clean("const N = -9223372036854775808;");
    assert_eq!(codes("const N = 2147483647 + 1;"), ["E0714"]);
    assert_clean("const N: Int64 = 2147483647; const M = N + 1;");
    assert_clean("const S = 'a' + 'b' + #10; var T: string := S;");
    assert_clean("var X := $FF shl 4; var Y := Byte(1); var Z := Cardinal(-1 + 2);");
    assert_eq!(codes("var Z := Cardinal(-1);"), ["E0714"]);
    let d = find("var Z := Cardinal(-1);", "E0714");
    assert_eq!(d.fixes[0].edits[0].replacement, "$FFFFFFFF");
    let d = find("var Z := Byte(300);", "E0714");
    assert_eq!(d.fixes[0].edits[0].replacement, "44");
    // When the hex notation would be the same as the original, show a negative decimal value (2.5)
    let d = find("var Z := Integer($FFFFFFFF);", "E0714");
    assert_eq!(d.fixes[0].edits[0].replacement, "-1");
    let d = find("var Z := Integer($80000001);", "E0714");
    assert_eq!(d.fixes[0].edits[0].replacement, "-2147483647");
    // Hex at the width of the target type (8 digits for Integer)
    let d = find("var Z := Integer($1FFFFFFFF);", "E0714");
    assert_eq!(d.fixes[0].edits[0].replacement, "$FFFFFFFF");
    let d = find("var Z := SmallInt(-40000);", "E0714");
    assert_eq!(d.fixes[0].edits[0].replacement, "$63C0");
    assert_eq!(codes("var Z := Byte(1.5);"), ["E3200"]);
    assert_eq!(codes("var Z := Integer('a');"), Vec::<String>::new());
    assert_eq!(
        codes("var Z := Boolean(1); var W := Char(65); var V := string('a');"),
        Vec::<String>::new()
    );
    assert_eq!(codes("var Z := Integer('ab');"), ["E0724"]);
    assert_clean("var X := Chr(65); var Y := Ord('a'); var Z := Abs(-3); var W := Ord(True);");
    assert_eq!(codes("var X := Chr(70000);"), ["E0714"]);
}

#[test]
fn routines_and_calls() {
    assert_clean(
        "procedure P(A: Integer; var B: Integer; out C: string); begin B := A; C := ''; end;\nvar X := 0; var S := '';\nP(1, X, S);",
    );
    assert_eq!(
        codes("procedure P(A: Integer); begin end;\nP(1, 2);"),
        ["E0705"]
    );
    assert_eq!(
        codes("procedure P(A: Integer); begin end;\nP('a');"),
        ["E0702"]
    );
    assert_eq!(
        msgs("procedure P(A: Integer); begin end;\nP('a');"),
        ["type mismatch: expected `Integer`, found `Char` (argument 1, parameter `A`)"]
    );
    assert_eq!(
        codes("procedure P(var A: Integer); begin A := 1; end;\nP(1);"),
        ["E0706"]
    );
    assert_eq!(
        codes("procedure P(var A: Integer); begin A := 1; end;\nconst C = 1;\nP(C);"),
        ["E0706"]
    );
    assert_eq!(
        codes("procedure P(var A: Int64); begin A := 1; end;\nvar X := 0;\nP(X);"),
        ["E0702"]
    );
    assert_eq!(
        codes("procedure P(const A: Integer); begin A := 1; end;"),
        ["E0707"]
    );
    assert_eq!(
        check("procedure P(const A: Integer); begin A := 1; end;")[0]
            .primary
            .as_ref()
            .unwrap()
            .label,
        "a `const` parameter"
    );
    assert_eq!(codes("const C = 1;\nC := 2;"), ["E0707"]);
    assert_eq!(codes("1 := 2;"), ["E0707"]);
    assert_eq!(codes("procedure P; begin end;\nvar X := P;"), ["E0725"]);
    assert_eq!(
        codes("function F: Integer; begin Result := F; end;"),
        ["E0704"]
    );
    assert_eq!(
        msgs("function F: Integer; begin Result := F; end;"),
        ["recursive call to `F` needs parentheses"]
    );
    assert_clean(
        "function F(N: Integer): Integer; begin if N = 0 then Result := 1 else Result := N * F(N - 1); end;\nWriteLn(F(5));",
    );
    assert_eq!(
        codes("function F(N: Integer): Integer; begin Result := N; end;\nvar X := F;"),
        ["E0704"]
    );
    assert_clean("function F: Integer; begin Result := 1; end;\nvar X := F;\nF;\nF();");
    assert_eq!(codes("procedure P; begin Result := 1; end;"), ["E0710"]);
    assert_eq!(codes("procedure P; begin Exit(1); end;"), ["E0711"]);
    assert_eq!(codes("Break;"), ["E0712"]);
    assert_clean("while True do begin Break; end;\nrepeat Continue; until True;");
    assert_eq!(codes("procedure P; forward;"), ["E0723"]);
    assert_eq!(
        codes("procedure P(A: Integer); forward;\nprocedure P(B: Integer); begin end;"),
        ["E0723"]
    );
    assert_clean("procedure P(A: Integer); forward;\nprocedure P(A: Integer); begin end;\nP(1);");
    assert_eq!(
        codes("procedure P; begin end;\nprocedure P; begin end;"),
        ["E0701"]
    );
    assert_eq!(codes("var X := 1;\nvar X := 2;"), ["E0701"]);
    assert_eq!(codes("var Result := 1;"), ["E0701"]);
    assert_eq!(msgs("var Result := 1;"), ["`Result` is a reserved name"]);
    assert_eq!(
        codes("var X := 1;\nprocedure P; begin WriteLn(X); end;"),
        ["E0700"]
    );
    assert_eq!(
        find("var X := 1;\nprocedure P; begin WriteLn(X); end;", "E0700")
            .notes
            .len(),
        2
    );
    assert_eq!(codes("procedure P; begin end;\nvar P := 1;"), ["E0701"]);
    assert_eq!(
        codes("var N := 1; procedure P; begin end;\nN();"),
        ["E0704"]
    );
    assert_eq!(
        codes("procedure P(X: Integer); begin end;\nP(1:5);"),
        ["E0717"]
    );
    assert_eq!(codes("procedure P; stdcall; begin end;"), ["W0009"]);
    assert_eq!(codes("procedure P; safecall; begin end;"), ["E3010"]);
}

#[test]
fn sysutils_and_uses() {
    assert_eq!(codes("WriteLn(IntToStr(1))"), ["E0700"]);
    let d = check("WriteLn(IntToStr(1))");
    assert_eq!(
        d[0].fixes[0].edits[0].replacement,
        "uses System.SysUtils;\n"
    );
    assert_clean(
        "uses System.SysUtils;\nWriteLn(IntToStr(1), StrToInt('1'), System.SysUtils.Trim(' a '), System.Length('a'));",
    );
    assert_clean("uses System.SysUtils as SU;\nWriteLn(SU.IntToStr(1));");
    let d = check("uses System.SysUtils as SU;\nWriteLn(IntToStr(1));");
    assert_eq!(d[0].fixes[0].edits[0].replacement, "SU.IntToStr");
    assert_eq!(codes("uses SysUtils;\nWriteLn(IntToStr(1));"), ["E2501"]);
    assert_eq!(codes("uses Foo;"), ["E2500"]);
    assert_eq!(codes("uses Vcl.Forms;"), ["E2500"]);
    assert_eq!(
        check("uses Vcl.Forms;")[0].notes[0],
        "Puy has no OS-specific units, VCL or FMX"
    );
    assert_eq!(codes("uses System.SysUtils, System.SysUtils;"), ["E2500"]);
    assert_eq!(
        codes("program P;\nvar X: Integer;\nbegin X := StrToInt('1') end."),
        ["E0700"]
    );
    let d = check("program P;\nvar X: Integer;\nbegin X := StrToInt('1') end.");
    assert_eq!(
        d[0].fixes[0].edits[0].replacement,
        "\nuses System.SysUtils;"
    );
    assert_eq!(d[0].fixes[0].edits[0].range.start(), 10.into());
    assert_eq!(codes("var X := 'a' + 1;"), ["E0703"]);
    let d = check("WriteLn('Total: ' + 1);");
    assert_eq!(d[0].fixes.len(), 2);
    assert_eq!(d[0].fixes[1].edits[0].replacement, ", ");
}

#[test]
fn builtin_procs() {
    assert_clean("var I := 0; Inc(I); Dec(I, 2); var C := 'a'; Inc(C);");
    // A constant is not a variable (E0706, same as for a `var` argument)
    assert_eq!(codes("const C = 1; Inc(C);"), ["E0706"]);
    assert_eq!(codes("Inc(1);"), ["E0706"]);
    assert_eq!(
        codes("var S := ''; ReadLn(S); var I := 0; ReadLn(I); ReadLn;"),
        Vec::<String>::new()
    );
    assert_eq!(codes("var C := 'a'; ReadLn(C);"), ["E3200"]);
    assert_eq!(codes("var A := 0; var B := 0; ReadLn(A, B);"), ["E3200"]);
    assert_eq!(codes("ReadLn('a');"), ["E0706"]);
    assert_clean(
        "Assert(1 = 1); Assert(True, 'msg'); if True then Halt; if True then Halt(2); Exit;",
    );
    assert_eq!(codes("Assert('x');"), ["E0702"]);
    assert_eq!(codes("WriteLn(1:2:3);"), ["E0717"]);
    assert_eq!(codes("WriteLn(1:'a');"), ["E0702"]);
    assert_eq!(codes("WriteLn(ErrOutput, 1);"), ["E3200"]);
    assert_clean("Write; Write(); WriteLn; WriteLn();");
    assert_eq!(
        codes("WriteLn(ParamCount, ParamStr(0), Length('a'), Copy('abc', 1, 2), Pos('b', 'abc'));"),
        Vec::<String>::new()
    );
}

#[test]
fn for_loops() {
    assert_clean("for var I := 1 to 10 do WriteLn(I);");
    assert_clean("var I: Integer := 0; for I := 1 to 3 do WriteLn(I);");
    assert_eq!(codes("for I := 1 to 3 do WriteLn(I);"), ["E0700", "E0700"]);
    let d = check("for I := 1 to 3 do WriteLn(I);");
    assert_eq!(d[0].fixes[0].edits[0].replacement, "var ");
    assert_eq!(codes("for var I := 1 to 3 do I := 2;"), ["E0707"]);
    assert_eq!(
        codes("for var I := 1 to 3 do ;\nWriteLn(I);"),
        ["W0018", "E0700"]
    );
    assert_eq!(
        codes("var I := 0; for I := 1 to 3 do ;\nWriteLn(I);"),
        ["W0018", "E2004"]
    );
    // A value parameter can be a loop variable; a const parameter gives E0707, a var / out parameter E0722
    assert_eq!(
        codes("procedure P(I: Integer); begin for I := 1 to 3 do ; end;"),
        ["W0018"]
    );
    assert_eq!(
        codes("procedure P(I: Integer); begin for I := 1 to 3 do ; WriteLn(I); end;"),
        ["W0018", "E2004"]
    );
    assert_eq!(
        codes("procedure P(const I: Integer); begin for I := 1 to 3 do ; end;"),
        ["E0707", "W0018"]
    );
    assert_eq!(
        codes("procedure P(var I: Integer); begin for I := 1 to 3 do ; end;"),
        ["E0722", "W0018"]
    );
    assert_eq!(
        codes("procedure P(out I: Integer); begin I := 0; for I := 1 to 3 do ; end;"),
        ["E0722", "W0018"]
    );
    assert_eq!(
        codes("var S := ''; for S := 'a' to 'b' do ;"),
        ["E0722", "W0018"]
    );
    assert_eq!(
        codes(
            "program P;\nvar I: Integer;\nprocedure Q; begin for I := 1 to 2 do ; end;\nbegin for I := 1 to 2 do ; end."
        ),
        ["E0722", "W0018", "W0018"]
    );
    assert_eq!(
        codes("for var I := 1 to 3 do for I := 1 to 2 do ;"),
        ["E0707", "W0018"]
    );
}

#[test]
fn definite_assignment() {
    assert_eq!(codes("var X: Integer; WriteLn(X);"), ["E2000"]);
    let d = check("var X: Integer; WriteLn(X);");
    assert_eq!(d[0].fixes[0].edits[0].replacement, " = 0");
    let d = check("begin var X: Integer; WriteLn(X); end;");
    assert_eq!(d[0].fixes[0].edits[0].replacement, " := 0");
    assert_eq!(
        codes("procedure P; var S: string; begin S := S + 'x'; end;"),
        ["E2000"]
    );
    let d = check("procedure P; var S: string; begin S := S + 'x'; end;");
    assert_eq!(d[0].fixes[0].edits[0].replacement, " = ''");
    assert_eq!(
        d[0].fixes[0].title,
        "add an initial value: `var S: string = '';`"
    );
    assert_eq!(
        all_codes("procedure P; var A, B: Integer; begin WriteLn(A); B := 1; end;"),
        ["W0001", "E2000"]
    );
    assert_clean("var X: Integer; if True then X := 1 else X := 2; WriteLn(X);");
    assert_eq!(
        codes("var X: Integer; if True then X := 1; WriteLn(X);"),
        ["E2000"]
    );
    assert_eq!(
        codes("var X: Integer; while True do X := 1; WriteLn(X);"),
        ["E2000"]
    );
    assert_clean("var X: Integer; repeat X := 1; until True; WriteLn(X);");
    assert_eq!(
        codes("var X: Integer; repeat Break; X := 1; until True; WriteLn(X);"),
        ["W0003", "E2000"]
    );
    assert_eq!(codes("function F: Integer; begin end;"), ["E2001"]);
    assert_eq!(
        codes("function F(B: Boolean): Integer; begin if B then Exit; Result := 1; end;"),
        ["E2001"]
    );
    let d = check("function F(B: Boolean): Integer; begin if B then Exit; Result := 1; end;");
    assert_eq!(d[0].secondary[0].label, "returns here");
    assert_eq!(d[0].fixes[0].edits[0].replacement, " Result := 0;");
    assert_clean("function F(B: Boolean): Integer; begin if B then Exit(1); Result := 2; end;");
    assert_eq!(
        codes("function F: Integer; begin Result := Result + 1; end;"),
        ["E2000"]
    );
    assert_eq!(
        codes("procedure P(out X: Integer); begin WriteLn(X); X := 1; end;"),
        ["E2002"]
    );
    assert_eq!(codes("procedure P(out X: Integer); begin end;"), ["E2002"]);
    assert_clean("procedure P(out X: Integer); begin X := 1; end;");
    assert_eq!(codes("var X: Integer := 0; Halt; X := 1;"), ["W0003"]);
    assert_eq!(
        all_codes("var X := 1; var Y: Integer; Y := 2;"),
        ["W0001", "W0001"]
    );
    assert_eq!(all_codes("var X := 1; WriteLn(X);"), Vec::<String>::new());
    assert_eq!(all_codes("for var I := 1 to 2 do ;"), ["W0018"]);
    let d = check("var X := 1;");
    assert_eq!(d[0].fixes[0].edits[0].range.end(), 11.into());
    assert_eq!(
        codes("var S := ''; ReadLn(S); var N: Integer; ReadLn(N); WriteLn(S, N);"),
        Vec::<String>::new()
    );
    assert_clean("var S := 'abc'; S[1] := 'x'; WriteLn(S);");
    assert_eq!(codes("var S: string; S[1] := 'x';"), ["E2000"]);
    assert_eq!(
        codes("procedure P(var A: Integer); begin A := 1; end; var X: Integer; P(X);"),
        ["E2000"]
    );
    assert_clean(
        "procedure P(out A: Integer); begin A := 1; end; var X: Integer; P(X); WriteLn(X);",
    );
    assert_eq!(
        all_codes("{$WARN unused-variable OFF}\nvar X := 1;"),
        Vec::<String>::new()
    );
    assert_eq!(
        all_codes("{$WARN unused-variable ERROR}\nvar X := 1;"),
        ["W0001"]
    );
    assert_eq!(
        check("{$WARN unused-variable ERROR}\nvar X := 1;")[0].severity,
        crate::diagnostics::Severity::Error
    );
    assert_eq!(
        check("var X := 1;")[0].notes,
        ["disable with `{$WARN unused-variable OFF}`"]
    );
}

#[test]
fn syntax_errors_skip_flow() {
    // Control-flow analysis is skipped for routines containing syntax errors
    assert_eq!(
        codes("procedure P; var X: Integer; begin WriteLn(X) WriteLn(1); end;"),
        ["E0400"]
    );
    assert_eq!(codes("var X: Integer;\nWriteLn(X)\nWriteLn(1);"), ["E0400"]);
    assert_eq!(codes("X := ;"), ["E0700", "E0403"]);
}

#[test]
fn function_name_assignment() {
    // Assigning to the function name assigns to `Result` (4.3)
    assert_clean("function Sq(X: Integer): Integer; begin Sq := X * X; end;\nWriteLn(Sq(3));");
    assert_clean(
        "function F(X: Int64): Int64; begin if X <= 1 then F := 1 else F := X * F(X - 1); end;\nWriteLn(F(3));",
    );
    // Cannot assign to the name of another function
    assert_eq!(
        codes(
            "function A: Integer; begin Result := 1; end;\nfunction B: Integer; begin A := 1; Result := 2; end;"
        ),
        ["E0707"]
    );
    // Cannot assign to the name of a procedure either
    assert_eq!(codes("procedure P; begin P := 1; end;"), ["E0707"]);
    // Nor can it be the target of `Inc`
    assert_eq!(
        codes("function F: Integer; begin Result := 0; Inc(F); end;"),
        ["E0706"]
    );
}

#[test]
fn paren_chain_fix() {
    // Fix that parenthesizes `X = 1 and Y = 2` (safe)
    let d = find(
        "var X := 1; var Y := 2;\nif X = 1 and Y = 2 then WriteLn(1);",
        "E0703",
    );
    assert_eq!(d.fixes.len(), 1);
    assert_eq!(d.fixes[0].title, "add parentheses around each comparison");
    assert_eq!(
        d.fixes[0].applicability,
        crate::diagnostics::Applicability::Safe
    );
    let edits: Vec<(u32, &str)> = d.fixes[0]
        .edits
        .iter()
        .map(|e| (u32::from(e.range.start()), e.replacement.as_str()))
        .collect();
    assert_eq!(edits, [(27, "("), (32, ")"), (37, "("), (42, ")")]);
    // A parenthesized part is treated as a leaf; a lone Boolean part is not wrapped
    let d = find(
        "var X := 1; var B := True;\nif X = 1 and B then WriteLn(1);",
        "E0703",
    );
    assert_eq!(d.fixes[0].edits.len(), 2);
    let d = find(
        "var X := 1; var Y := 2;\nif X = 1 and (Y = 2) then WriteLn(1);",
        "E0703",
    );
    assert_eq!(d.fixes[0].edits.len(), 2);
    // No parentheses when a part does not type as Boolean
    let d = find(
        "var S := 'a'; var Y := 2;\nif 'a' = S and Y then WriteLn(1);",
        "E0703",
    );
    assert!(d.fixes.is_empty());
    // Parts containing arithmetic (`Y + 1 = 2`) are also typed by precedence
    let d = find(
        "var X := 1; var Y := 2;\nif X = 1 and Y + 1 = 2 then WriteLn(1);",
        "E0703",
    );
    assert_eq!(d.fixes[0].edits.len(), 4);
    let d = find(
        "var X := 1; var S := 'a';\nif X = 1 and S then WriteLn(1);",
        "E0703",
    );
    assert!(d.fixes.is_empty());
    // Diagnostics from the trial type check do not remain
    assert_eq!(
        codes("var X := 1; var Y := 2;\nif X = 1 and Y = 2 then WriteLn(1);"),
        ["E0703"]
    );
}

#[test]
fn write_width_is_64bit() {
    // The width is a 64-bit integer expression (`WriteLn('x':3000000000)` raises EOutOfMemory at run time; 6.1)
    assert_clean("WriteLn('x':3000000000);");
    assert_clean("var L: Int64 := 3; WriteLn('x':L);");
    assert_eq!(codes("WriteLn('x':'y');"), ["E0702"]);
}

#[test]
fn script_top_level_is_one_scope() {
    // Top-level variables, constants and routines share one scope (1.2)
    assert_eq!(codes("var N := 1; const N = 2;"), ["E0701"]);
    assert_eq!(codes("var Foo := 1; procedure Foo; begin end;"), ["E0701"]);
    assert_eq!(codes("procedure Foo; begin end; var Foo := 1;"), ["E0701"]);
    assert_eq!(codes("const C = 1; procedure C; begin end;"), ["E0701"]);
    // Routine parameters and locals may share names with top-level variables
    assert_clean(
        "var X := 1; procedure P(X: Integer); var Y: Integer; begin Y := X; WriteLn(Y); end;\nP(X);",
    );
}
