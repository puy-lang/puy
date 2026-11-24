// The qualified System flag still names the global when a program variable shadows it.
program LeakReportScope;

var
  ReportMemoryLeaksOnShutdown: Boolean = True;
begin
  WriteLn(ReportMemoryLeaksOnShutdown, ' ', System.ReportMemoryLeaksOnShutdown);
  System.ReportMemoryLeaksOnShutdown := True;
  ReportMemoryLeaksOnShutdown := False;
  WriteLn(ReportMemoryLeaksOnShutdown, ' ', System.ReportMemoryLeaksOnShutdown);
end.
