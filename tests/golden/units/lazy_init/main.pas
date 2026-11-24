program Main;

// A unit's `initialization` creates a TLazy; its value is made at the first read in the main
// program, once (docs/spec/stdlib/Puy.Lazy/README.md 1).

uses
  Settings;

begin
  WriteLn('main starts');
  WriteLn(Greeting.Value);
  WriteLn(Greeting.Value);
end.
