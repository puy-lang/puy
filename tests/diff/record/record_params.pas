program RecordParams;

// Records and static arrays as value, const, var and out parameters; a field of a var
// parameter passed on to another var parameter (a cell whose root is a cell; lifetime.md 3.7);
// local types and aliases (types.md 1.1).

type
  TText = string;
  TName = TText;
  TItem = record
    Name: TName;
    Count: Integer;
  end;
  TBag = record
    Items: array[Byte] of TItem;
    Size: Integer;
  end;

var
  Global: TItem;

procedure Grow(var N: Integer);
begin
  N := N * 2;
end;

procedure Tag(var S: TText);
begin
  S := '[' + S + ']';
end;

procedure Touch(var Item: TItem);
begin
  Item.Count := Item.Count + 1;
  Grow(Item.Count);
  Tag(Item.Name);
end;

procedure Reset(out Item: TItem);
begin
  Item := Default(TItem);
  Item.Name := 'reset';
end;

procedure Keep(Item: TItem);
begin
  Item.Name := 'local';
  Touch(Item);
  WriteLn('keep: ', Item.Name, ' ', Item.Count);
end;

procedure Add(var Bag: TBag; const Name: string);
begin
  Bag.Items[Bag.Size].Name := Name;
  Bag.Items[Bag.Size].Count := Bag.Size;
  Touch(Bag.Items[Bag.Size]);
  Inc(Bag.Size);
end;

procedure Local;
type
  TPair = array[1..2] of Integer;
var
  P: TPair;
begin
  P := Default(TPair);
  P[1] := 1;
  P[2] := 2;
  Grow(P[2]);
  WriteLn('local: ', P[1], ' ', P[2]);
end;

var
  Bag: TBag;
  I: Integer;
begin
  Global := Default(TItem);
  Global.Name := 'g';
  Global.Count := 3;
  Touch(Global);
  WriteLn(Global.Name, ' ', Global.Count);
  Keep(Global);
  WriteLn(Global.Name, ' ', Global.Count);
  Reset(Global);
  WriteLn(Global.Name, ' ', Global.Count);

  Bag := Default(TBag);
  Add(Bag, 'a');
  Add(Bag, 'b');
  Add(Bag, 'c');
  for I := 0 to Bag.Size - 1 do
    WriteLn(Bag.Items[I].Name, ' ', Bag.Items[I].Count);
  WriteLn(Length(Bag.Items), ' ', High(Bag.Items));
  Local;
end.
