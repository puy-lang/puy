unit Counter;

interface

var
  Count: Integer;

procedure Bump;
function Describe: string;

implementation

uses Geometry, Log;

procedure Bump;
begin
  Inc(Count);
end;

function Describe: string;
begin
  if KindOf(MakeRect(Count, 5)) = TKind.Wide then
    Result := 'wide'
  else
    Result := 'not wide';
end;

initialization
  Note('init Counter');
  Count := 0;

finalization
  Note('fini Counter');
  Flush(Output); // FPC 3.2.2 does not write out what finalization writes without it
end.
