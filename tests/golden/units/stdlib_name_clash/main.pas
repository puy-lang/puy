// A unit of the standard library is found after the units of the package, and its
// declarations are shown at `<stdlib>/…` (docs/spec/collections.md 1.2): a name declared both by
// a unit of the package and by the Pascal part of System.SysUtils is E2502, whose label points
// into the standard library.
program Main;

uses Errors, System.SysUtils;

begin
  raise EListError.Create('x');
end.
