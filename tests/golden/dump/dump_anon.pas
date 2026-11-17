program DumpAnon;

// The dumps of anonymous methods and captured variables (objects.md 15.5): `anon fN` and the
// `.routine … closure` of its function with `.capture` lines; the variables in cells (` cell`):
// a value parameter (moved into its cell), a local, `for var` (a new cell each iteration) and an
// inline variable (a new cell where it is declared); the closure's own variables (`CapGet` and
// `CellRef`), and an anonymous method inside one passing on a variable two routines out.

procedure Run(N: Integer);
var
  Total: Integer;
  P: TProc;
begin
  Total := 0;
  for var I := 1 to 2 do
  begin
    var K: Integer;
    K := I;
    P := procedure
      var
        Q: TProc;
      begin
        Total := Total + I * K + N;
        Q := procedure begin WriteLn(Total) end;
        Q();
      end;
    P();
  end;
end;

begin
  Run(10);
end.
