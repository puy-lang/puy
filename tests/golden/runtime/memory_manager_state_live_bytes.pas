// With several live types, the compatibility snapshot equals the sum of their accounted bytes.
program MemoryManagerStateLiveBytes;

uses Puy.Runtime;

var
  Text: string;
  Data: array of Int64;

begin
  SetLength(Text, 100);
  SetLength(Data, 1000);

  var State := Default(TMemoryManagerState);
  GetMemoryManagerState(State);
  var Types := TMemory.LiveTypes;
  var Bytes: Int64 := 0;
  for var I := 0 to High(Types) do
    Inc(Bytes, Types[I].Bytes);

  WriteLn(Length(Types) > 1);
  WriteLn(State.TotalAllocatedLargeBlockSize = NativeUInt(Bytes));
end.
