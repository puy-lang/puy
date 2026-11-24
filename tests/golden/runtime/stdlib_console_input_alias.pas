program StdlibConsoleInputAlias;

// TConsole.Input.Read into a buffer whose element is lent to a var parameter raises EAliasError
// before it reads a byte, so the input it would have read is still there for ReadLn
// (docs/spec/bytecode.md 2.22, I113).

uses
  System.SysUtils, System.Classes, Puy.Console;

var
  B: TBytes;

procedure Lend(var X: Byte);
begin
  try
    TConsole.Input.Read(B, 0, 4);
  except
    on E: EAliasError do
      WriteLn(E.ClassName);
  end;
end;

var
  S: string;
  N: Integer;
begin
  SetLength(B, 4);
  // At the start of a line, and in a line read in part
  Lend(B[1]);
  ReadLn(S);
  WriteLn('[', S, ']');
  Read(N);
  Lend(B[3]);
  ReadLn(S);
  WriteLn(N, ' [', S, ']');
end.
