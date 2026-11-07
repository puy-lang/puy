program FuncName;

// Assigning to the function name is assigning to Result

function Square(X: Integer): Integer;
begin
  Square := X * X;
end;

function Fact(N: Int64): Int64;
begin
  if N <= 1 then
    Fact := 1
  else
    Fact := N * Fact(N - 1);
end;

function Sign(N: Integer): Integer;
begin
  Sign := 0;
  if N > 0 then
    Sign := 1;
  if N < 0 then
    Sign := -1;
end;

begin
  WriteLn(Square(7));
  WriteLn(Fact(20));
  WriteLn(Sign(-5), ' ', Sign(0), ' ', Sign(9));
end.
