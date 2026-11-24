// Halt and leaks are both reported, with different outcomes and independent reproduction commands.
unit Second_test;

interface

implementation

type
  TNode = class(TInterfacedObject)
    Next: TNode;
  end;

[Test]
procedure Stops;
begin
  Halt(3);
end;

[Test]
procedure Leaks;
var
  Node: TNode;
  Unused: Integer;
begin
  Node := TNode.Create;
  Node.Next := Node;
end;

[Test]
procedure FilteredOut;
begin
  Assert(False);
end;

end.
