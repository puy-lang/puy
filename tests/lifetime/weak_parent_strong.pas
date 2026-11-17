program WeakParentStrong;

// The tree of `weak_parent_tree` with a strong `Parent` field: each child and its parent refer
// to each other, so letting go of the root destroys nothing and the instances leak (objects.md
// 4.4).

type
  TNode = class(TRefObject)
  public
    Name: string;
    Parent: TNode;
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
