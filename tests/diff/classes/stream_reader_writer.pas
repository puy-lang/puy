program StreamReaderWriter;

// TStreamWriter and TStreamReader on streams in memory (docs/spec/stdlib/System.Classes/
// Readers.md) as Delphi's: Write and WriteLine of values with NewLine set to LF, the preamble of
// TEncoding.UTF8 and of UTF-16, AutoFlush on a stream given, reading the three line breaks, the
// byte order marks, Peek / Read / ReadToEnd / EndOfStream. Streams and their readers are
// reference counted in Puy, which does not allow Free; the other compilers keep them until the
// program ends.

uses System.SysUtils, System.Classes;

procedure ShowBytes(const M: TMemoryStream);
var
  B: TBytes;
  I: Integer;
begin
  SetLength(B, M.Size);
  M.Position := 0;
  M.Read(B, 0, Length(B));
  for I := 0 to High(B) do
    Write(IntToHex(B[I], 2), ' ');
  WriteLn('(', Length(B), ')');
end;

function FromBytes(const B: TBytes): TMemoryStream;
begin
  Result := TMemoryStream.Create;
  Result.Write(B, 0, Length(B));
  Result.Position := 0;
end;

var
  M: TMemoryStream;
  W: TStreamWriter;
  R: TStreamReader;
  B: TBytes;
begin
  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M, TEncoding.UTF8);
  W.NewLine := #10;
  WriteLn(W.AutoFlush);
  W.Write('a');
  W.Write(12);
  W.Write(True);
  W.WriteLine;
  W.WriteLine('é');
  W.WriteLine(-3);
  WriteLn(M.Size);
  ShowBytes(M);

  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M, TEncoding.Unicode);
  W.Write('ab');
  ShowBytes(M);

  M := FromBytes(TEncoding.UTF8.GetBytes('one'#10'two'#13#10'three'#13'four'#10#10'five'));
  R := TStreamReader.Create(M);
  while not R.EndOfStream do
    WriteLn('[', R.ReadLine, ']');

  B := TEncoding.UTF8.GetPreamble;
  M := FromBytes(B);
  M.Position := M.Size;
  B := TEncoding.UTF8.GetBytes('é1'#10'2');
  M.Write(B, 0, Length(B));
  M.Position := 0;
  R := TStreamReader.Create(M, TEncoding.Unicode, True);
  WriteLn(Length(R.ReadLine), ' ', R.ReadLine, ' ', R.EndOfStream);
  WriteLn(R.CurrentEncoding.CodePage);

  B := TEncoding.Unicode.GetPreamble;
  M := FromBytes(B);
  M.Position := M.Size;
  B := TEncoding.Unicode.GetBytes('xy'#13#10'zw');
  M.Write(B, 0, Length(B));
  M.Position := 0;
  R := TStreamReader.Create(M, True);
  WriteLn(R.Peek, ' ', R.Read, ' ', R.Peek);
  WriteLn(R.CurrentEncoding.CodePage);
  WriteLn('[', R.ReadLine, '] [', R.ReadToEnd, ']');
  WriteLn(R.EndOfStream, ' ', R.Peek, ' ', R.Read);
end.
