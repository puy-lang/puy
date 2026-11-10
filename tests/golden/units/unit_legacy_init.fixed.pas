// The old form `begin … end.` of a unit's initialization is accepted with W0020, whose fix
// replaces `begin` with `initialization` (docs/spec/units.md 1).
unit UnitLegacyInit;

interface

implementation

initialization
  WriteLn('init');
end.
