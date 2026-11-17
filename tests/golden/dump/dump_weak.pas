program DumpWeak;

// The dumps of `[weak]` (objects.md 4.4, 15.1-15.3): ` weak` after the type of a field, a
// `class var`, a record field, a global and a local in the hir, and the `weak` expression around
// each read; in the ir, a read is an uncounted load followed by `LoadWeak`, and a write is the
// uncounted instruction (`ObjSet`, `FieldSet`, `StoreGlobal`, `Move`) from a value read without
// counting; in the bytecode, the slots of `[weak]` fields are not counted (`refs=`).

type
  TNode = class(TRefObject)
  public
    [weak] Parent: TNode;
    Next: TNode;
    class var [weak] Last: TNode;
  end;
  TLink = record
    [weak] Target: TNode;
  end;

var
  [weak] Current: TNode;

procedure Link(N: TNode);
var
  L: TLink;
  [weak] W: TNode;
begin
  L := Default(TLink);
  N.Parent := N.Next;
  L.Target := N;
  TNode.Last := N;
  Current := N;
  W := Current;
  if W.Parent <> L.Target then
    WriteLn(TNode.Last <> nil);
end;

var
  N: TNode;
begin
  N := TNode.Create;
  Link(N);
end.
