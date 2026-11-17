program WeakReads;

// Reading a `[weak]` reference gives a counted value (objects.md 4.4): a method called through
// it keeps the instance alive even when the call lets go of its last other reference, and the
// value read lives until the routine ends (design/lifetime.md 3.4). A `[weak]` variable written
// from a variable is not counted, so letting go of that variable destroys the instance; reading
// a `[weak]` variable into another then writes `nil`. A `[weak]` global reads `nil` once the
// instance is gone, and so do a `[weak]` record field and a `[weak]` `class var`. A `[weak]`
// loop variable of `for..in` takes the elements without counting them.

type
  TNode = class(TRefObject)
  public
    Name: string;
    [weak] Parent: TNode;
    constructor Create(const AName: string);
    destructor Destroy; override;
    procedure Drop;
    class var [weak] Last: TNode;
  end;
  TLink = record
    [weak] Target: TNode;
  end;

var
  Holder: TNode;
  [weak] Seen: TNode;

constructor TNode.Create(const AName: string);
begin
  Name := AName;
end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

procedure TNode.Drop;
begin
  Holder := nil;
  WriteLn('dropped; ', Name, ' still alive');
end;

procedure CallThroughWeak;
var
  Child: TNode;
begin
  Holder := TNode.Create('parent');
  Child := TNode.Create('child');
  Child.Parent := Holder;
  if Child.Parent <> nil then
    Child.Parent.Drop;
  WriteLn('after the call');
end;

procedure WeakToWeak;
var
  N: TNode;
  [weak] A, B: TNode;
begin
  N := TNode.Create('n');
  A := N;
  Seen := N;
  N := nil;
  B := A;
  WriteLn('B is nil: ', B = nil);
end;

procedure ForInWeak;
var
  Items: TArray<TNode>;
  [weak] Item: TNode;
begin
  Items := [TNode.Create('first'), TNode.Create('second')];
  for Item in Items do
    WriteLn(Item.Name);
end;

procedure RecordAndClassVar;
var
  L: TLink;
  N: TNode;
begin
  N := TNode.Create('linked');
  L := Default(TLink);
  L.Target := N;
  TNode.Last := N;
  N := nil;
  WriteLn('target is nil: ', L.Target = nil, ', last is nil: ', TNode.Last = nil);
end;

procedure ReadSeen;
begin
  WriteLn('Seen is nil: ', Seen = nil);
end;

begin
  CallThroughWeak;
  WeakToWeak;
  ReadSeen;
  ForInWeak;
  RecordAndClassVar;
  WriteLn('end');
end.
