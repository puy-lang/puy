// A snapshot counts existing non-object blocks and excludes the allocations for its own result.
program MemoryLiveTypeSnapshot;

uses Puy.Runtime;

type
  TNumbers = array of Int64;

var
  Numbers: TNumbers;
  Text: string;
  Types: TArray<TMemoryTypeUsage>;
  EmptyCount: Integer;

begin
  EmptyCount := Length(TMemory.LiveTypes);
  SetLength(Text, 100);
  SetLength(Numbers, 1000);
  Types := TMemory.LiveTypes;
  WriteLn(EmptyCount);
  WriteLn(Length(Types));
  for var I := 0 to High(Types) do
    WriteLn(Types[I].TypeName, ' ', Types[I].Count, ' ', Types[I].Bytes > 0);
end.
