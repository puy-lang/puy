unit High;

interface

uses Low;

implementation

finalization
  try
    Halt(4);
  finally
    WriteLn('finally High');
  end;
  WriteLn('not reached');
end.
