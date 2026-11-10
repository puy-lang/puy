unit Bad;

interface

var
  Values: array of Integer;

implementation

initialization
  WriteLn('init Bad');
  SetLength(Values, 2);
  Values[2] := 1;
finalization
  WriteLn('fini Bad');
end.
