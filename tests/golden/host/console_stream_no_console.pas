// puy-test: no-console
program ConsoleStreamNoConsole;

// Without a `Console` (docs/spec/stdlib/Puy.Console/README.md 2, 3), what the output and error
// streams write is discarded as Write's is, and reading the input stream raises
// `ENotSupportedException` as ReadLn does.

uses
  System.SysUtils, System.Classes, Puy.Console;

var
  B: TBytes;
begin
  TConsole.Output.WriteText('discarded');
  TConsole.Error.WriteText('discarded');
  SetLength(B, 4);
  TConsole.Input.Read(B, 0, 4);
end.
