program RaiseHeld;

// Exception objects held by a record field, a dynamic array element, a global variable and the
// raised one are all released when the uncaught exception ends the program (no leaks;
// docs/spec/exceptions.md 6.1, 7.2, docs/design/lifetime.md 3.8).

type
  TFailure = record
    Code: Integer;
    Error: Exception;
  end;

var
  Last: Exception;

procedure Fail(const What: string);
var
  F: TFailure;
  All: TArray<Exception>;
begin
  F := Default(TFailure);
  F.Code := 1;
  F.Error := EConvertError.Create(What);
  SetLength(All, 2);
  All[0] := F.Error;
  All[1] := Exception.Create('second ' + What);
  Last := All[1];
  WriteLn(F.Error.Message, ' / ', All[1].Message, ' / ', Length(All[1].Suppressed));
  raise All[0];
end;

begin
  Last := nil;
  Fail('port');
  WriteLn(Last.Message);
end.
