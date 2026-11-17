program InterfaceWeak;

// `[weak]` on an interface variable and field (objects.md 4.4, 7.4): writing it does not count,
// reading it gives `nil` once the instance is destroyed, and a strong reference read from it
// keeps the instance alive to the end of the routine.

type
  INode = interface
    function Name: string;
  end;

  TNode = class(TRefObject, INode)
  private
    FName: string;
  public
    [weak] Peer: INode;
    constructor Create(const AName: string);
    destructor Destroy; override;
    function Name: string;
  end;

constructor TNode.Create(const AName: string);
begin
  FName := AName;
end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', FName);
  inherited Destroy;
end;

function TNode.Name: string;
begin
  Result := FName;
end;

procedure Weak;
var
  [weak] W: INode;
  S: INode;
begin
  S := TNode.Create('a');
  W := S;
  S := nil;
  WriteLn('after release: ', W = nil);
end;

procedure ReadKeeps;
var
  [weak] W: INode;
  S: INode;
begin
  S := TNode.Create('b');
  W := S;
  WriteLn('read: ', W.Name);
  S := nil;
  WriteLn('kept by the read: ', W <> nil);
end;

procedure Field;
var
  A, B: TNode;
begin
  A := TNode.Create('x');
  B := TNode.Create('y');
  A.Peer := B;
  B.Peer := A;
  WriteLn('peers: ', A.Peer.Name, ' ', B.Peer.Name);
end;

begin
  Weak;
  ReadKeeps;
  Field;
  WriteLn('end');
end.
