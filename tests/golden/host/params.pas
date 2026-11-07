program Params;

var
  I: Integer;
begin
  WriteLn(ParamCount);
  for I := 0 to ParamCount do
    WriteLn(I, ': ', ParamStr(I));
  WriteLn('[', ParamStr(ParamCount + 1), ']');
end.
