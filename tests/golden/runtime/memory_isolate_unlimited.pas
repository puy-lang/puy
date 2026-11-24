// An isolate started without a limit reports -1.
program MemoryIsolateUnlimited;

uses Puy.Runtime;

begin
  WriteLn(TMemory.IsolateLimit);
end.
