program Tree;

uses
  System.SysUtils;

// Reference counted classes: a class that descends from TRefObject is freed
// when the last reference to it goes away, like a string or a dynamic array.
// A child points back to its parent with a [weak] field, which does not count
// and reads as nil once the parent is gone, so parent and child do not keep
// each other alive.

type
  TNode = class(TRefObject)
  private
    FName: string;
    [weak] FParent: TNode;
    FChildren: TArray<TNode>;
  public
    constructor Create(const AName: string);
    destructor Destroy; override;
    function Add(const AName: string): TNode;
    function Path: string;
    procedure Print(const Indent: string = '');
    property Name: string read FName;
    property Parent: TNode read FParent;
  end;

constructor TNode.Create(const AName: string);
begin
  inherited Create;
  FName := AName;
end;

destructor TNode.Destroy;
begin
  WriteLn('  freed ', FName);
  inherited;
end;

function TNode.Add(const AName: string): TNode;
begin
  Result := TNode.Create(AName);
  Result.FParent := Self;
  FChildren := FChildren + [Result];
end;

function TNode.Path: string;
begin
  if Assigned(FParent) then
    Result := FParent.Path + '/' + FName
  else
    Result := FName;
end;

procedure TNode.Print(const Indent: string);
begin
  WriteLn(Indent, FName);
  for var Child in FChildren do
    Child.Print(Indent + '  ');
end;

// The results of Add that are only used to call Add again are temporaries:
// they hold a reference until this function returns.
function BuildTree(out Leaf: TNode): TNode;
begin
  Result := TNode.Create('root');
  Leaf := Result.Add('docs').Add('readme.md');
  Result.Add('src').Add('main.pas');
end;

procedure Run;
var
  Root, Docs, Leaf: TNode;
begin
  Root := BuildTree(Leaf);
  Root.Print;
  WriteLn(Leaf.Path);

  // The root's array still holds the docs node, so it survives this.
  Docs := Leaf.Parent;
  Docs := nil;
  WriteLn('docs still alive: ', Assigned(Leaf.Parent));

  // Letting go of the root frees the whole tree except the leaf, which the
  // variable Leaf still holds. Its [weak] parent field now reads as nil.
  WriteLn('-- root := nil');
  Root := nil;
  WriteLn('leaf parent assigned: ', Assigned(Leaf.Parent));
  WriteLn(Leaf.Path);
end; // the leaf goes here

begin
  Run;
  WriteLn('done');
end.
