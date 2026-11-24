program E3003UnclosedUnicodeGuard;

// An unclosed Unicode guard still adds the note up to the end of the file.
type
  {$IFDEF UNICODE}
  TAlias = AnsiString;
begin
end.
