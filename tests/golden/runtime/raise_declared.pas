// An uncaught exception of a declared class is reported with the declared name and the trace of
// the `raise` (docs/spec/exceptions.md 3, 7.1).
type
  EParseError = class(Exception);
  EEmptyInput = class(EParseError) end;

procedure Parse(const S: string);
begin
  if S = '' then
    raise EEmptyInput.Create('the input is empty');
  WriteLn('parsed ', S);
end;

Parse('abc');
Parse('');
WriteLn('not reached');
