program StringBuiltins;

// String built-ins (semantics.md 5.1, 5.3) at the edges FPC agrees on: `Copy` with an index
// or count outside the string, `Pos` with no match and an empty needle, `Concat` of many
// arguments, `Insert` / `Delete` in a loop, `SetLength` to shorten and lengthen, `StringOfChar`,
// `UpperCase` / `LowerCase`, `Trim`, `IntToStr` in concatenation, and comparisons.

uses System.SysUtils;

var
  S, T: string;
  I, N: Integer;
begin
  S := 'abcdef';
  WriteLn('[', Copy(S, 0, 2), '] [', Copy(S, 5, 10), '] [', Copy(S, 7, 1), '] [', Copy(S, 3, 0), '] [', Copy(S, 2, -1), ']');
  WriteLn('[', Copy(S, 2, 2147483647), '] [', Copy('', 1, 1), '] [', Copy(S, -5, 3), ']');
  WriteLn(Pos('cd', S), ' ', Pos('x', S), ' ', Pos('', S), ' ', Pos('f', S), ' ', Pos('abcdefg', S), ' ', Pos('a', ''));
  WriteLn(Concat('a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'), ' ', Length(Concat(S, S, S)));

  // Remove every vowel with Delete, then put a dash between letters with Insert
  T := 'programming language';
  I := 1;
  while I <= Length(T) do
    if Pos(T[I], 'aeiou') > 0 then
      Delete(T, I, 1)
    else
      Inc(I);
  WriteLn(T);
  I := Length(T);
  while I > 1 do
  begin
    Insert('-', T, I);
    Dec(I);
  end;
  WriteLn(T);

  SetLength(S, 3);
  WriteLn(S, ' ', Length(S));
  SetLength(S, 5); // the new characters are #0 in Puy and not set in FPC, so they are not read
  WriteLn(Length(S), ' ', Copy(S, 1, 3));
  S[4] := 'X';
  S[5] := 'Y';
  WriteLn(S);
  WriteLn('[', StringOfChar('=', 5), '] [', StringOfChar('x', 1), ']'); // FPC takes an AnsiChar
  WriteLn(UpperCase('Hello, World 1'), ' ', LowerCase('Hello, World 1'));
  WriteLn('[', Trim('  x y  '), '] [', Trim(''), '] [', Trim(#9' tab '#10), ']');
  N := 0;
  S := '';
  for I := 1 to 5 do
  begin
    S := S + IntToStr(I * I);
    if I < 5 then
      S := S + ',';
    N := N + Length(IntToStr(I * I));
  end;
  WriteLn(S, ' ', N);
  WriteLn('abc' < 'abd', ' ', 'ab' < 'abc', ' ', 'B' < 'a', ' ', '' < 'a', ' ', 'abc' = 'ab' + 'c');
end.
