// The CLI's host measures process usage and reports -1 for the limit and the remaining bytes.
program MemoryProcessUnlimited;

uses Puy.Runtime;

begin
  WriteLn(TMemory.Used > 0, ' ', TMemory.Limit, ' ', TMemory.Available);
end.
