// puy-test: merge-stderr, stdout-buffering=full
program StdlibConsoleOutput;

// TConsole.Output writes through the standard output buffer of Write / WriteLn and
// TConsole.Error writes at once after writing that buffer out, so mixing them keeps the
// program's order (docs/spec/stdlib/Puy.Console/README.md 2). The capture merges the two
// streams like `2>&1`. The streams cannot seek, are not read / written the other way, and Close
// closes only the stream.

uses
  System.SysUtils, System.Classes, Puy.Console;

procedure Attempt(Step: Integer);
begin
  try
    case Step of
      1: WriteLn(TConsole.Output.Position);
      2: WriteLn(TConsole.Error.Size);
      3: TConsole.Input.Seek(0, TSeekOrigin.soBeginning);
      4: TConsole.Output.Size := 0;
      5: TConsole.Output.ReadData(Step);
      6: TConsole.Input.WriteData(Step);
      7: TConsole.Output.WriteText('x');
      8: WriteLn(TConsole.Output.Position);
      9: TConsole.Output.Write(nil, 0, 1);
    end;
    WriteLn(Step, ': no exception');
  except
    on E: Exception do
      WriteLn(Step, ': ', E.ClassName, ': ', E.Message);
  end;
end;

var
  B: TBytes;
begin
  Write('a');
  TConsole.Output.WriteText('b');
  TConsole.Error.WriteText('E1'#10);
  WriteLn('c');
  B := TEncoding.UTF8.GetBytes('d€'#10'e');
  WriteLn(TConsole.Output.Write(B, 0, Length(B)));
  TConsole.Error.WriteText('E2 ');
  WriteLn(ErrOutput, 'E3');
  // The low 3 bytes of the value, little-endian: 'f', 'g' and a line break
  TConsole.Output.WriteBufferData(Integer($0A6766), 3);
  WriteLn('h');

  WriteLn(TConsole.Input.CanSeek, ' ', TConsole.Output.CanSeek, ' ', TConsole.Error.CanSeek);
  // One stream of each per isolate
  WriteLn(TConsole.Output = TConsole.Output, ' ', TConsole.Output = TConsole.Error);
  Attempt(9);
  for var I := 1 to 6 do
    Attempt(I);
  // Close closes the stream, not standard output
  TConsole.Output.Close;
  TConsole.Output.Close;
  Attempt(7);
  Attempt(8);
  WriteLn('still written');
  TConsole.Error.WriteText('E4'#10);
end.
