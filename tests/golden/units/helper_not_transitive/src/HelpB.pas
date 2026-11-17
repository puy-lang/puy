unit HelpB;

// Uses `HelpA`, so its helper works here.

interface

uses HelpA;

function Loud(const S: string): string;

implementation

function Loud(const S: string): string;
begin
  Result := S.Bang;
end;

end.
