program RefcountSelfLives;

// A method whose call let go of the last reference to its instance (`Self`) keeps the instance
// until it returns: the receiver read from a global is counted for the call (objects.md 4.1).

type
  TItem = class(TRefObject)
  public
    Name: string;
    procedure Drop;
    destructor Destroy; override;
  end;

var
  Current: TItem;

procedure TItem.Drop;
begin
  Current := nil;
  WriteLn('after the last reference: ', Name);
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited Destroy;
end;

procedure Run;
begin
  Current := TItem.Create;
  Current.Name := 'item';
  Current.Drop;
  WriteLn('returned');
end;

begin
  Run;
  WriteLn('end');
end.
