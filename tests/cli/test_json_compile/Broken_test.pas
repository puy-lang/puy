// A compile error leaves the tests array empty and puts the diagnostics in the one report.
unit Broken_test;

interface

implementation

[Test]
procedure Broken;
begin
  WriteLn(Unknown);
end;

end.
