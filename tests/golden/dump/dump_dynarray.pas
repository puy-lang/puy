program DumpDynArray;

// The `hir` and `bytecode` dumps of dynamic arrays (semantics.md 9.1, bytecode.md 5.2):
// constructors, `nil`, elements, `SetLength` with two lengths, `Copy`, `High`, `Assigned`
// and an open array parameter.

type
  TInts = array of Integer;
  TGrid = TArray<TInts>;

function Total(const A: array of Integer): Integer;
var
  I: Integer;
begin
  Result := 0;
  for I := 0 to High(A) do
    Result := Result + A[I];
end;

var
  A: TInts;
  G: TGrid;
begin
  A := [1, 2];
  SetLength(G, 2, 3);
  G[1][2] := Total(A);
  A := Copy(G[1]);
  if Assigned(A) and (A <> nil) then
    WriteLn(Total(A));
  A := nil;
end.
