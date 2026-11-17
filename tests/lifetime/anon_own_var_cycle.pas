program AnonOwnVarCycle;

// An anonymous method stored in the variable it captures makes a cycle (objects.md 9.6): the cell
// holds the anonymous method and the anonymous method holds the cell, so neither is freed and the
// leak check reports both (no warning is given).

procedure Run;
var
  F: TProc;
begin
  F := nil;
  F := procedure
    begin
      if False then
        F();
      WriteLn('called');
    end;
  F();
end;

begin
  Run;
  WriteLn('end');
end.
