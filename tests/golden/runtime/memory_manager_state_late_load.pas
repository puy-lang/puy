// The anonymous method's signature needs TInstant before its body can ask for System.Native.
program MemoryManagerStateLateLoad;

type
  TSnapshot = reference to function(const At: TInstant): Boolean;

var
  Snapshot: TSnapshot;

begin
  Snapshot := function(const At: TInstant): Boolean
    begin
      var State := Default(TMemoryManagerState);
      GetMemoryManagerState(State);
      Result := (At.ToUnixSeconds = 0) and (State.TotalAllocatedLargeBlockSize > 0);
    end;
  WriteLn(Snapshot(TInstant.FromUnixSeconds(0)));
end.
