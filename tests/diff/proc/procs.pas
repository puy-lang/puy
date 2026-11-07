program Procs;

procedure Swap(var X, Y: Integer);
var
  T: Integer;
begin
  T := X;
  X := Y;
  Y := T;
end;

function Fact(N: Integer): Int64;
begin
  Result := 1;
  if N > 1 then
    Result := N * Fact(N - 1);
end;

function Fib(N: Integer): Integer;
begin
  if N < 2 then
    Exit(N);
  Result := Fib(N - 1) + Fib(N - 2);
end;

procedure DivMod(A, B: Integer; out Q, R: Integer);
begin
  Q := A div B;
  R := A mod B;
end;

function Describe(const S: string; Count: Integer): string;
begin
  Result := S + '=' + Chr(Ord('0') + Count);
end;

procedure Greet;
begin
  WriteLn('hello from Greet');
end;

var
  A, B, Q, R: Integer;
begin
  A := 1;
  B := 2;
  Swap(A, B);
  WriteLn(A, ' ', B);
  WriteLn(Fact(20));
  WriteLn(Fib(15));
  DivMod(47, 6, Q, R);
  WriteLn(Q, ' ', R);
  WriteLn(Describe('n', 7));
  Greet;
end.
