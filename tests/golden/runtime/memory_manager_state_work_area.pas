// A deeper call grows the register work area without leaving live blocks behind.
program MemoryManagerStateWorkArea;

procedure Descend(Depth: Integer);
begin
  if Depth > 0 then
    Descend(Depth - 1);
end;

begin
  var State := Default(TMemoryManagerState);
  GetMemoryManagerState(State);
  var BeforeTotal := State.TotalAllocatedLargeBlockSize;
  var BeforeReserved := State.ReservedLargeBlockAddressSpace;

  Descend(128);

  GetMemoryManagerState(State);
  WriteLn(State.ReservedLargeBlockAddressSpace > BeforeReserved);
  WriteLn(State.TotalAllocatedLargeBlockSize = BeforeTotal);
end.
