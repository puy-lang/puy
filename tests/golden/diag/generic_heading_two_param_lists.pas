program GenericHeadingTwoParamLists;

// A routine heading takes at most one type parameter list after each name
// (`GenericName`, grammar.md 1.3): a second list after a routine's name is a syntax error
// (E0400), so the semantic analysis never sees one.

function F<T><U>(x: T): T;
begin
  Result := x;
end;

begin
end.
