program LendPutBack;

// The value lent to a `var` / `out` parameter goes back when the call ends, also when it ends
// with an exception (types.md chapter 3): what the callee wrote stays. When the location is
// gone (the array shrank or became nil), the value is let go and the call raises
// `ERangeError` for the first such location after giving back the others.

type
  TRec = record
    N: Integer;
  end;

var
  A: array of string;
  K: array of Integer;
  R: array of TRec;

procedure Fail(var S: string; var N: Integer);
begin
  S := S + '!';
  N := 9;
  raise Exception.Create('failed');
end;

procedure Clear(out S: string);
begin
  S := 'out';
end;

procedure Drop(var X: string; var Y: Integer; var Z: string);
begin
  SetLength(A, 1);
  X := 'lost';
  Y := 5;
  Z := 'kept';
end;

procedure Nil1(var X: Integer);
begin
  R := nil;
  X := 7;
end;

begin
  SetLength(A, 3);
  A[0] := 'a';
  A[2] := 'c';
  SetLength(K, 1);
  try
    Fail(A[0], K[0]);
  except
    on E: Exception do
      WriteLn(E.Message, ' ', A[0], ' ', K[0]);
  end;
  Clear(A[0]);
  WriteLn(A[0]);
  try
    Drop(A[2], K[0], A[0]);
  except
    on E: ERangeError do
      WriteLn(E.Message);
  end;
  WriteLn(Length(A), ' ', A[0], ' ', K[0]);
  SetLength(R, 1);
  try
    Nil1(R[0].N);
  except
    on E: ERangeError do
      WriteLn(E.Message);
  end;
  WriteLn(Length(R));
end.
