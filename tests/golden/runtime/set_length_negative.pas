program SetLengthNegative;

// A negative length in `SetLength` is an ERangeError (types.md 2.3), for a dynamic array and
// for a string; the array keeps its value.

var
  A: array of Integer;
  N: Integer;
begin
  SetLength(A, 2);
  N := -1;
  SetLength(A, N);
end.
