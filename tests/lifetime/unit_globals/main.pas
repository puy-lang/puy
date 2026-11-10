// Each unit's managed global variables are released right after its `<fini>`, in reverse
// declaration order, so strings, arrays and records held by units leave no leak; a `<fini>`
// still reads its own unit's values and those of the units it uses (docs/spec/units.md 5.4,
// docs/design/lifetime.md 3.8).
program Main;

uses Store, Names;

var
  Mine: string;
begin
  Mine := Greeting + '!';
  Add(Mine);
  WriteLn(Mine, ' ', Length(Items));
end.
