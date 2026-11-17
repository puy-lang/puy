program DumpAssignTry;

// Assignments to a local variable inside a `try` body (semantics.md 3.1, design/vm.md 3.15;
// M3-75): the value is made in a temporary and moved to the variable when an instruction after
// the first write may raise, and made in the variable when only the last instruction writes it
// or nothing may raise. The temporaries of the expression then keep their numbers and stack
// slots as if no temporary had been made for the value.

function F(A, B: Integer): Integer;
begin
  Result := A - B;
end;

procedure P(X, Y: Integer);
var
  Z: Integer;
  B: Boolean;
begin
  Z := 0;
  try
    {$Q-}
    // Only the last instruction writes the value: made in Z
    Z := X + 1;
    // The call's argument temporaries come after the dropped one: made in Z
    Z := F(X, Y + 1);
    {$Q+}
    // NarrowOvf may raise after the addition: a temporary, then Move
    Z := X + 1;
    // Branches, but nothing may raise: made in B
    B := (X > 0) and (Y > 0);
    WriteLn(Z, ' ', B);
  finally
    WriteLn('done');
  end;
end;

begin
  P(1, 2);
end.
