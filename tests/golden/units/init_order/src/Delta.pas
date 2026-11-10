// A unit with only `finalization` (docs/spec/units.md 1) and one with neither section.
unit Delta;

interface

uses Epsilon;

implementation

finalization
  WriteLn('fini Delta');
end.
