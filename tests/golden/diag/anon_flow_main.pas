program AnonFlowMain;

// Definite assignment of anonymous methods in `<main>` (objects.md 9.12): making one reads the
// inline variables of `<main>` its body may read before assigning them (E2000 with
// `captured here`), as in a routine; global variables are not checked.

var
  G: Integer;
  P: TProc;
begin
  var A: Integer;
  var B: Integer;
  P := procedure
    begin
      WriteLn(A, G);
      B := 1;
      WriteLn(B);
    end;
  P();
  var C: Integer;
  P := procedure
    var
      R: TProc;
    begin
      R := procedure begin WriteLn(C) end;
      R();
    end;
  P();
end.
