// Only the flag's final value matters, even when a block was leaked while it was true.
program LeakReportDisabled;

begin
  ReportMemoryLeaksOnShutdown := True;
  TObject.Create;
  System.ReportMemoryLeaksOnShutdown := False;
end.
