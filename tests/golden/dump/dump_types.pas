program DumpTypes;

// The dumps of records and static arrays (semantics.md 9.1, bytecode.md 5.1, 5.2): `.type`,
// `field` / `index`, a constant shape, `Unique`, `FieldSet`, `ChkIndex` and a cell.

type
  TPoint = record
    X: Integer;
    Name: string;
  end;
  TRow = array[1..2] of TPoint;

const
  K: TPoint = (X: 1; Name: 'a');

procedure Fill(var N: Integer);
begin
  N := 3;
end;

var
  P: TPoint;
  R: TRow;
  I: Integer;
begin
  P.X := 1;
  P := K;
  I := 2;
  R[I].Name := 'b';
  Fill(R[1].X);
  WriteLn(P.X, R[1].X, R[I].Name);
end.
