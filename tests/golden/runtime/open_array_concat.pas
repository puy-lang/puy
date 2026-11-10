program OpenArrayConcat;

// `+` and Concat take an open array parameter like a dynamic array of its element type
// (types.md 8.6): the result is a new dynamic array, and a constructor next to it takes that
// type. (FPC reads such a constructor as a set, so this case is not in diff/.)

type
  TInts = array of Integer;

function Plus(const A: array of Integer): TInts;
begin
  Result := A + [100];
end;

function Join(const A, B: array of Integer): TInts;
begin
  Result := Concat([0], A, B);
end;

procedure Show(const A: TInts);
var
  X: Integer;
begin
  Write(Length(A), ':');
  for X in A do
    Write(' ', X);
  WriteLn;
end;

var
  S: array[1..2] of Integer = (7, 8);
begin
  Show(Plus([1, 2]));
  Show(Plus([]));
  Show(Plus(S));
  Show(Join(S, [9]));
end.
