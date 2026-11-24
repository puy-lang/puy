// The entry for a qualified class name counts live instances and disappears when all are freed.
program MemoryLiveTypeCount;

uses Puy.Runtime;

type
  TItem = class
  end;

function ItemCount: Int64;
begin
  var Types := TMemory.LiveTypes;
  for var I := 0 to High(Types) do
    if Types[I].TypeName = 'MemoryLiveTypeCount.TItem' then
      Exit(Types[I].Count);
  Result := -1;
end;

var
  A, B, C: TItem;

begin
  WriteLn(ItemCount);
  A := TItem.Create;
  B := TItem.Create;
  C := TItem.Create;
  WriteLn(ItemCount);
  B.Free;
  WriteLn(ItemCount);
  A.Free;
  C.Free;
  WriteLn(ItemCount);
end.
