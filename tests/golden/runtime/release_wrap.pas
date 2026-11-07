// puy-test: release
program ReleaseWrap;

var
  B: Byte;
  I: Integer;
begin
  I := 300;
  B := I;
  WriteLn(B);
  {$IFDEF RELEASE}
  WriteLn('release');
  {$ENDIF}
  {$IFDEF DEBUG}
  WriteLn('debug');
  {$ENDIF}
  Assert(False, 'skipped in release');
  WriteLn('end');
end.
