// A unit passed to puy check is checked and dumped, not run (docs/spec/units.md 1, 8.3).
unit Main platform;

interface

uses Helpers;

var
  Seed: Integer = 3;

function Next: Integer;

implementation

function Next: Integer;
begin
  Result := Double(Seed);
end;

end.
