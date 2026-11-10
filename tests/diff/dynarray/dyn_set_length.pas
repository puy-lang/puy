program DynSetLength;

// `SetLength` makes the array unique before resizing it (types.md 2.3): other references keep
// the old elements. Growing gives default elements, shrinking drops the tail; several
// lengths resize the nested arrays.

type
  TInts = array of Integer;
  TPair = record
    Key: string;
    Value: Integer;
  end;

procedure Show(const Title: string; const A: TInts);
var
  I: Integer;
begin
  Write(Title, ':');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn(' (', Length(A), ')');
end;

procedure Grow(var A: TInts);
begin
  SetLength(A, Length(A) + 1);
  A[High(A)] := Length(A);
end;

var
  A, B: TInts;
  P: array of TPair;
  S: array of string;
  M: array of array of Integer;
  I, J: Integer;
begin
  SetLength(A, 3);
  Show('fresh', A);
  A[0] := 1; A[1] := 2; A[2] := 3;
  B := A;
  SetLength(A, 5);
  A[0] := 10;
  Show('A', A);
  Show('B', B);
  B := A;
  SetLength(B, 2);
  B[1] := 20;
  Show('A', A);
  Show('B', B);
  SetLength(B, 0);
  WriteLn(B = nil);
  Grow(B);
  Grow(B);
  Show('B', B);
  SetLength(P, 2);
  P[1].Key := 'k';
  P[1].Value := 5;
  SetLength(P, 3);
  WriteLn('[', P[1].Key, '] ', P[1].Value, ' [', P[2].Key, '] ', P[2].Value);
  SetLength(S, 3);
  S[2] := 'last';
  SetLength(S, 2);
  SetLength(S, 3);
  WriteLn('[', S[2], ']');
  SetLength(M, 2, 3);
  for I := 0 to High(M) do
    for J := 0 to High(M[I]) do
      M[I][J] := I * 10 + J;
  SetLength(M[1], 4);
  for I := 0 to High(M) do
    Show('M', M[I]);
end.
