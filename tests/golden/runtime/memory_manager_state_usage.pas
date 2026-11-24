// The compatibility API tracks the isolate's live blocks and accounted bytes through allocation and release.
program MemoryManagerStateUsage;

uses Puy.Runtime;

var
  State: TMemoryManagerState;
  Data: array of Int64;
  BeforeCount, DuringCount: Cardinal;
  BeforeSize, DuringSize: NativeUInt;
  BeforeMatches, DuringMatches, AfterMatches: Boolean;

begin
  GetMemoryManagerState(State);
  BeforeCount := State.AllocatedLargeBlockCount;
  BeforeSize := State.TotalAllocatedLargeBlockSize;
  BeforeMatches := (State.ReservedLargeBlockAddressSpace = NativeUInt(TMemory.IsolateUsed)) and
    (BeforeSize <= State.ReservedLargeBlockAddressSpace);

  SetLength(Data, 100000);
  GetMemoryManagerState(State);
  DuringCount := State.AllocatedLargeBlockCount;
  DuringSize := State.TotalAllocatedLargeBlockSize;
  DuringMatches := (State.ReservedLargeBlockAddressSpace = NativeUInt(TMemory.IsolateUsed)) and
    (DuringSize <= State.ReservedLargeBlockAddressSpace);

  Data := nil;
  GetMemoryManagerState(State);
  AfterMatches := (State.ReservedLargeBlockAddressSpace = NativeUInt(TMemory.IsolateUsed)) and
    (State.TotalAllocatedLargeBlockSize <= State.ReservedLargeBlockAddressSpace);

  WriteLn(BeforeMatches, ' ', DuringMatches, ' ', AfterMatches);
  WriteLn(DuringCount = BeforeCount + 1, ' ', DuringSize > BeforeSize);
  WriteLn(State.AllocatedLargeBlockCount = BeforeCount, ' ', State.TotalAllocatedLargeBlockSize = BeforeSize);
end.
