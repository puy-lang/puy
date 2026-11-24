program StdlibStreamData;

// ReadData / WriteData and the Buffer versions (spec/stdlib/System.Classes/TStream.md): each
// type's bytes, little-endian, read back the same; with Count, the extra bytes are zeros when
// written and skipped when read; a read cut short by the end replaces only the low bytes read;
// ReadBufferData raises EReadError at the end.

uses
  System.SysUtils, System.Classes;

procedure Dump(const S: TMemoryStream);
begin
  var B := S.ToBytes;
  for var I := 0 to High(B) do
    Write(IntToHex(B[I], 2));
  WriteLn;
end;

begin
  var S := TMemoryStream.Create;
  var Bo: Boolean := True;
  var C: Char := 'é';
  var Si: ShortInt := -2;
  var By: Byte := 200;
  var Sm: SmallInt := -300;
  var W: Word := 65000;
  var I: Integer := -123456789;
  var Ca: Cardinal := 4000000000;
  var I6: Int64 := -1234567890123;
  var U6: UInt64 := UInt64(High(Int64)) + 10;
  var Sg: Single := 1.5;
  var D: Double := -0.1;
  Write(S.WriteData(Bo), S.WriteData(C), S.WriteData(Si), S.WriteData(By), S.WriteData(Sm),
    S.WriteData(W), S.WriteData(I), S.WriteData(Ca), S.WriteData(I6), S.WriteData(U6),
    S.WriteData(Sg), S.WriteData(D));
  WriteLn;
  Dump(S);

  S.Position := 0;
  Bo := False;
  C := 'x';
  Si := 0;
  By := 0;
  Sm := 0;
  W := 0;
  I := 0;
  Ca := 0;
  I6 := 0;
  U6 := 0;
  Sg := 0;
  D := 0;
  Write(S.ReadData(Bo), S.ReadData(C), S.ReadData(Si), S.ReadData(By), S.ReadData(Sm),
    S.ReadData(W), S.ReadData(I), S.ReadData(Ca), S.ReadData(I6), S.ReadData(U6),
    S.ReadData(Sg), S.ReadData(D));
  WriteLn;
  WriteLn(Bo, ' ', C, ' ', Si, ' ', By, ' ', Sm, ' ', W, ' ', I, ' ', Ca, ' ', I6, ' ', U6, ' ',
    Sg:0:1, ' ', D:0:1);

  // With Count: two bytes of an Integer, then an Integer padded to 6 bytes
  S.Clear;
  I := $11223344;
  WriteLn(S.WriteData(I, 2), ' ', S.WriteData(I, 6));
  Dump(S);
  S.Position := 0;
  I := -1;
  WriteLn(S.ReadData(I, 2), ' ', IntToHex(I, 8));
  WriteLn(S.ReadData(I, 6), ' ', IntToHex(I, 8), ' ', S.Position);

  // Cut short by the end: only the low bytes read change
  S.Clear;
  S.WriteData(Word($ABCD));
  S.Position := 0;
  I := $11223344;
  WriteLn(S.ReadData(I), ' ', IntToHex(I, 8));

  // A byte other than 0 and 1 reads as True
  S.Clear;
  S.WriteData(Byte(7));
  S.Position := 0;
  WriteLn(S.ReadData(Bo), ' ', Bo);

  S.Position := 0;
  try
    S.ReadBufferData(I);
  except
    on E: EReadError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  S.Clear;
  S.WriteBufferData(D);
  S.WriteBufferData(I, 1);
  S.Position := 0;
  D := 0;
  S.ReadBufferData(D);
  I := 0;
  S.ReadBufferData(I, 1);
  WriteLn(D:0:1, ' ', I, ' ', S.Size);
end.
