unit Log;

interface

procedure Note(const S: string);

implementation

procedure Note(const S: string);
begin
  WriteLn('[', S, ']');
end;

initialization
  WriteLn('init Log');

finalization
  WriteLn('fini Log');
  Flush(Output); // FPC 3.2.2 does not write out what finalization writes without it
end.
