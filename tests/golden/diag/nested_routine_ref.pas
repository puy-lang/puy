program NestedRoutineRef;

// The nested routine takes precedence over a global of the same name in the outer body
// (semantics.md 4.9): it is called, and as a loop variable it is not a variable (E0707)

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
