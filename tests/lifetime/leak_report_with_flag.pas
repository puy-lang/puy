// A runner with leak checking enabled counts a self-cycle once when the flag is also true.
program LeakReportWithFlag;

type
  TNode = class(TRefObject)
    Next: TNode;
  end;

var
  Node: TNode;
begin
  ReportMemoryLeaksOnShutdown := True;
  Node := TNode.Create;
  Node.Next := Node;
end.
