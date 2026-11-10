// `puy fix` applies the safe fixes in the units too (docs/spec/units.md 8.3): the program's
// and each unit's results are compared with main.fixed.pas and src/*.fixed.pas.
{$APPTYPE CONSOLE}
program Main;

uses Greet, Counter;

begin
  Hello;
  WriteLn(Next)
end.
