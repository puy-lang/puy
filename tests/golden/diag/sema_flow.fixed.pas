program SemaFlow;
var N: Integer;
function F1: Integer;
begin
end;
function F2(X: Integer): Integer;
begin
  if X > 0 then
    Result := 1;
end;
function F3(X: Integer): Integer;
begin
  if X > 0 then
    Exit;
  Result := 1;
end;
function F4(X: Integer): Integer;
begin
  if X > 0 then
    Exit(2);
  Result := 1;
end;
procedure O1(out A: Integer);
begin
  A := 0;
  WriteLn(A);
  A := 1;
end;
procedure O2(out A: Integer; X: Integer);
begin
  if X > 0 then
    A := 1;
end;
procedure O3(out A: Integer; X: Integer);
begin
  if X > 0 then
    Exit;
  A := 1;
end;
procedure O4(out A: string);
begin
  A := '';
  A := A + 'x';
end;
procedure O5(out A: Integer);
begin
  A := 0;
  Inc(A);
end;
procedure O6(out A: Integer);
begin
  O1(A);
end;
function F5: Integer;
begin
  Result := 0;
  Result := Result + 1;
end;
function F6: Integer;
begin
  Result := 0;
  Inc(Result);
end;
procedure Loop;
var I, J: Integer;
begin
  for I := 1 to 3 do
    J := I;
  WriteLn(I, J);
  for var K := 1 to 3 do
    WriteLn(K);
  WriteLn(K);
end;
begin
  N := 0;
  O1(N);
  O2(N, 1);
  O3(N, 1);
  O5(N);
  O6(N);
  WriteLn(F1, F2(1), F3(1), F4(1), F5, F6);
  Loop;
end.
