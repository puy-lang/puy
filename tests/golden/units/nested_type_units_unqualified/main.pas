program Main;

// A type nested in a class or record of a unit in `uses` is named through the outer type
// without the unit (`TC.TI`) in every type position (units.md 3.1, objects.md 2.2).

uses Shapes;

type
  TA = TC.TI;
  TD = class(TC.TI)
  end;
  TRef = class of TC.TI;
  TArr = TArray<TC.TI>;

var
  I: TC.TI;
  E: TC.TE;
  R: TC.TR;
  J: TRec.TI;
  A: TArray<TC.TI>;
  B: TArr;
  D: TD;
  K: TRef;
begin
  I := TA.Create;
  I.V := 3;
  E := TC.TE.eC;
  R.X := 4;
  J := TRec.TI.Create;
  J.W := 5;
  A := [I];
  B := A;
  D := TD.Create;
  D.V := 6;
  K := TD;
  WriteLn(A[0].V, ' ', B[0].V, ' ', Ord(E), ' ', R.X, ' ', J.W, ' ', D.V, ' ', K.ClassName);
  D.Free;
  J.Free;
  I.Free;
end.
