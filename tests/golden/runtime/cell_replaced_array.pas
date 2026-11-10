program CellReplacedArray;

// An element of a dynamic array passed to a `var` parameter is written to the array it was in
// when the call started, even if the callee resizes the variable meanwhile (types.md chapter 3;
// Delphi leaves this undefined). A field of a record element is the same.

type
  TItem = record
    N: Integer;
  end;

var
  A, Keep: array of Integer;
  R: array of TItem;

procedure Write5(var X: Integer);
begin
  SetLength(A, 4);
  X := 5;
end;

procedure WriteR(var X: Integer);
begin
  R := nil;
  X := 7;
end;

begin
  A := [1, 2];
  Keep := A;
  Write5(A[0]);
  WriteLn(A[0], ' ', Length(A), ' ', Keep[0]);
  SetLength(R, 1);
  WriteR(R[0].N);
  WriteLn(Length(R));
end.
