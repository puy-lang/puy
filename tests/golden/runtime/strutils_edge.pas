// System.StrUtils beyond what the diff case checks against FPC: SplitString, surrogate pairs in
// ReverseString, positions and counts far outside the string, case folding beyond ASCII, a
// result too long for a string, and IfThen with System.Math in the uses clause
// (docs/spec/stdlib/System.StrUtils/README.md, docs/spec/stdlib/System.SysUtils/IfThen.md).
program StrUtilsEdge;

uses
  System.SysUtils, System.Math, System.StrUtils;

procedure Show(const A: TStringDynArray);
var
  I: Integer;
begin
  Write(Length(A), ':');
  for I := 0 to High(A) do
    Write(' [', A[I], ']');
  WriteLn;
end;

var
  Parts: TStringDynArray;
begin
  // Every string starts and ends with '' (Delphi XE4's EndsText says it does not end with '')
  WriteLn(StartsStr('', 'abc'), ' ', StartsText('', 'abc'), ' ', EndsStr('', 'abc'), ' ',
    EndsText('', 'abc'), ' ', EndsText('', ''));

  // SplitString: each character of Delimiters separates; empty parts stay
  Show(SplitString('a,b;;c,', ',;'));
  Show(SplitString('', ','));
  Show(SplitString('abc', ''));
  Show(SplitString(',', ','));
  Parts := SplitString('x y', ' ');
  WriteLn(Parts[0], Parts[1]);

  // ReverseString keeps a surrogate pair in its order; a lone surrogate is one character
  WriteLn(ReverseString('a😀b'), ' ', ReverseString('😀😁'), ' ',
    Length(ReverseString('😀')));
  WriteLn(Ord(ReverseString(#$D800 + 'a')[1]), ' ', Ord(ReverseString(#$DC00 + #$D800)[1]));

  // Positions and counts far outside the string are cut to it
  WriteLn(PosEx('a', 'banana', -1), ' ', PosEx('a', 'banana', MaxInt), ' ',
    PosEx('a', 'banana', Low(Integer)));
  WriteLn('[', LeftStr('abc', MaxInt), '] [', RightStr('abc', MaxInt), '] [',
    MidStr('abc', MaxInt, MaxInt), '] [', MidStr('abc', Low(Integer), MaxInt), ']');
  WriteLn('[', StuffString('abc', 0, 1, 'X'), '] [', StuffString('abc', -5, 10, 'X'), '] [',
    StuffString('abc', 5, 1, 'X'), '] [', StuffString('abc', 2, -1, 'X'), '] [',
    StuffString('abc', 2, MaxInt, 'X'), '] [', StuffString('abc', Low(Integer), MaxInt, 'X'),
    '] [', StuffString('abc', MaxInt, MaxInt, 'X'), ']');

  // The *Text routines fold to lower case one character at a time, as AnsiSameText
  WriteLn(StartsText('ÉT', 'été'), ' ', EndsText('ΣΑΣ', 'σας'), ' ',
    ContainsText('x' + #$212A + 'y', 'k'), ' ', ContainsText('straße', 'SS'), ' ',
    IndexText('ÄB', ['x', 'äb']), ' ', ReplaceText('ÄäA', 'ä', '-'));

  // A result longer than a string can be
  try
    WriteLn(Length(DupeString('abcdefgh', MaxInt)));
  except
    on E: EOutOfMemory do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // One IfThen for numbers and strings, though System.Math and System.StrUtils are both used
  WriteLn(IfThen(True, 1, 2), ' ', IfThen(False, Int64(1), Int64(10000000000)), ' ',
    IfThen(True, 1.5):0:1, ' [', IfThen(True, 'a'), '] [', IfThen(False, 'a'), ']');
end.
