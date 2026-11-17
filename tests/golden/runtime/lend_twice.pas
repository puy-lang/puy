program LendTwice;

// Passing the same element to two `var` parameters, found at run time, is an `EAliasError` when
// the second one is taken out (types.md chapter 3); the first one is given back unchanged.
// Two fields of the same record are two locations; an element and a field inside it overlap.

type
  TPair = record
    X, Y: Integer;
  end;
  TArr = array[1..3] of Integer;

var
  K: array of Integer;
  S: TArr;
  P: array of TPair;
  I, J: Integer;

procedure Swap(var A, B: Integer);
var
  T: Integer;
begin
  T := A;
  A := B;
  B := T;
end;

procedure Touch(var R: TPair; var N: Integer);
begin
  N := R.X;
end;

begin
  K := [1, 2, 3];
  I := 2;
  J := 2;
  try
    Swap(K[I], K[J]);
  except
    on E: EAliasError do
      WriteLn(E.Message);
  end;
  WriteLn(K[0], K[1], K[2]);
  S[1] := 1;
  S[2] := 2;
  S[3] := 3;
  I := 3;
  J := 3;
  try
    Swap(S[I], S[J]);
  except
    on E: EAliasError do
      WriteLn(E.Message);
  end;
  WriteLn(S[1], S[2], S[3]);
  SetLength(P, 2);
  P[1].X := 1;
  P[1].Y := 2;
  I := 1;
  J := 1;
  try
    Swap(P[I].X, P[J].X);
  except
    on E: EAliasError do
      WriteLn(E.Message);
  end;
  try
    Touch(P[I], P[J].X);
  except
    on E: EAliasError do
      WriteLn(E.Message);
  end;
  Swap(P[I].X, P[J].Y);
  WriteLn(P[1].X, P[1].Y);
  J := 0;
  Swap(K[I], K[J]);
  WriteLn(K[0], K[1], K[2]);
end.
