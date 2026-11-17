program PeepholeCalls;

uses System.SysUtils;

// What reads see where codegen's rules move them past calls or load constants once before a
// loop (design/vm.md 3.15): a variable read before a call gives the value it had then, also when
// the call writes it through a nested routine or a var parameter; a call's result is read where
// the call leaves it; and constants loaded before a loop give the same values in every kind of
// loop, also around exceptions, Break and Continue

var
  Calls: Integer;

function Twice(N: Integer): Integer;
begin
  Inc(Calls);
  Result := N * 2;
end;

function Same(N: Integer): Integer;
begin
  if N < 0 then
    Exit(-N);
  Result := N;
end;

function BumpAndGet(var V: Integer): Integer;
begin
  V := V + 100;
  Result := 1;
end;

procedure Nested;
var
  S, I: Integer;
  A: array of Integer;

  function Peek: Integer;
  begin
    S := S + 1000;
    Result := 1;
  end;

begin
  S := 1;
  S := S + Peek;
  WriteLn('nested: ', S);
  S := 0;
  for I := 1 to 3 do
    S := (S + Peek) and $FFFFFF;
  WriteLn('nested loop: ', S);
  SetLength(A, 2000);
  S := 5;
  A[S] := Peek;
  WriteLn('nested index: ', A[5], ' ', A[1005], ' ', S);
end;

procedure ByRef;
var
  X: Integer;
  A: array of Integer;
begin
  X := 5;
  X := X + BumpAndGet(X);
  WriteLn('var arg: ', X);
  SetLength(A, 200);
  X := 5;
  A[X] := BumpAndGet(X);
  WriteLn('var index: ', A[5], ' ', A[105], ' ', X);
end;

procedure Results;
var
  I, S: Integer;
  A: array of Integer;
begin
  WriteLn('results: ', Twice(3) + Twice(4) * Twice(5), ' ', Twice(Twice(2)), ' ', Same(-7));
  SetLength(A, 10);
  A[Twice(1)] := Twice(3);
  WriteLn('index: ', A[2]);
  S := 0;
  for I := 1 to 5 do
    if Twice(I) > 4 then
      S := S + Twice(I) - Same(I);
  WriteLn('branch: ', S, ' calls ', Calls);
end;

function WhileFirst(N: Integer): Integer;
begin
  while N < 3000000 do
    N := N + 1000000;
  Result := N;
end;

procedure Loops;
var
  I, J, K, S: Integer;
  F: Double;
begin
  S := 0;
  F := 0.0;
  for I := 1 to 3 do
  begin
    S := (S + 100000) and $FFFFFF;
    for J := 1 to 2 do
    begin
      S := S + 70000;
      F := F + 2.5 * J;
    end;
  end;
  WriteLn('for: ', S, ' ', F:0:1);
  K := 0;
  repeat
    K := K + 300;
    if K = 600 then
      Continue;
    if K > 1500 then
      Break;
    S := S xor 123456;
  until K >= 3000;
  WriteLn('repeat: ', K, ' ', S);
  WriteLn('while first: ', WhileFirst(5), ' ', WhileFirst(5000000));
  S := 0;
  for I := 1 to 3 do
  begin
    try
      if I = 2 then
        raise Exception.Create('two');
      S := S + 1000;
    except
      on E: Exception do
        S := S + 5000;
    end;
    S := S + 200;
  end;
  WriteLn('try: ', S);
  S := 0;
  for I := 5 downto 6 do
    S := S + 999999;
  WriteLn('empty: ', S);
  S := 0;
  for I := 1 to 3 do
    S := S + Twice(1000);
  WriteLn('argument: ', S);
  // A loop whose body leaves a try..finally with Continue and Break, and a loop inside an except
  // block
  S := 0;
  for I := 1 to 4 do
  begin
    try
      S := (S + 100000) and $FFFFFF;
      if I = 2 then
        Continue;
      if I = 4 then
        Break;
      S := S xor 123456;
    finally
      S := S + 1000;
    end;
  end;
  try
    raise Exception.Create('x');
  except
    K := 0;
    while K < 5000 do
      K := K + 1000;
    S := S + K;
  end;
  WriteLn('finally: ', S);
end;

begin
  Nested;
  ByRef;
  Results;
  Loops;
end.
