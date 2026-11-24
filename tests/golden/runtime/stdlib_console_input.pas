program StdlibConsoleInput;

// TConsole.Input reads the bytes Read / ReadLn have not read, from the same buffer
// (docs/spec/stdlib/Puy.Console/README.md 3): binary input as TBytes, a line read in part, a
// line read ahead by Eof, the end of input. Read gives the bytes there are, up to Count.

uses
  System.SysUtils, System.Classes, Puy.Console;

procedure Dump(const B: TBytes; N: Integer);
begin
  Write(N, ':');
  for var I := 0 to N - 1 do
    Write(' ', B[I]);
  WriteLn;
end;

var
  S: string;
  N: Integer;
  C: Char;
  B: TBytes;
  W: Word;
  R: TStreamReader;
begin
  // A header line, then 6 bytes of binary data (CR, LF, 0, 255 and a little-endian Word)
  ReadLn(S);
  WriteLn('[', S, ']');
  SetLength(B, 4);
  TConsole.Input.ReadBuffer(B, 4);
  Dump(B, 4);
  TConsole.Input.ReadBufferData(W);
  WriteLn(W);
  // The rest of the line after the binary data
  ReadLn(S);
  WriteLn('[', S, ']');

  // Read stops in a line: the stream goes on after the number, with the CR LF of the line
  Read(N);
  WriteLn(N);
  TConsole.Input.ReadBuffer(B, 4);
  Dump(B, 4);

  // An invalid UTF-8 byte is U+FFFD for ReadLn and the byte itself for the stream
  ReadLn(S);
  WriteLn(Length(S), ' ', Ord(S[1]), ' ', S[2]);
  Read(C);
  WriteLn(Ord(C));
  TConsole.Input.ReadBuffer(B, 3);
  Dump(B, 3);

  // Read of one UTF-16 code unit of a surrogate pair: the stream goes on after the character
  Read(C);
  WriteLn(Ord(C));
  TConsole.Input.ReadBuffer(B, 3);
  Dump(B, 3);

  // At the end of a line, Eof reads the next one ahead; the stream reads the line break first,
  // then that line
  Read(S);
  WriteLn('[', S, '] ', Eoln, ' ', Eof);
  SetLength(B, 8);
  TConsole.Input.ReadBuffer(B, 8);
  Dump(B, 8);

  // A reader on the stream reads the lines left; then the stream and ReadLn are at the end
  R := TStreamReader.Create(TConsole.Input);
  while not R.EndOfStream do
    WriteLn('reader: [', R.ReadLine, ']');
  WriteLn(TConsole.Input.Read(B, 0, 8), ' ', Eof);
  ReadLn(S);
  WriteLn('[', S, ']');
  // A count of 0 reads nothing; a range out of the buffer is ERangeError
  WriteLn(TConsole.Input.Read(B, 8, 0));
  try
    TConsole.Input.Read(B, 4, 5);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
