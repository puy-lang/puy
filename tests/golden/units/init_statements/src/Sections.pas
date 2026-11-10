unit Sections;

interface

var
  Total: Integer;

implementation

var
  I: Integer;

initialization
  for I := 1 to 3 do
    Total := Total + I;
  var Text := 'init Sections';
  WriteLn(Text, ' ', Total);
  if Total > 0 then
    Exit;
  WriteLn('not reached');
finalization
  var Text := 'fini Sections';
  WriteLn(Text);
end.
