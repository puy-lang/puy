program MemoryUsage;

uses
  System.SysUtils, Puy.Runtime;

type
  TSampleBuffer = class
  public
    Samples: TBytes;
  end;

procedure ShowBufferUsage;
begin
  var Buffer := TSampleBuffer.Create;
  try
    SetLength(Buffer.Samples, 4096);
    var State: TMemoryManagerState;
    GetMemoryManagerState(State);
    WriteLn('Live heap bytes: ', State.TotalAllocatedLargeBlockSize);
    WriteLn('Isolate bytes in use: ', TMemory.IsolateUsed);
    WriteLn('Isolate peak bytes: ', TMemory.IsolatePeak);
    for var Usage in TMemory.LiveTypes do
      if Usage.TypeName = 'MemoryUsage.TSampleBuffer' then
        WriteLn(Usage.TypeName, ': ', Usage.Count, ' block(s), ', Usage.Bytes, ' bytes');
  finally
    Buffer.Free;
  end;
end;

begin
  // Any blocks left after shutdown cleanup are reported on stderr.
  ReportMemoryLeaksOnShutdown := True;
  TMemory.ResetIsolatePeak;
  ShowBufferUsage;
  var ProcessUsed := TMemory.Used;
  if ProcessUsed >= 0 then
    WriteLn('Process allocator bytes in use: ', ProcessUsed);
end.
