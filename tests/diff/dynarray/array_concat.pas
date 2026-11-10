program ArrayConcat;

// `+`, Concat, Insert and Delete of dynamic arrays (types.md 8.6), within the array's range
// (golden/ has the ERangeError outside it): `+` makes a new array, a constructor next to an
// array takes its type, and Insert takes one element or an array. (`+` of an open array
// parameter is in golden/: FPC reads the constructor next to it as a set.)

type
  TInts = array of Integer;

procedure Show(const A: array of Integer);
var
  I: Integer;
begin
  Write(Length(A), ':');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn;
end;

function Sum(const A: TInts): TInts;
begin
  Result := A + [100];
end;

var
  A, B, C: TInts;
  S: array of string;
begin
  A := [1, 2, 3];
  B := [4, 5];
  C := A + B;
  Show(C);
  C[0] := 99;
  Show(A);
  Show(A + [6] + B);
  Show([0] + A);
  Show(Concat(A, B, [7, 8]));
  Show(Concat(A));
  Show(Sum(B));
  Show(Sum([]));
  Insert(10, A, 0);
  Show(A);
  Insert(20, A, Length(A));
  Show(A);
  Insert([30, 31], A, 2);
  Show(A);
  Insert(B, A, 1);
  Show(A);
  Insert(A, A, 0);
  Show(A);
  Delete(A, 0, 2);
  Show(A);
  Delete(A, 3, MaxInt);
  Show(A);
  Delete(A, 1, 0);
  Delete(A, 100, -1);
  Show(A);
  C := nil;
  Insert(1, C, 0);
  Show(C);
  Delete(C, 0, 1);
  Show(C);
  S := ['b'];
  Insert('a', S, 0);
  S := S + ['c'];
  Delete(S, 1, 1);
  WriteLn(Length(S), ' ', S[0], S[1]);
end.
