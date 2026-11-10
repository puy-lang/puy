program NestedOpenArray;

// Open array parameters of an enclosing routine (semantics.md 4.9, types.md 2.4): a value
// parameter written by a nested routine is the routine's own copy, a `var` parameter is the
// caller's array. (Kept out of diff/ as it is not checked against FPC.)
procedure P(A: array of Integer; const B: array of Integer; var C: array of Integer);

  procedure Q;
  begin
    A[0] := 99;
    C[0] := B[0] + 1;
  end;

begin
  Q;
  WriteLn(A[0], ' ', C[0]);
end;

var
  D: array of Integer;
  E: array[0..1] of Integer;
begin
  D := [1, 2];
  E[0] := 5;
  E[1] := 6;
  P(D, D, E);
  WriteLn(D[0], ' ', E[0]);
end.
