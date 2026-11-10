// The statements of `initialization` / `finalization` follow the rules of `<main>`: inline
// variables are local to the section, `Exit` leaves the section, a global variable can be a
// `for` variable. The old form `begin … end.` runs as `initialization` with W0020
// (docs/spec/units.md 1, 5.1).
program Main;

uses Sections, Legacy;

begin
  WriteLn('main ', Total);
end.
