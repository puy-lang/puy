uses System.Native;

procedure GetMemoryManagerState(out AMemoryManagerState: TMemoryManagerState);
begin
  AMemoryManagerState := Default(TMemoryManagerState);
  // Default leaves the record without a heap block. Writing this field allocates the
  // record's block so that the live-block totals below include it.
  AMemoryManagerState.SmallBlockTypeStates := Default(TSmallBlockTypeStates);
  AMemoryManagerState.AllocatedLargeBlockCount := System.Native.MemoryManagerLiveBlockCount;
  AMemoryManagerState.TotalAllocatedLargeBlockSize := System.Native.MemoryManagerLiveBlockBytes;
  AMemoryManagerState.ReservedLargeBlockAddressSpace := System.Native.MemoryManagerIsolateUsed;
end;
