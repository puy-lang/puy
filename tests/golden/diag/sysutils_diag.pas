program SysUtilsDiag;

// Diagnostics of the System.SysUtils built-ins (semantics.md 5.3): an `array of const` element of
// a type it cannot take (E0702), an argument that is not a constructor, a range, the argument
// counts, the `out` argument of TryStrToInt, a non-integer IntToHex, the flags of StringReplace,
// and an `array of const` parameter a program declares (E3200 until M3).

uses System.SysUtils;

type
  TColor = (Red, Green);
  TPoint = record
    X: Integer;
  end;

procedure Log(const Args: array of const);
begin
end;

var
  C: TColor;
  P: TPoint;
  A: array of Integer;
  N: Integer;
  B: Byte;
begin
  C := TColor.Red;
  P.X := 1;
  A := [1];
  N := 0;
  B := 0;
  WriteLn(Format('%d', [C]));
  WriteLn(Format('%d', [P, A]));
  WriteLn(Format('%d', [[TColor.Red]]));
  WriteLn(Format('%d', N));
  WriteLn(Format('%d', [1..2]));
  WriteLn(Format('%d'));
  WriteLn(Format(1, [N]));
  WriteLn(TryStrToInt('1', 5));
  WriteLn(TryStrToInt('1', B));
  WriteLn(TryStrToInt('1'));
  WriteLn(IntToHex(1.5, 2));
  WriteLn(BoolToStr(True, True, True));
  WriteLn(StringReplace('a', 'b', 'c', [TReplaceFlag.rfIgnoreCase, 3]));
  WriteLn(StringReplace('a', 'b', 'c', [rfReplaceAll]));
  WriteLn(StringOfChar('ab', 2));
  raise Exception.CreateFmt('%d', [C]);
end.
