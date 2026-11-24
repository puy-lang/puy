const NumSmallBlockTypes = 46;

type
  TSmallBlockTypeState = record
    InternalBlockSize: Cardinal;
    UseableBlockSize: Cardinal;
    AllocatedBlockCount: NativeUInt;
    ReservedAddressSpace: NativeUInt;
  end;

  TSmallBlockTypeStates = array[0..NumSmallBlockTypes - 1] of TSmallBlockTypeState;

  TMemoryManagerState = record
    SmallBlockTypeStates: TSmallBlockTypeStates;
    AllocatedMediumBlockCount: Cardinal;
    TotalAllocatedMediumBlockSize: NativeUInt;
    ReservedMediumBlockAddressSpace: NativeUInt;
    AllocatedLargeBlockCount: Cardinal;
    TotalAllocatedLargeBlockSize: NativeUInt;
    ReservedLargeBlockAddressSpace: NativeUInt;
  end;

procedure GetMemoryManagerState(out AMemoryManagerState: TMemoryManagerState);
