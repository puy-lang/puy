unit Second;

interface

implementation

initialization
  WriteLn('init Second');
  try
    Halt;
  finally
    WriteLn('finally Second');
  end;
  WriteLn('not reached');
finalization
  WriteLn('fini Second');
end.
