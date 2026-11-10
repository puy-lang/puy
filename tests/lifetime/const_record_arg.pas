program ConstRecordArg;

// A global record passed to a `const` parameter is copied into a temporary (counted) and lent
// from there (lifetime.md 3.3), so the callee changing the global does not change the argument.
// In Delphi the argument is not counted and this reads freed memory.

type
  TItem = record
    Name: string;
    Count: Integer;
  end;

var
  Global: TItem;

procedure Look(const Item: TItem);
begin
  Global.Count := 1000;
  Global.Name := 'changed';
  WriteLn(Item.Name, ' ', Item.Count);
end;

begin
  Global := Default(TItem);
  Global.Name := 'g';
  Global.Count := 3;
  Look(Global);
  WriteLn(Global.Name, ' ', Global.Count);
end.
