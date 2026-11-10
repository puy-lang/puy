program NestedArrayInsert;

// Insert into an array whose elements are dynamic arrays (types.md 8.6): a constructor source is
// always read as an array of the target's type, so one element is inserted as a one-element
// constructor (`[[1, 2]]`) or as a value of the element type; `+` works the same way.

type
  TInts = array of Integer;

var
  M: array of TInts = nil;
  T: TInts;
  R: TInts;
begin
  T := [1, 2];
  Insert(T, M, 0);
  Insert([[3], [4, 5]], M, 1);
  Insert([[6, 7, 8]], M, 0);
  M := M + [[9]];
  for R in M do
    WriteLn(Length(R), ' ', R[0]);
end.
