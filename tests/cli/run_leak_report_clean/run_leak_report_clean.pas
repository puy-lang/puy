// A live global, a cached string constant and a freed object leave no blocks to report.
program LeakReportClean;

var
  Name: string;
  Obj: TObject;
begin
  ReportMemoryLeaksOnShutdown := True;
  Name := 'clean';
  WriteLn(Name);
  Obj := TObject.Create;
  Obj.Free;
end.
