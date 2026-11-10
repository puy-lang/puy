// A unit checked on its own (no puy.toml): its syntax, E0414 per section, and the initialization
// sections that are still E3200 (docs/spec/units.md 1).
unit UnitAlone;

interface
uses System.SysUtils;

const
  Answer = 42;


function Describe: string;

implementation

uses Missing;

function Describe: string;
begin
  Result := IntToStr(Answer);
end;

initialization
  WriteLn(Describe);
finalization
  WriteLn('bye');
end.
