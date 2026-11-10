program DirTryBoundaries;

// Directives between the statements of try / except / finally blocks and between `on` handlers are
// at a boundary (directives.md 3.1); between `on` and `do` they are not (W0005).

uses System.SysUtils;

begin
  try
    {$R-}
    WriteLn('body');
    {$IFDEF NEVER}
    WriteLn('never');
    {$ENDIF}
    raise EConvertError.Create('x');
  except
    {$IFDEF PUY}
    on E: EConvertError do
      WriteLn('convert');
    {$ENDIF}
    on E {$R+}: Exception do
      WriteLn('other');
    {$IFDEF NEVER}
  else
    WriteLn('else');
    {$ENDIF}
  end;
  try
    WriteLn('again');
  finally
    {$R+}
    WriteLn('finally');
  end;
end.
