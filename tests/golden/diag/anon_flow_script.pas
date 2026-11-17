// Definite assignment of anonymous methods in a script's `<main>` (objects.md 9.12): making one
// reads the top-level variables its body may read before assigning them (E2000 with
// `captured here`); one assigned first is not reported.

var A: Integer;
var B: Integer;
var P: TProc := procedure
  begin
    WriteLn(A);
    B := 1;
    WriteLn(B);
  end;
P();
var C: Integer;
C := 2;
P := procedure begin WriteLn(C) end;
P();
