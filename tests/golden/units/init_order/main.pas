// Units are initialized depth-first along `uses` (dependencies first, the interface `uses`
// before the implementation `uses`, each unit once) and finalized in reverse order
// (docs/spec/units.md 5.2, 5.4).
program Main;

uses Alpha, Beta;

begin
  WriteLn('main');
end.
