program SysUtilsBuiltins;

// The System.SysUtils built-ins of M2b (semantics.md 5.3) with results FPC shares: Format with
// each kind, index, width, precision, `*` and `%%`, IntToHex, StrToBool / BoolToStr,
// TryStrToInt, TrimLeft / TrimRight, SameText / CompareText / CompareStr, StringReplace,
// StringOfChar and QuotedStr. Defaults of the real kinds, ties, narrow integer types and
// Unicode case folding differ from FPC and are in golden/runtime/ (out of range TryStrToInt
// too: FPC truncates).

uses System.SysUtils;

var
  I, V: Integer;
  N: Int64;
  W: Word;
  D: Double;
  C: Char;
  S: string;
  B: Boolean;
begin
  I := 42;
  N := -1234567890123;
  W := 65535;
  D := 3.14159;
  C := 'z';
  S := 'abc';
  // Integers
  WriteLn(Format('[%d] [%5d] [%-5d] [%.4d] [%.4d]', [I, I, I, I, -I]));
  WriteLn(Format('[%d] [%u] [%x] [%.6x] [%X]', [N, W, 255, 255, I]));
  WriteLn(Format('[%u] [%x]', [-1, -1]));
  // Strings and characters
  WriteLn(Format('[%s] [%8s] [%-8s] [%.2s] [%s]', [S, S, S, S, C]));
  WriteLn(Format('[%s] [%s]', ['日本語', 'x']));
  // Index, `*` and `%%`
  WriteLn(Format('%1:s %0:s %s %s', ['a', 'b', 'c']));
  WriteLn(Format('[%*d] [%-*d] [%.*d] [%%] [100%%]', [6, 7, 6, 7, 3, 7]));
  // Reals with a precision
  WriteLn(Format('[%.2f] [%.0f] [%8.3f] [%-8.1f]', [D, D, D, D]));
  WriteLn(Format('[%.3e] [%.4e]', [1234.5, -0.000123]));
  WriteLn(Format('[%.3g] [%.6g]', [0.0001234, 1234.5]));
  WriteLn(Format('[%.2n] [%.1n] [%.2n]', [1234567.891, -9876543.21, 12.0]));
  // IntToHex
  WriteLn(IntToHex(255, 4), ' ', IntToHex(4096, 1), ' ', IntToHex(-1, 2), ' ', IntToHex(N, 4), ' ', IntToHex(W, 2));
  // StrToBool / BoolToStr
  WriteLn(StrToBool('true'), ' ', StrToBool('FALSE'), ' ', StrToBool('0'), ' ', StrToBool('-3'));
  WriteLn(BoolToStr(True), ' ', BoolToStr(False), ' ', BoolToStr(True, True), ' ', BoolToStr(False, True));
  // TryStrToInt
  B := TryStrToInt('123', V);
  WriteLn(B, ' ', V);
  B := TryStrToInt('$1F', V);
  WriteLn(B, ' ', V);
  B := TryStrToInt('12x', V);
  WriteLn(B, ' ', V);
  TryStrToInt('77', V); // as a statement
  WriteLn(V);
  // TrimLeft / TrimRight
  WriteLn('[', TrimLeft('  a b  '), '] [', TrimRight('  a b  '), '] [', TrimLeft(#9#10'x'), ']');
  // SameText / CompareText / CompareStr (only the sign of the comparisons is fixed)
  WriteLn(SameText('Hello', 'hELLO'), ' ', SameText('a', 'b'), ' ', SameText('a', 'ab'));
  WriteLn(CompareText('abc', 'ABC') = 0, ' ', CompareText('abc', 'ABD') < 0, ' ', CompareText('b', 'A') > 0, ' ', CompareText('a', '_') < 0);
  WriteLn(CompareStr('abc', 'abc') = 0, ' ', CompareStr('a', 'B') > 0, ' ', CompareStr('ab', 'abc') < 0);
  // StringReplace
  WriteLn(StringReplace('a-b-c', '-', '+', []), ' ', StringReplace('a-b-c', '-', '+', [TReplaceFlag.rfReplaceAll]));
  WriteLn(StringReplace('aXbxc', 'x', '', [TReplaceFlag.rfReplaceAll, TReplaceFlag.rfIgnoreCase]), ' ', StringReplace('aaa', 'aa', 'b', [TReplaceFlag.rfReplaceAll]));
  // StringOfChar and QuotedStr
  WriteLn('[', StringOfChar('*', 3), '] [', StringOfChar('*', 0), '] [', StringOfChar('*', -2), ']');
  WriteLn(QuotedStr('it''s'), ' ', QuotedStr(''));
end.
