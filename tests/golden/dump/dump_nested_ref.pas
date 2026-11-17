program DumpNestedRef;

// The dumps of nested routines compiled as closures (objects.md 15.5): `nested-ref fN name block
// vN` and `call fN block vN` (`block link` inside a routine of the same group), the hidden
// variables holding the group blocks (`.group vN [cells]`, made with `MakeClosure` of function
// -1 on entry), a routine's own block copied for a routine nested in it (`.own-block vN`), the
// index in the group's block of each capture, and `ClosureWith` for the value.

function Make(Start: Integer): TProc;
var
  Count: Integer;

  procedure Show;
  begin
    WriteLn(Count);
  end;

  procedure Step(K: Integer);
    procedure Again;
    begin
      Step(K - 1);
    end;
  begin
    Inc(Count, Start);
    Show;
    if K > 0 then
      Again;
  end;

  procedure Run;
  begin
    Step(1);
  end;

begin
  Count := 0;
  Result := Run;
end;

var
  P: TProc;
begin
  P := Make(2);
  P();
end.
