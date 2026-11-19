program AnonRecordDeclaredHere;

// A diagnostic naming an anonymous record shows it as `record` and points at its `record`
// keyword with `record type declared here` (types.md 1.2, diagnostics.md 2.2): once for each
// anonymous record it names, in the order they are named, also inside the name of another type.

var
  A: record
    X: Integer;
  end;
  B: record
    X: Integer;
  end;
  C: array[0..1] of record
    X: Integer;
  end;
  N: Integer;
begin
  N := A;
  A := B;
  N := N + A;
  A.Y := 1;
  N := C;
end.
