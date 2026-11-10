program DynLifetime;

// Where the lifetime instructions go for dynamic arrays (lifetime.md 3.11): a constructor
// builds the array from its elements and moves it, `SetLength` changes the variable without
// counting, an element is read with `DynGetRef` and written with `DynSetRef` while shared, an
// open array argument from a static array is materialized, and an element passed to a `var`
// parameter makes a cell rooted at the array.

type
  TPair = array[0..1] of string;

procedure Touch(var S: string);
begin
  S := S + '!';
end;

function Count(const A: array of string): Integer;
begin
  Result := Length(A);
end;

var
  A: array of string;
  P: TPair;
  S: string;
begin
  S := 'x';
  A := [S, 'y'];
  SetLength(A, 3);
  A[2] := A[0];
  Touch(A[1]);
  WriteLn(A[1], Count(P), Count(A));
end.
