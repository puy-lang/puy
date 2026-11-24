program E3003UnicodeGuard;

// Both spellings of a Unicode guard should add a note, including inside nested branches.
type
  {$IFDEF UNICODE}
  TBytesAlias = RawByteString;
    {$IFDEF ABSENT}
    {$ELSE}
    TNested = AnsiChar;
    {$ENDIF}
  {$ENDIF}
  (*$if (defined (unicode))*)
  TAnsiAlias = AnsiString;
  (*$ifend*)
begin
end.
