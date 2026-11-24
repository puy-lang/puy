// The snapshot initializes a local record before its first read.
program MemoryManagerStateOut;

begin
  var State: TMemoryManagerState;
  GetMemoryManagerState(State);
  WriteLn(State.AllocatedLargeBlockCount > 0);
end.
