program AnonMoreErrors;

// More errors of anonymous methods (objects.md 9.6, 9.9, 9.12): one going to a type that is not a
// procedural type cannot have its type inferred (E0729); one written before a loop that writes
// the loop variable is reported where the variable becomes the loop variable (E0707); the generic
// `TProc<Integer>` is a type of `System` (generics.md 6.2).

procedure Before;
var
  I: Integer;
  P: TProc;
begin
  P := procedure
    begin
      I := 5;
    end;
  for I := 1 to 2 do
    P();
end;

var
  N: Integer;
  G: TProc<Integer>;
begin
  N := procedure begin end;
  Before;
end.
