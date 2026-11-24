// The isolate peak grows on resize, survives release, and can be reset to current usage.
program MemoryIsolatePeak;

uses Puy.Runtime;

var
  A: array of Int64;
  Before, Large, Peak, AfterReset, UsedAfterReset: Int64;

begin
  SetLength(A, 100);
  Before := TMemory.IsolateUsed;
  TMemory.ResetIsolatePeak;
  SetLength(A, 100000);
  Large := TMemory.IsolateUsed;
  A := nil;
  Peak := TMemory.IsolatePeak;
  TMemory.ResetIsolatePeak;
  AfterReset := TMemory.IsolatePeak;
  UsedAfterReset := TMemory.IsolateUsed;
  WriteLn(Large > Before, ' ', Peak >= Large, ' ', AfterReset = UsedAfterReset);
end.
