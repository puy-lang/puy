program DynIndexRange;

// An index outside a dynamic array is always an ERangeError, whatever `{$R}` says
// (types.md 2.3); the message gives the array's bounds (semantics.md 7.1).

{$R-}

var
  A: array of Integer;
  I: Integer;
begin
  A := [1, 2, 3];
  WriteLn(A[2]);
  I := 3;
  WriteLn(A[I]);
end.
