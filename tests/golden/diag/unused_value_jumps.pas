program UnusedValueJumps;

// W0002 with `Break`, `Continue`, `Exit` through `finally`, and `case`.

procedure Cleanup;
begin
end;

// Not reported: the value reaches the code after the loop through `Break`
procedure ByBreak(N: Integer);
var
  X: Integer;
begin
  X := 0;
  while N > 0 do
  begin
    X := 1;
    if N = 2 then
      Break;
    X := 2;
    Dec(N);
  end;
  WriteLn(X);
end;

// Not reported: the next iteration reads the value through `Continue`
procedure ByContinue(N: Integer);
var
  X: Integer;
begin
  X := 0;
  for var I := 1 to N do
  begin
    WriteLn(X);
    X := I;
    if I mod 2 = 0 then
      Continue;
    X := -I;
  end;
end;

// Not reported: `finally` reads the value on the way out through `Exit`
procedure ByExit(Flag: Boolean);
var
  Y: Integer = 0;
begin
  try
    Y := 1;
    if Flag then
      Exit;
    Y := 2;
  finally
    WriteLn(Y);
  end;
end;

// Reported: every arm of the `case` overwrites the value
procedure AllArms(N: Integer);
var
  S: string;
begin
  S := '';
  case N of
    1: S := 'one';
    2: S := 'two';
  else
    S := 'many';
  end;
  WriteLn(S);
end;

// Reported: before the `try`, the value reaches the `finally` block past `N := 2` only by an
// exception, which leaves the routine without reading it
procedure BeforeFinally;
var
  N: Integer;
begin
  N := 1;
  try
    N := 2;
  finally
    Cleanup;
  end;
  WriteLn(N);
end;

begin
  ByBreak(3);
  ByContinue(3);
  ByExit(True);
  AllArms(2);
  BeforeFinally;
end.
