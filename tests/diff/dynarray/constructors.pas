program Constructors;

// Array constructors (types.md chapter 4): the type comes from the context (assignment,
// initial value, argument, `Exit`, another constructor's element). Static arrays and inline
// variables are in golden/runtime/static_array_ctor.

type
  TInts = array of Integer;
  TPoint = record
    X, Y: Integer;
  end;
  TPoints = array of TPoint;

function Squares(N: Integer): TInts;
var
  I: Integer;
begin
  if N = 0 then
    Exit([]);
  Result := [0];
  SetLength(Result, N);
  for I := 1 to N do
    Result[I - 1] := I * I;
end;

function Pick(First: Boolean): TInts;
begin
  if First then
    Exit([1])
  else
    Result := [2, 3];
end;

function At(X, Y: Integer): TPoint;
begin
  Result := Default(TPoint);
  Result.X := X;
  Result.Y := Y;
end;

procedure Show(const A: array of Integer);
var
  I: Integer;
begin
  for I := 0 to High(A) do
    Write(A[I], ' ');
  WriteLn('(', Length(A), ')');
end;

const
  Primes: TInts = [2, 3, 5];

var
  A: TInts = [];
  G: array of string = ['g1', 'g2'];
  M: array of TInts;
  P: TPoints;
  B: TInts;
  K: Integer;
  S: string;
begin
  K := 5;
  S := 'b';
  A := [K, K + 1, K * 2];
  Show(A);
  Show(Pick(True));
  Show(Pick(False));
  Show(Squares(0));
  Show(Squares(3));
  M := [[1, 2], [], [3]];
  WriteLn(Length(M), ' ', Length(M[0]), ' ', Length(M[1]), ' ', M[2][0]);
  G[0] := S + 'c';
  WriteLn(G[0]);
  P := [At(1, 2), At(3, 4)];
  WriteLn(P[1].X, ' ', P[1].Y, ' ', Length(P));
  A := [9, 8];
  Show(A);
  Show(Primes);
  B := Primes;
  WriteLn(G[1], ' ', Length(A));
  WriteLn(Length(B), ' ', B[2]);
end.
