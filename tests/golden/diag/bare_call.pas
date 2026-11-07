program BareCall;

function Square(X: Integer): Integer;
begin
  Square := X * X;
end;

function Next: Integer;
begin
  Next;
  Result := 1;
end;

procedure Show(X: Integer);
begin
  WriteLn(X);
end;

procedure Loop;
begin
  Loop;
end;

begin
  Square;
  Show;
  Next;
  WriteLn(Next);
end.
