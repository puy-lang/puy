program StdlibStreamWriter;

// TStreamWriter and TStream.WriteText on streams in memory (spec/stdlib/System.Classes/
// Readers.md): the text of each Write and WriteLine, NewLine, the preamble only at the start of
// the stream, AutoFlush and the buffer, a surrogate pair written in two Writes, Close, the last
// reference flushing, and the errors.

uses
  System.SysUtils, System.Classes;

type
  TThing = class
  public
    function ToString: string; override;
  end;

function TThing.ToString: string;
begin
  Result := 'a thing';
end;

procedure Dump(const M: TMemoryStream);
begin
  var B := M.ToBytes;
  for var I := 0 to High(B) do
    Write(IntToHex(B[I], 2), ' ');
  WriteLn('(', Length(B), ')');
end;

procedure Show(const M: TMemoryStream);
begin
  WriteLn('[', TEncoding.Default.GetString(M.ToBytes), ']');
end;

// A writer whose only reference goes when the procedure returns
procedure WriteAndDrop(const M: TMemoryStream);
begin
  var W := TStreamWriter.Create(M, nil, 1024);
  W.AutoFlush := False;
  W.Write('dropped');
end;

begin
  // Each overload writes the text of its value; WriteLine adds NewLine (LF)
  var M := TMemoryStream.Create;
  var W := TStreamWriter.Create(M);
  var Chars: TArray<Char> := ['w', 'x', 'y', 'z'];
  W.Write('s');
  W.Write(True);
  W.Write('c');
  W.Write(Chars);
  W.Write(Chars, 1, 2);
  W.Write(-5);
  W.Write(Cardinal(4000000000));
  W.Write(Int64(-1234567890123));
  W.Write(UInt64(18446744073709551615));
  W.Write(Single(0.1));
  W.Write(0.1);
  var Thing := TThing.Create;
  W.Write(Thing);
  W.WriteLine;
  W.WriteLine('line');
  W.WriteLine(False);
  W.WriteLine(7);
  W.WriteLine(2.5);
  W.WriteLine(Chars, 2, 2);
  W.WriteLine(Thing);
  Thing.Free;
  Show(M);
  W.NewLine := #13#10;
  W.WriteLine('crlf');
  WriteLn(M.Size, ' ', W.Encoding.EncodingName, ' ', W.BaseStream = M);
  try
    W.Write(Chars, 3, 2);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // The preamble is written when the stream is at its start; TEncoding.Default has none
  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M, TEncoding.UTF8);
  W.Write('é');
  Dump(M);
  W := TStreamWriter.Create(M, TEncoding.UTF8);
  W.Write('x');
  Dump(M);
  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M, TEncoding.Unicode);
  W.Write('aé');
  Dump(M);
  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M, TEncoding.BigEndianUnicode);
  W.Write('a');
  Dump(M);
  M := TMemoryStream.Create;
  TStreamWriter.Create(M).Write('ü');
  Dump(M);

  // Without AutoFlush the text waits until Flush, or until BufferSize characters
  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M, nil, 8);
  WriteLn(W.AutoFlush);
  W.AutoFlush := False;
  W.Write('abc');
  WriteLn(M.Size);
  W.Flush;
  WriteLn(M.Size);
  W.Write('defg');
  W.Write('hijk');
  WriteLn(M.Size);
  W.Flush;
  Show(M);
  // The last reference flushes
  M := TMemoryStream.Create;
  WriteAndDrop(M);
  Show(M);

  // A surrogate pair written in two Writes is one character, also with AutoFlush; Flush writes
  // a lone high surrogate as U+FFFD
  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M);
  W.Write(#$D83D);
  WriteLn(M.Size);
  W.Write(#$DE00);
  Dump(M);
  W.Write(#$D83D);
  W.Flush;
  Dump(M);

  // TStream.WriteText: the text in the encoding (Default without one), never a preamble
  M := TMemoryStream.Create;
  M.WriteText('é');
  M.WriteText('é', TEncoding.UTF8);
  M.WriteText('é', TEncoding.Unicode);
  M.WriteText('', TEncoding.Unicode);
  M.WriteText('a', nil);
  Dump(M);

  // Close writes the buffer and closes the stream; the writer cannot be used afterwards
  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M);
  W.AutoFlush := False;
  W.Write('kept');
  var Saved := TMemoryStream.Create;
  W.Flush;
  M.SaveToStream(Saved);
  W.Close;
  W.Close;
  Show(Saved);
  WriteLn(W.BaseStream = nil);
  try
    M.Position := 0;
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    W.Write('x');
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    W.Flush;
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    var None: TStream := nil;
    TStreamWriter.Create(None);
  except
    on E: EArgumentException do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // A lone surrogate with UTF8Strict raises when the text reaches the stream
  M := TMemoryStream.Create;
  W := TStreamWriter.Create(M, TEncoding.UTF8Strict);
  try
    W.Write('a'#$DC00);
  except
    on E: EEncodingError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
