program WeakParentTree;

// A tree whose children refer to their parent through a `[weak]` field (objects.md 4.4): letting
// go of the root destroys the whole tree, so nothing leaks (the same tree without `[weak]` is
// `weak_parent_strong`). The children read the parent while it is alive; the root's destructor
// reads `nil` through a child's `[weak]` field once the root is being destroyed, and so do the
// children's destructors.

type
  TNode = class(TRefObject)
  public
    Name: string;
    [weak] Parent: TNode;
    Children: TArray<TNode>;
    constructor Create(const AName: string);
    destructor Destroy; override;
    procedure Add(Child: TNode);
    function Path: string;
  end;

constructor TNode.Create(const AName: string);
begin
  Name := AName;
end;

destructor TNode.Destroy;
begin
  if Length(Children) > 0 then
    WriteLn('destroy ', Name, '; parent of first child is nil: ', Children[0].Parent = nil)
  else
    WriteLn('destroy ', Name, '; parent is nil: ', Parent = nil);
  inherited;
end;

procedure TNode.Add(Child: TNode);
begin
  SetLength(Children, Length(Children) + 1);
  Children[High(Children)] := Child;
  Child.Parent := Self;
end;

function TNode.Path: string;
begin
  if Parent = nil then
    Result := Name
  else
    Result := Parent.Path + '/' + Name;
end;

procedure Build;
var
  Root, Mid: TNode;
begin
  Root := TNode.Create('root');
  Mid := TNode.Create('mid');
  Root.Add(Mid);
  Mid.Add(TNode.Create('leaf'));
  Root.Add(TNode.Create('other'));
  WriteLn(Mid.Children[0].Path);
  WriteLn(Root.Children[1].Path);
  WriteLn('root count: ', Root.RefCount);
end;

begin
  Build;
  WriteLn('end');
end.
