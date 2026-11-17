program NestedValueGroups;

// A nested routine made a value captures only what its group of routines uses (objects.md 9.7,
// 14.9): `B` uses `Saved` and `A` does not, so they are in different groups, and storing the
// value of `A` in `Saved` while returning the value of `B` makes no cycle. Everything is freed
// once the last value is let go of, including the instance in a captured variable.

type
  TItem = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

function Make: TProc;
var
  Saved: TProc;
  Item: TItem;
  Count: Integer;

  procedure A;
  begin
    WriteLn('A ', Item.Name);
  end;

  procedure B;
  begin
    Inc(Count);
    WriteLn('B ', Count);
    Saved();
  end;

begin
  Item := TItem.Create('item');
  Count := 0;
  Saved := A;
  Result := B;
end;

procedure Run;
var
  P: TProc;
begin
  P := Make();
  P();
  P();
  WriteLn('drop');
  P := nil;
  WriteLn('dropped');
end;

begin
  Run;
  WriteLn('end');
end.
