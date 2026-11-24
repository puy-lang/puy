// The `class var`s no kept function uses leave the global variables and the units' ranges stay
// contiguous (docs/spec/units.md 6.2, 6.3): Hidden's range becomes empty, Kept's keeps its `var`
// and the `class var` it uses, and each unit's objects are still freed after its `finalization`.
program Main;

uses Hidden, Kept;

var
  Mine: TNoisy;

begin
  Mine := TNoisy.Create('main');
  TKeeper.Held := TNoisy.Create('kept');
  Counter := 1;
  WriteLn('main ', Counter);
end.
