program DumpForIn;

// The `hir` dump of `for..in` (semantics.md 9.1): the hidden variables for the collection and
// the position, a `for var` loop variable with a written type, and a global loop variable.

type
  TInts = array of Integer;

var
  G: Integer;
  Ints: TInts;

function Total(const A: array of Integer): Int64;
begin
  Result := 0;
  for var E: Int64 in A do
    Result := Result + E;
end;

begin
  Ints := [1, 2, 3];
  for G in Ints do
    if G = 2 then
      Continue;
  for var C in 'ab' do
    WriteLn(C);
  WriteLn(Total(Ints));
end.
