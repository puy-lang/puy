program DumpAssertProbes;

// The probes of an Assert condition, the marks of the right sides of `or` and `and`, the split
// fold, and the debug information section of the values recorded (docs/spec/semantics.md 5.4, 9;
// docs/spec/bytecode.md 3.2, 5.2).

type
  TColor = (Red, Green);
  TPoint = record
    X: Integer;
    C: TColor;
  end;

function Same(const Q: TPoint): Boolean;
begin
  Result := Q.X = 2;
end;

var
  P: TPoint;
  N: Integer;
begin
  N := 1;
  P.X := 2;
  Assert((N + 1 - P.X = 3) or (Same(P) and (P.C = TColor.Green)));
end.
