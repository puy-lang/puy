program DumpBuiltinArgCopy;

// The counted copy of a global, a field or an element lent to a register range of a built-in
// instruction (`Pos` of a global, a record field, an element and an instance's field,
// `ConcatStrN`, `Format`, `Copy` of an array; lifetime.md 3.3) is let go of right after the
// instruction, so the next `G := G + 'x'` appends in place; a copy of an array of objects stays
// to the end of the routine (ir).

uses
  System.SysUtils;

type
  TRec = record
    S: string;
  end;

  TBox = class
  public
    S: string;
  end;

var
  G, H: string;
  R: TRec;
  A: array of Integer;
  B: array of TObject;
  SA: array of string;
  O: TBox;
  N, I: Integer;
begin
  for I := 1 to 3 do
  begin
    N := Pos('x', G);
    G := G + 'x';
  end;
  N := N + Pos('x', R.S);
  SetLength(SA, 1);
  N := N + Pos('x', SA[0]);
  O := TBox.Create;
  N := N + Pos('x', O.S);
  O.Free;
  H := '[' + G + ']';
  H := Format('%s', [G]);
  A := Copy(A, 0, 1);
  B := Copy(B, 0, 1);
  WriteLn(N, ' ', G, ' ', H, ' ', Length(A), ' ', Length(B));
end.
