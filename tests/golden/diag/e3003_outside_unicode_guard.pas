program E3003OutsideUnicodeGuard;

// Closing the guard, taking an unrelated branch or merely mentioning Unicode does not add a note.
type
  {$IFDEF UNICODE}
  TSupported = string;
  {$ENDIF}
  TAfterGuard = RawByteString;
  {$IFNDEF UNICODE}
  {$ELSE}
  TElse = AnsiString;
  {$ENDIF}
  {$IF Defined(UNICODE) or Defined(DEBUG)}
  TCompound = AnsiChar;
  {$ENDIF}
begin
end.
