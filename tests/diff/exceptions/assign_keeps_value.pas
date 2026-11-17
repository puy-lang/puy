program AssignKeepsValue;

// An assignment whose right-hand side raises leaves the variable as it was before the statement,
// also when the value is computed in several steps (an overflow after the addition, a call after
// the first operand of `and`); the handler and the `finally` block see the old value.

uses System.SysUtils;

function Fails(X: Integer): Boolean;
begin
  Result := False;
  if X > 0 then
    raise Exception.Create('fails');
end;

function Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

procedure Overflow;
var
  X, Z: Integer;
begin
  X := MaxInt;
  Z := 7;
  try
    Z := X + 1;
  except
    on E: Exception do
      WriteLn('overflow: Z = ', Z);
  end;
  Z := 8;
  try
    // Twice returns; the multiplication after the call overflows
    Z := Twice(X div 4) * 4;
  except
    on E: Exception do
      WriteLn('call: Z = ', Z);
  end;
end;

procedure ShortCircuit;
var
  X: Integer;
  B: Boolean;
begin
  X := 5;
  B := False;
  try
    B := (X > 0) and Fails(X);
  except
    on E: Exception do
      WriteLn('and: B = ', B);
  end;
end;

function InFinally(X: Integer): Integer;
var
  Z: Integer;
begin
  Result := 1;
  Z := 3;
  try
    try
      Z := X * X;
      Result := X * X * X;
    finally
      WriteLn('finally: Z = ', Z, ', Result = ', Result);
    end;
  except
    on E: Exception do
      WriteLn('caught');
  end;
end;

begin
  Overflow;
  ShortCircuit;
  WriteLn(InFinally(2000));
end.
