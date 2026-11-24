// Shutdown reports both a reference cycle and a missed Free without changing ExitCode.
program LeakReport;

type
  TNode = class(TRefObject)
    Next: TNode;
  end;

var
  A, B: TNode;
begin
  ReportMemoryLeaksOnShutdown := True;
  A := TNode.Create;
  B := TNode.Create;
  A.Next := B;
  B.Next := A;
  TObject.Create;
  ExitCode := 7;
end.
