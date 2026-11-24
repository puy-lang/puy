// A dynamic array increases isolate usage when allocated and resized, then releases its bytes.
program MemoryIsolateUsage;

uses Puy.Runtime;

var
  A: array of Int64;
  Before, Small, Large, After: Int64;

begin
  Before := TMemory.IsolateUsed;
  SetLength(A, 100);
  Small := TMemory.IsolateUsed;
  SetLength(A, 100000);
  Large := TMemory.IsolateUsed;
  A := nil;
  After := TMemory.IsolateUsed;
  WriteLn(Small > Before, ' ', Large > Small, ' ', After = Before);
end.
