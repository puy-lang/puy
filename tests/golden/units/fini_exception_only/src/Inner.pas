unit Inner;

interface

implementation

finalization
  try
    Halt(0);
  finally
    WriteLn('finally Inner');
    raise Exception.Create('dropped');
  end;
end.
