program StdlibBinaryReaderWriter;

// TBinaryWriter / TBinaryReader on streams in memory (spec/stdlib/System.Classes/Readers.md):
// each type's bytes, little-endian, read back the same; strings with a 7-bit encoded length;
// characters in the encoding, a pair read as two code units; PeekChar; the end of the stream;
// a length that is not a 7-bit encoded integer; Seek and Close.

uses
  System.SysUtils, System.Classes;

procedure Dump(const M: TMemoryStream; From: Integer);
begin
  var B := M.ToBytes;
  for var I := From to High(B) do
    Write(IntToHex(B[I], 2), ' ');
  WriteLn('(', Length(B) - From, ')');
end;

begin
  var M := TMemoryStream.Create;
  var W := TBinaryWriter.Create(M);
  W.Write(True);
  W.Write(Byte(200));
  W.Write(ShortInt(-2));
  W.Write(SmallInt(-300));
  W.Write(Word(65000));
  W.Write(Integer(-123456789));
  W.Write(Cardinal(4000000000));
  W.Write(Int64(-1234567890123));
  W.Write(UInt64(18446744073709551615));
  W.Write(Single(1.5));
  W.Write(Double(-0.1));
  Dump(M, 0);
  M.Position := 0;
  var R := TBinaryReader.Create(M);
  WriteLn(R.ReadBoolean, ' ', R.ReadByte, ' ', R.ReadSByte, ' ', R.ReadInt16, ' ',
    R.ReadUInt16, ' ', R.ReadInt32, ' ', R.ReadUInt32, ' ', R.ReadInt64, ' ', R.ReadUInt64, ' ',
    R.ReadSingle:0:2, ' ', R.ReadDouble:0:2);
  // The other names of the same reads
  M.Position := 1;
  WriteLn(R.ReadByte, ' ', R.ReadShortInt, ' ', R.ReadSmallInt, ' ', R.ReadWord, ' ',
    R.ReadInteger, ' ', R.ReadCardinal);

  // Strings: the length in bytes as a 7-bit encoded integer (2 bytes from 128), then the bytes
  M := TMemoryStream.Create;
  W := TBinaryWriter.Create(M);
  W.Write('');
  W.Write('é日');
  Dump(M, 0);
  var Long := StringOfChar('a', 200);
  W.Write(Long);
  M.Position := 7;
  var B: TBytes;
  SetLength(B, 3);
  M.Read(B, 3);
  WriteLn(IntToHex(B[0], 2), ' ', IntToHex(B[1], 2), ' ', IntToHex(B[2], 2), ' ', M.Size);
  M.Position := 0;
  R := TBinaryReader.Create(M);
  WriteLn('[', R.ReadString, '] [', R.ReadString, '] ', R.ReadString = Long);
  // In UTF-16
  M := TMemoryStream.Create;
  W := TBinaryWriter.Create(M, TEncoding.Unicode);
  W.Write('ab');
  Dump(M, 0);
  M.Position := 0;
  WriteLn(TBinaryReader.Create(M, TEncoding.Unicode).ReadString);

  // Characters in the encoding, with no length; a pair is read as its two code units
  M := TMemoryStream.Create;
  W := TBinaryWriter.Create(M);
  W.Write('a');
  W.Write(Char('é'));
  var Chars: TArray<Char> := ['x', '日', 'y'];
  W.Write(Chars);
  W.Write(Chars, 2, 1);
  W.Write(TEncoding.UTF8.GetBytes('😀'));
  B := [1, 2, 3, 4];
  W.Write(B, 1, 2);
  Dump(M, 0);
  M.Position := 0;
  R := TBinaryReader.Create(M);
  WriteLn(R.PeekChar, ' ', R.ReadChar, ' ', R.Read, ' ', M.Position);
  var C := R.ReadChars(2);
  WriteLn(Length(C), ' ', C[0], C[1], ' ', R.ReadChar);
  WriteLn(IntToHex(R.PeekChar, 4), ' ', IntToHex(R.Read, 4), ' ', IntToHex(R.PeekChar, 4), ' ',
    IntToHex(Ord(R.ReadChar), 4), ' ', IntToHex(R.PeekChar, 4), ' ', IntToHex(R.Read, 4));
  SetLength(B, 4);
  WriteLn(R.Read(B, 1, 3), ' ', B[1], ' ', B[2]);
  // The end: Read and PeekChar give -1, ReadChars and ReadBytes fewer, the others EReadError
  WriteLn(R.Read, ' ', R.PeekChar, ' ', Length(R.ReadChars(3)), ' ', Length(R.ReadBytes(3)));
  try
    R.ReadChar;
  except
    on E: EReadError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    R.ReadInt32;
  except
    on E: EReadError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  M.Position := 0;
  B := R.ReadBytes(3);
  WriteLn(Length(B), ' ', B[0], ' ', B[2]);

  // Invalid UTF-8 is U+FFFD; in UTF-16 a character is 2 bytes
  B := [$FF, $41];
  R := TBinaryReader.Create(TBytesStream.Create(B));
  WriteLn(R.Read, ' ', R.Read, ' ', R.Read);
  B := [$41, $00, $3D, $D8, $00, $DE];
  R := TBinaryReader.Create(TBytesStream.Create(B), TEncoding.Unicode);
  WriteLn(R.Read, ' ', IntToHex(R.Read, 4), ' ', IntToHex(R.Read, 4), ' ', R.Read);

  // PeekChar moves the stream back also when decoding raises (UTF8Strict)
  B := [$41, $FF, $42];
  var Strict := TBytesStream.Create(B);
  R := TBinaryReader.Create(Strict, TEncoding.UTF8Strict);
  WriteLn(R.ReadChar, ' ', Strict.Position);
  try
    R.PeekChar;
  except
    on E: EEncodingError do
      WriteLn(E.ClassName, ': ', E.Message, ' ', Strict.Position);
  end;

  // A length that does not end within 5 bytes
  B := [$80, $80, $80, $80, $80, $01];
  try
    TBinaryReader.Create(TBytesStream.Create(B)).ReadString;
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  // A string cut short
  B := [$05, $61];
  try
    TBinaryReader.Create(TBytesStream.Create(B)).ReadString;
  except
    on E: EReadError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // Seek, Flush, BaseStream and Close
  M := TMemoryStream.Create;
  W := TBinaryWriter.Create(M);
  W.Write(Integer(1));
  WriteLn(W.Seek(-2, TSeekOrigin.soCurrent), ' ', W.BaseStream = M);
  W.Write(Word($FFFF));
  W.Flush;
  Dump(M, 0);
  W.Close;
  W.Close;
  try
    M.Position := 0;
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    W.Write(Byte(1));
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  R := TBinaryReader.Create(TMemoryStream.Create);
  R.Close;
  try
    R.ReadByte;
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
