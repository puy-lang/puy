// Larger totals sort first, and equal-sized class entries sort by their qualified names.
program MemoryLiveTypeOrder;

uses Puy.Runtime;

type
  TAlpha = class end;
  TMiddle = class
    Payload: Int64;
  end;
  TZulu = class end;

var
  A: TAlpha;
  M: TMiddle;
  Z: TZulu;
  Types: TArray<TMemoryTypeUsage>;

begin
  Z := TZulu.Create;
  M := TMiddle.Create;
  M.Payload := 42;
  A := TAlpha.Create;
  Types := TMemory.LiveTypes;
  for var I := 0 to High(Types) do
    if (Types[I].TypeName = 'MemoryLiveTypeOrder.TAlpha') or
       (Types[I].TypeName = 'MemoryLiveTypeOrder.TMiddle') or
       (Types[I].TypeName = 'MemoryLiveTypeOrder.TZulu') then
      WriteLn(Types[I].TypeName);
  A.Free;
  M.Free;
  Z.Free;
end.
