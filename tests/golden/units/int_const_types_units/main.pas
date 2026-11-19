program Main;

// Whether a unit's constant had its type written carries to the programs that use it
// (semantics.md 3.2): KU (Integer(1)) and FU (Int64) keep their types, NU (folded from literals)
// adapts to the other operand like a literal

{$Q+}

uses System.SysUtils, Consts;

var
  C: Cardinal;
  V: Integer;

begin
  C := High(Cardinal);
  V := MaxInt;
  WriteLn(C + KU);
  WriteLn(V + FU);
  try
    WriteLn(C + NU);
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
end.
