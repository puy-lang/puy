program MethodRefNotCounted;

// A method reference does not count its instance (objects.md 9.3, §5.2): an instance of a
// reference counted class is destroyed when its last variable is set to `nil`, though a method
// reference to it remains (and is no longer assigned). While a call through a method reference
// runs, the VM holds the instance, so a handler that drops the last reference to its own instance
// finishes first and the instance is destroyed when the call returns (14.9).

type
  TEvent = procedure of object;

  TNode = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
    procedure Hello;
    procedure DropSelf;
  end;

var
  Keeper: TNode;

constructor TNode.Create(const AName: string);
begin
  Name := AName;
end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

procedure TNode.Hello;
begin
  WriteLn('hello from ', Name);
end;

procedure TNode.DropSelf;
begin
  WriteLn('dropping ', Name);
  Keeper := nil;
  WriteLn('dropped, ', Name, ' still runs');
end;

procedure Run;
var
  N: TNode;
  E: TEvent;
begin
  N := TNode.Create('a');
  E := N.Hello;
  E;
  WriteLn('set to nil');
  N := nil;
  WriteLn('assigned: ', Assigned(E));
  N := TNode.Create('b');
  Keeper := N;
  N := nil;
  E := Keeper.DropSelf;
  E;
  WriteLn('after the call: ', Assigned(E));
end;

begin
  Run;
  WriteLn('end');
end.
