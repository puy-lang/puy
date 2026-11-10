program OpenArrayArgs;

// Open array parameters (types.md 2.4): dynamic arrays, static arrays, constructors and other
// open arrays as value, `const` and `var` arguments.

type
  TInts = array of Integer;
  TFour = array[5..8] of Integer;
  TBox = record
    Nums: TFour;
  end;

function Sum(const A: array of Integer): Integer;
var
  I: Integer;
begin
  Result := 0;
  for I := Low(A) to High(A) do
    Result := Result + A[I];
end;

function Count(const A: array of Integer): Integer;
begin
  Result := Length(A);
end;

// A value open array is the routine's own copy
function SumTwice(A: array of Integer): Integer;
var
  I: Integer;
begin
  for I := 0 to High(A) do
    A[I] := A[I] * 2;
  Result := Sum(A);
end;

procedure Bump(var A: array of Integer);
var
  I: Integer;
begin
  for I := 0 to High(A) do
    Inc(A[I]);
end;

procedure BumpAll(var A: array of Integer);
begin
  Bump(A);
end;

procedure Join(const Parts: array of string);
var
  I: Integer;
begin
  for I := 0 to High(Parts) do
    Write(Parts[I], '|');
  WriteLn;
end;

procedure Twice(var N: Integer);
begin
  N := N * 2;
end;

var
  D: TInts;
  F: TFour;
  Box, Box2: TBox;
  Words: array of string;
begin
  D := [1, 2, 3];
  F[5] := 10; F[6] := 20; F[7] := 30; F[8] := 40;
  WriteLn(Sum(D), ' ', Sum(F), ' ', Sum([4, 5, 6]), ' ', Sum([]), ' ', Count(F));
  WriteLn(SumTwice(D), ' ', SumTwice(F), ' ', SumTwice([1, 1]));
  WriteLn(Sum(D), ' ', Sum(F));
  Bump(D);
  Bump(F);
  BumpAll(D);
  WriteLn(D[0], ' ', D[2], ' ', F[5], ' ', F[8]);
  Box.Nums := F;
  Box2 := Box;
  Bump(Box2.Nums);
  WriteLn(Box.Nums[5], ' ', Box2.Nums[5], ' ', Sum(Box2.Nums));
  Twice(D[1]);
  Twice(F[6]);
  WriteLn(D[1], ' ', F[6]);
  Words := ['a', 'b'];
  Join(Words);
  Join(['x', 'y' + 'z', '']);
  Join([]);
end.
