// The default flag leaves leaks unreported even when the System part is loaded.
program LeakReportDefault;

begin
  WriteLn(ReportMemoryLeaksOnShutdown);
  TObject.Create;
end.
