program RefcountDepthFirst;

// The locals of a routine are let go in declaration order, and an instance whose last reference
// goes is destroyed whole, its fields included, before the next local is let go (depth first;
// objects.md 3.2, design/objects.md 3.8 B): `A`, then `C` (a field of `A`), then `B`.

type
  TNode = class(TRefObject)
  public
    Name: string;
    Child: TNode;
    constructor Create(const AName: string; WithChild: Boolean);
    destructor Destroy; override;
  end;

constructor TNode.Create(const AName: string; WithChild: Boolean);
begin
  Name := AName;
  if WithChild then
    Child := TNode.Create('C', False);
end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited Destroy;
end;

procedure Run;
var
  A, B: TNode;
begin
  A := TNode.Create('A', True);
  B := TNode.Create('B', False);
  WriteLn(A.Name, A.Child.Name, B.Name);
end;

begin
  Run;
  WriteLn('end');
end.
