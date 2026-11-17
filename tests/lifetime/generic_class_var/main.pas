// The `class var`s of an instantiation are global variables of the unit that asked for the
// instantiation first (here `Users` for `THolder<Integer>` and `THolder<string>`, the program for
// `THolder<Double>`), released right after that unit's `<fini>` (generics.md 2.5,
// docs/design/generics.md 3.2, docs/spec/units.md 5.4): no leak, and `Users`'s `<fini>` still
// reads them.
program Main;

uses Users, Holders;

begin
  Run;
  THolder<string>.Keep('strings in Main', 'b');
  THolder<Double>.Keep('doubles', 1.5);
  WriteLn(Length(THolder<string>.Values), ' ', THolder<Double>.Tag.Name);
end.
