program SemaForward;
var N: Integer; S: string;
function Twice(X: Integer): Integer; forward;
function Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;
procedure NoBody; forward;
procedure Mismatch(A: Integer); forward;
procedure Mismatch(A: string);
begin
end;
function Mis2(A: Integer): Integer; forward;
function Mis2(A: Integer): string;
begin
  Result := '';
end;
procedure Mis3(const A: Integer); forward;
procedure Mis3(var A: Integer);
begin
end;
procedure Mis4(A, B: Integer); forward;
procedure Mis4(A: Integer; B: Integer);
begin
end;
procedure Mis5(A: Integer); forward;
procedure Mis5(B: Integer);
begin
end;
procedure Mis6(A: Integer); forward;
procedure Mis6(A: Integer; B: Integer);
begin
end;
function Rec(X: Integer): Integer;
begin
  if X = 0 then Result := 0 else Result := Rec + 1;
end;
function Rec2: Integer;
begin
  Result := Rec2 + 1;
end;
begin
  N := Twice(2);
  S := 'a';
  WriteLn(N, S, Rec(1), Rec2);
end.
