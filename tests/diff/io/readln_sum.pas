program ReadLnSum;

var
  N, I, X, Sum: Integer;
  Line: string;
begin
  ReadLn(N);
  Sum := 0;
  for I := 1 to N do
  begin
    ReadLn(X);
    Sum := Sum + X;
  end;
  WriteLn('sum=', Sum);
  ReadLn(Line);
  WriteLn('line=', Line);
end.
