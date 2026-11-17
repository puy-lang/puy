program ParenCallStmt;

// A call statement whose callee is parenthesized is typed like the same call in an expression
// (objects.md 9.4, 9.6): each form below reports its error instead of the statement being
// dropped without a diagnostic.

function Two: Integer;
begin
  Result := 2;
end;

var
  P: TProc;
  N: Integer;
  G: function: Integer;
begin
  P := procedure begin WriteLn('p') end;
  G := Two;
  N := 1;
  (P)();                                 // E0725: `(P)` calls P, which has no value
  (G)();                                 // E0704: `(G)` is an Integer
  (procedure begin WriteLn('a') end)();  // E0729
  (WriteLn)();                           // E0725
  (N)();                                 // E0704
end.
