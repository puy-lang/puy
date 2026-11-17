// Hint directives across units (attributes.md 6.2): `uses` of a deprecated unit is W0011 with the
// unit's description; a name qualified with the unit is reported for the declaration only, not
// for the unit again.
program Main;

uses Legacy;

begin
  WriteLn(Limit);
  OldRun;
  Legacy.OldRun;
  Trial;
end.
