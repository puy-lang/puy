program RecordCopy;

// Records and static arrays are values: assignment, value parameters and function results
// copy them, so writing to the copy leaves the original alone (types.md 2.1, 2.2).

type
  TPoint = record
    X, Y: Integer;
    Name: string;
  end;
  TLine = record
    A, B: TPoint;
  end;
  TRow = array[0..2] of Integer;

procedure Move(P: TPoint);
begin
  P.X := P.X + 100;
  P.Name := P.Name + '!';
  WriteLn('inside: ', P.Name, ' ', P.X);
end;

function Shifted(P: TPoint; D: Integer): TPoint;
begin
  Result := P;
  Result.X := Result.X + D;
  Result.Y := Result.Y + D;
end;

function Twice(const R: TRow): TRow;
var
  I: Integer;
begin
  Result := R;
  for I := 0 to 2 do
    Result[I] := R[I] * 2;
end;

var
  P, Q: TPoint;
  L, M: TLine;
  R, S: TRow;
begin
  P := Default(TPoint);
  P.X := 1;
  P.Y := 2;
  P.Name := 'p';
  Q := P;
  Q.X := 10;
  Q.Name := 'q';
  WriteLn(P.Name, ' ', P.X, ' ', P.Y);
  WriteLn(Q.Name, ' ', Q.X, ' ', Q.Y);
  Move(P);
  WriteLn('after: ', P.Name, ' ', P.X);
  Q := Shifted(P, 5);
  WriteLn(Q.X, ' ', Q.Y, ' ', P.X);
  WriteLn(Shifted(P, 7).Y);

  L := Default(TLine);
  L.A := P;
  L.B := L.A;
  L.B.Name := 'b';
  M := L;
  M.A.X := 99;
  WriteLn(L.A.Name, ' ', L.B.Name, ' ', L.A.X, ' ', M.A.X, ' ', M.B.Name);

  R[0] := 1;
  R[1] := 2;
  R[2] := 3;
  S := R;
  S[1] := 20;
  WriteLn(R[0], R[1], R[2], ' ', S[0], S[1], S[2]);
  S := Twice(R);
  WriteLn(S[0], ' ', S[1], ' ', S[2], ' ', R[2]);
end.
