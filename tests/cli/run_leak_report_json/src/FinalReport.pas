unit FinalReport;

interface

implementation

finalization
  System.ReportMemoryLeaksOnShutdown := True;
  raise Exception.Create('finalization');
end.
