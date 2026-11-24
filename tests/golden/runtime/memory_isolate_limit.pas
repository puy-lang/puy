// puy-test: memory-limit=64k
// The isolate's configured limit is reported in bytes, independently of the process limit.
program MemoryIsolateLimit;

uses Puy.Runtime;

begin
  WriteLn(TMemory.IsolateLimit, ' ', TMemory.Limit);
end.
