program UnusedAndUninit;

procedure Show(Flag: Boolean);
var
  Value, Unused: Integer;
begin
  if Flag then
    Value := 1;
  WriteLn(Value);
end;

begin
  Show(True);
end.
