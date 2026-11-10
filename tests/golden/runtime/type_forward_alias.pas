program TypeForwardAlias;

// A type may name a type declared later in the same `type` section (types.md 1.1), including
// an alias of an alias; Delphi does not allow this.

type
  TName = TText;
  TText = string;
  TItem = record
    Name: TName;
    Next: TPos;
  end;
  TPos = array[0..1] of Integer;

var
  Item: TItem;
begin
  Item := Default(TItem);
  Item.Name := 'n';
  Item.Next[1] := 2;
  WriteLn(Item.Name, ' ', Item.Next[1], ' ', Length(Item.Next));
end.
