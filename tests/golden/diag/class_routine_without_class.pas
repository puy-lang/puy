program ClassRoutineWithoutClass;

// A routine heading that starts with `class` but names no class (objects.md 3.2, 16) is E0747,
// like `constructor Foo;`: `class procedure`, `class function`, `class constructor`,
// `class destructor` and `class operator`, at the top level and nested in a routine. The word is
// the one written, lower-cased.

class procedure Bar;
begin
  WriteLn('Bar');
end;

Class Function Answer: Integer;
begin
  Result := 42;
end;

class constructor Setup;
begin
end;

class destructor Teardown;
begin
end;

class operator Add(A, B: Integer): Integer;
begin
  Result := A + B;
end;

procedure Outer;

  class procedure Inner;
  begin
    WriteLn('Inner');
  end;

  class function Twice(X: Integer): Integer;
  begin
    Result := X * 2;
  end;

begin
  WriteLn('Outer');
end;

begin
  Outer;
end.
