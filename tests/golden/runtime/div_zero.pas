program DivZero;

function Divide(A, B: Integer): Integer;
begin
  Result := A div B;
end;

procedure Run;
begin
  WriteLn('before');
  WriteLn(Divide(1, 0));
  WriteLn('after');
end;

begin
  Run;
end.
