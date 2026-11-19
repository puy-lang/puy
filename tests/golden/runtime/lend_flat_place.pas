program LendFlatPlace;

// A record or static array kept flat in an array, a record or an instance is lent to a `var`
// parameter where it is (types.md chapter 3, bytecode.md 2.8): touching it through another path
// during the call is an `EAliasError`; when its array is replaced or shrinks during the call, the
// values the callee wrote move out with it and go back to the place found again after the call
// (or raise `ERangeError` when it is gone); a static array passed to a `var` open array
// parameter is taken out into a block and goes back too, also when the call raises or the
// callee keeps a copy of the block.

uses
  System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
    Name: string;
  end;
  TTriple = array[0..2] of Integer;
  TShape = record
    P: TPoint;
    T: TTriple;
  end;

var
  A: array of TPoint;
  S: array of TShape;
  Other: array of TPoint;
  Kept: TTriple;
  K: Integer;

procedure Touch(var P: TPoint);
begin
  P.X := 1;
  try
    WriteLn(A[0].Y);
  except
    on E: EAliasError do
      WriteLn('read: ', E.Message);
  end;
  try
    A[0].Name := 'x';
  except
    on E: EAliasError do
      WriteLn('write: ', E.Message);
  end;
  try
    WriteLn(Length(Copy(A, 0, 1)));
  except
    on E: EAliasError do
      WriteLn('copy: ', E.Message);
  end;
end;

procedure Twice(var P, Q: TPoint);
begin
  WriteLn('not here');
end;

procedure Inner(var X: Integer);
begin
  X := 5;
end;

procedure Replace(var P: TPoint);
begin
  P.X := 2;
  P.Name := P.Name + '+';
  A := Other;
  Other := nil;
  P.Y := 3;
end;

procedure Shrink(var P: TPoint);
begin
  P.X := 4;
  SetLength(A, 0);
end;

procedure Grow(var P: TPoint);
begin
  P.X := 6;
  SetLength(A, 0);
  SetLength(A, 2);
  P.Name := 'back';
end;

procedure FillOpen(var V: array of Integer);
var
  I: Integer;
begin
  for I := 0 to High(V) do
    V[I] := V[I] + 10;
  try
    WriteLn(S[0].T[1]);
  except
    on E: EAliasError do
      WriteLn('open: ', E.Message);
  end;
  raise Exception.Create('raised');
end;

procedure Keep(var V: TTriple);
begin
  V[0] := 7;
  Kept := V;
  V[1] := 8;
end;

procedure KeepFail(var V: TTriple);
begin
  V[2] := 9;
  Kept := V;
  raise Exception.Create('kept');
end;

procedure ShapeOf(var Sh: TShape);
begin
  Inner(Sh.P.Y);
  Sh.T[2] := Sh.P.Y;
end;

begin
  SetLength(A, 2);
  A[0].Name := 'a';
  Touch(A[0]);
  WriteLn(A[0].X, ' ', A[0].Name);
  K := 1;
  try
    Twice(A[1], A[K]);
  except
    on E: EAliasError do
      WriteLn('twice: ', E.Message);
  end;
  SetLength(Other, 3);
  Other[1].Name := 'other';
  Replace(A[1]);
  WriteLn(A[1].X, ' ', A[1].Y, ' ', A[1].Name);
  try
    Shrink(A[1]);
  except
    on E: ERangeError do
      WriteLn('shrink: ', E.Message);
  end;
  WriteLn(Length(A));
  SetLength(A, 1);
  Grow(A[0]);
  WriteLn(Length(A), ' ', A[0].X, ' ', A[0].Name);

  SetLength(S, 1);
  S[0].T[1] := 1;
  try
    FillOpen(S[0].T);
  except
    on E: Exception do
      WriteLn(E.Message);
  end;
  WriteLn(S[0].T[0], ' ', S[0].T[1], ' ', S[0].T[2]);
  ShapeOf(S[0]);
  WriteLn(S[0].P.Y, ' ', S[0].T[2]);
  // The block the static array was taken out into, shared by the callee
  Keep(S[0].T);
  WriteLn(S[0].T[0], ' ', S[0].T[1], ' ', Kept[0], ' ', Kept[1]);
  try
    KeepFail(S[0].T);
  except
    on E: Exception do
      WriteLn(E.Message);
  end;
  WriteLn(S[0].T[2], ' ', Kept[2]);
end.
