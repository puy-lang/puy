program ForInFixed;

// The collection of `for..in` is evaluated once on entry and its reference is held for the
// whole loop (types.md chapter 5): assigning another value or `SetLength` in the body does not
// change the number of iterations. A dynamic array is shared, so writing an element not yet
// visited is seen; a string is a value, so writes are not seen. (Assigning `nil` and writing
// a static array are in golden/runtime/for_in_held: FPC reads the variable itself.)

type
  TInts = array of Integer;

var
  D: TInts;
  S: string;
  E: Integer;
  C: Char;
  Calls: Integer;

function Make: TInts;
begin
  Inc(Calls);
  SetLength(Result, 3);
  Result[0] := 7;
  Result[1] := 8;
  Result[2] := 9;
end;

begin
  SetLength(D, 3);
  D[0] := 1;
  D[1] := 2;
  D[2] := 3;
  Write('set length:');
  for E in D do
  begin
    Write(' ', E);
    SetLength(D, 10);
  end;
  WriteLn(' (', Length(D), ')');

  SetLength(D, 3);
  D[0] := 1;
  D[1] := 2;
  D[2] := 3;
  Write('shared element:');
  for E in D do
  begin
    Write(' ', E);
    D[2] := 30;
  end;
  WriteLn;

  S := 'abc';
  Write('string value:');
  for C in S do
  begin
    Write(' ', C);
    S := S + 'x';
    S[3] := 'z';
  end;
  WriteLn(' (', S, ')');

  Calls := 0;
  Write('evaluated once:');
  for E in Make do
    Write(' ', E);
  WriteLn(' (', Calls, ')');
end.
