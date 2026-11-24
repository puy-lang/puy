program BinaryReaderWriter;

// TBinaryWriter and TBinaryReader on a stream in memory (docs/spec/stdlib/System.Classes/
// Readers.md) as Delphi's: the bytes of each type, little-endian; a string with its length as a
// 7-bit encoded integer (2 bytes from 128); ASCII characters; reading it all back. Streams and
// their readers are reference counted in Puy, which does not allow Free; the other compilers
// keep them until the program ends.

uses System.SysUtils, System.Classes;

procedure ShowBytes(const M: TMemoryStream; From, Count: Integer);
var
  B: TBytes;
  I: Integer;
begin
  SetLength(B, Count);
  M.Position := From;
  M.Read(B, 0, Count);
  for I := 0 to Count - 1 do
    Write(IntToHex(B[I], 2), ' ');
  WriteLn;
end;

var
  M: TMemoryStream;
  W: TBinaryWriter;
  R: TBinaryReader;
  Long: string;
  B: TBytes;
  Sg: Single;
  Db: Double;
begin
  Sg := 1.5;
  Db := -0.1;
  M := TMemoryStream.Create;
  W := TBinaryWriter.Create(M);
  W.Write(True);
  W.Write(Byte(200));
  W.Write(ShortInt(-2));
  W.Write(SmallInt(-300));
  W.Write(Word(65000));
  W.Write(Integer(-123456789));
  W.Write(Cardinal(4000000000));
  W.Write(Int64(-1234567890123));
  W.Write(Sg);
  W.Write(Db);
  W.Write('hé');
  Long := StringOfChar('a', 200);
  W.Write(Long);
  W.Write(Char('z'));
  SetLength(B, 3);
  B[0] := 7;
  B[1] := 8;
  B[2] := 9;
  W.Write(B, 1, 2);
  WriteLn(M.Size);
  ShowBytes(M, 0, 35);
  ShowBytes(M, 35, 7);
  ShowBytes(M, 239, 3);

  M.Position := 0;
  R := TBinaryReader.Create(M);
  WriteLn(R.ReadBoolean, ' ', R.ReadByte, ' ', R.ReadShortInt, ' ', R.ReadSmallInt, ' ',
    R.ReadWord, ' ', R.ReadInteger, ' ', R.ReadCardinal, ' ', R.ReadInt64);
  WriteLn(R.ReadSingle:0:2, ' ', R.ReadDouble:0:2, ' ', R.ReadString);
  WriteLn(R.ReadString = Long, ' ', R.ReadChar);
  B := R.ReadBytes(2);
  WriteLn(Length(B), ' ', B[0], ' ', B[1], ' ', M.Position);
end.
