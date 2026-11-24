// The old System memory APIs lead to TMemory in both qualified and unqualified forms.
program MemoryManagerStateUnavailable;

var
  Plain: THeapStatus;
  Qualified: System.THeapStatus;
begin
  GetHeapStatus;
  System.GetHeapStatus;
  WriteLn(AllocMemSize, AllocMemCount);
  WriteLn(System.AllocMemSize, System.AllocMemCount);
end.
