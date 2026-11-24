program StdlibConsoleReadLine;

// TConsole.ReadLine reads a line as ReadLn does, from the same buffer, so the two and the input
// stream read in turn; at the end of input it is '' and Eof tells it from an empty line
// (docs/spec/stdlib/Puy.Console/README.md 1).

uses
  System.SysUtils, System.Classes, Puy.Console;

var
  N: Integer;
  B: TBytes;
begin
  WriteLn('[', TConsole.ReadLine, ']');
  Read(N);
  WriteLn(N, ' [', TConsole.ReadLine, ']');
  SetLength(B, 2);
  TConsole.Input.ReadBuffer(B, 2);
  WriteLn(B[0], ' ', B[1], ' [', TConsole.ReadLine, ']');
  WriteLn('[', TConsole.ReadLine, '] ', Eof);
  WriteLn('[', TConsole.ReadLine, '] ', Eof);
  WriteLn('[', TConsole.ReadLine, '] ', Eof);
end.
