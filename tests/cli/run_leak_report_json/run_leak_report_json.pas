// JSON reports the leak after both the program and finalization exceptions.
program LeakReportJson;

uses FinalReport;

begin
  TObject.Create;
  raise Exception.Create('main');
end.
