program ProcElementStmt;

// An element of an array of a procedural type written as a statement (`A[0];`, also in
// parentheses) is not a statement (E0402, from the parser). The element is typed as a value, not called, so no
// E0725 `has no value` is reported on top of it (objects.md 9.4).

type
  TProc = procedure;

procedure Hello;
begin
  WriteLn('hello');
end;

var
  A: array[0..1] of TProc;
begin
  A[0] := Hello;
  A[0];
  (A[0]);
  A[0]();
end.
