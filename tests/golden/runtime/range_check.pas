program RangeCheck;

var
  B: Byte;
  I: Integer;
begin
  I := 200;
  B := I;
  WriteLn(B);
  I := I + 100;
  B := I;
  WriteLn(B);
end.
