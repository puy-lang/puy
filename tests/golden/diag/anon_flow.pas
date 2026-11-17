program AnonFlow;

// Definite assignment and anonymous methods (objects.md 9.6): making an anonymous method reads the
// captured variables its body (or an anonymous method inside it) may read before assigning them
// (E2000 with `captured here`); one that assigns before reading, or only assigns, reads nothing;
// what the body assigns does not count as assigned outside it, and inside the body a captured
// variable counts as assigned.

procedure Reads;
var
  A, B, C, D: Integer;
  P, Q: TProc;
begin
  P := procedure
    begin
      WriteLn(A);
      B := 1;
      WriteLn(B);
    end;
  Q := procedure
    var
      R: TProc;
    begin
      R := procedure begin WriteLn(C) end;
      R();
      D := 2;
    end;
  P();
  Q();
  WriteLn(D);
end;

begin
  Reads;
end.
