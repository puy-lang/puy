program NestedValueCycle;

// The value of a nested routine stored in a variable its group uses makes a cycle, like an
// anonymous method capturing itself (objects.md 9.7): the closure holds the cell of `Saved`, and
// `Saved` holds the closure. Puy does not warn; the leak check reports the closure by the name of
// its function and the cell by the variable's name.

function Make: TProc;
var
  Saved: TProc;

  procedure Show;
  begin
    WriteLn('Show ', Assigned(Saved));
  end;

begin
  Saved := nil;
  Saved := Show;
  Result := Show;
end;

procedure Run;
var
  P: TProc;
begin
  P := Make();
  P();
  P := nil;
end;

begin
  Run;
  WriteLn('end');
end.
