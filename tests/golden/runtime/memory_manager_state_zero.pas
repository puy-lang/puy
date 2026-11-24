// System exposes the Delphi declaration without uses and overwrites every small and medium field.
program MemoryManagerStateZero;

var
  State: System.TMemoryManagerState;
  Small: TSmallBlockTypeState;
  SmallStates: TSmallBlockTypeStates;
  AllSmallZero: Boolean;

begin
  State := Default(System.TMemoryManagerState);
  Small.InternalBlockSize := 1;
  Small.UseableBlockSize := 2;
  Small.AllocatedBlockCount := 3;
  Small.ReservedAddressSpace := 4;
  for var I := 0 to System.NumSmallBlockTypes - 1 do
    SmallStates[I] := Small;
  State.SmallBlockTypeStates := SmallStates;
  State.AllocatedMediumBlockCount := 5;
  State.TotalAllocatedMediumBlockSize := 6;
  State.ReservedMediumBlockAddressSpace := 7;

  System.GetMemoryManagerState(State);

  AllSmallZero := True;
  for var I := Low(State.SmallBlockTypeStates) to High(State.SmallBlockTypeStates) do
  begin
    Small := State.SmallBlockTypeStates[I];
    AllSmallZero := AllSmallZero and (Small.InternalBlockSize = 0) and (Small.UseableBlockSize = 0) and
      (Small.AllocatedBlockCount = 0) and (Small.ReservedAddressSpace = 0);
  end;
  WriteLn(NumSmallBlockTypes, ' ', Low(State.SmallBlockTypeStates), ' ', High(State.SmallBlockTypeStates));
  WriteLn(AllSmallZero, ' ', State.AllocatedMediumBlockCount, ' ', State.TotalAllocatedMediumBlockSize,
    ' ', State.ReservedMediumBlockAddressSpace);
end.
