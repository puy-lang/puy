// Unit names come from the path under src/ (E2503, E2506, *_test.pas, E2500 variants, E2504;
// docs/spec/units.md 2.2, 2.3).
program Main;

uses
  Models.User, System.Extra, Pair.One, Calc_test, Tool, App.Missing.Name,
  Greeting in 'lib/Greeting.pas', Greeting in 'src/Greeting.pas';

begin
end.
