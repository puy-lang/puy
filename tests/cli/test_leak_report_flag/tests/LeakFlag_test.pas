// A test that enables the shutdown flag reports one leak in the test result.
unit LeakFlag_test;

interface

implementation

[Test]
procedure Leaks;
begin
  ReportMemoryLeaksOnShutdown := True;
  TObject.Create;
end;

end.
