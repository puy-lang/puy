program RecordFlow;

// Definite assignment of records and static arrays (types.md chapter 6): writing a field or
// element reads the whole variable, so it must be assigned first. A named type gets the fix
// `= Default(T)`; an anonymous type gets notes instead.

type
  TPoint = record
    X: Integer;
  end;

procedure Local;
var
  A: TPoint;
  B: array[0..2] of Integer;
begin
  A.X := 1;
  B[0] := 1;
  WriteLn(A.X, B[0]);
end;

function Make: TPoint;
begin
  Result.X := 1;
end;

begin
  Local;
  var Q: TPoint;
  Q.X := Make.X;
  WriteLn(Q.X);
end.
