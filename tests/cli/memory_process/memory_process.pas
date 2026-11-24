// The CLI's global allocator reports growing process usage and retains the process peak.
program MemoryProcess;

uses Puy.Runtime;

var
  A: array of Int64;
  Before, Large, After, Peak: Int64;

begin
  Before := TMemory.Used;
  SetLength(A, 100000);
  Large := TMemory.Used;
  A := nil;
  TMemory.ResetIsolatePeak;
  After := TMemory.Used;
  Peak := TMemory.Peak;
  WriteLn(Before > 0, ' ', Large > Before, ' ', After < Large, ' ', Peak >= Large);
end.
