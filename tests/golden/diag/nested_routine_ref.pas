program NestedRoutineRef;

// Referring to a nested routine (E3200) by name in the outer body does not add E0700 and the like
// (semantics.md chapter 8). The nested routine takes precedence over a global of the same name

var
  Twice: string;

function Outer(N: Integer): Integer;
  function Twice(X: Integer): Integer;
  begin
    Result := X * 2;
  end;
begin
  Result := Twice(N) + 1;
  for Twice := 1 to 2 do
    WriteLn(N);
end;

begin
  Twice := 'a';
  WriteLn(Outer(1), Twice);
end.
