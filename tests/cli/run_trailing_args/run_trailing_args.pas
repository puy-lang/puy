program RunTrailingArgs;
var
  I: Integer;
begin
  WriteLn(ParamCount);
  I := 1;
  while I <= ParamCount do
  begin
    WriteLn(ParamStr(I));
    I := I + 1;
  end;
end.
