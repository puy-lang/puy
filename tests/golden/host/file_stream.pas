// puy-test: test-fs
program FileStream;

// TFileStream on the runner's file system in memory (spec/stdlib/System.Classes/TStream.md):
// the ways to open a file, reading and writing, Seek / Size / Position, setting the size,
// Close, the exceptions of the host's failures with their ErrorKind, and TMemoryStream's
// SaveToFile / LoadFromFile.

uses
  System.SysUtils, System.Classes;

procedure Show(const What: string; const S: TStream);
begin
  WriteLn(What, ': Size=', S.Size, ' Position=', S.Position);
end;

begin
  var F := TFileStream.Create('a.bin', fmCreate);
  WriteLn(F.FileName, ' ', F is THandleStream);
  var B: TBytes := [1, 2, 3, 4, 5, 6, 7, 8];
  F.WriteBuffer(B, 8);
  F.WriteData(Integer(-2));
  Show('written', F);
  F.Position := 2;
  var R: TBytes;
  SetLength(R, 4);
  WriteLn('Read ', F.Read(R, 1, 3), ' ', R[0], ' ', R[1], ' ', R[3]);
  WriteLn('Seek ', F.Seek(-4, TSeekOrigin.soEnd));
  var I := 0;
  F.ReadBufferData(I);
  WriteLn(I, ' ', F.Read(R, 4));
  F.Size := 5;
  Show('cut', F);
  F.Size := 7;
  F.Position := 3;
  F.ReadBuffer(R, 4);
  WriteLn(R[0], ' ', R[1], ' ', R[2], ' ', R[3]);
  F.Close;
  F.Close;
  try
    F.Read(R, 1);
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // Open an existing file to read, write, and both
  var G := TFileStream.Create('a.bin', fmOpenRead or fmShareDenyWrite);
  Show('opened', G);
  try
    G.WriteBuffer(B, 1);
  except
    on E: EWriteError do
      WriteLn(E.ClassName, ' ', E.ErrorKind <> TIOErrorKind.ekNone);
  end;
  G := nil;
  var H := TFileStream.Create('a.bin', fmOpenWrite);
  H.Position := 7;
  H.WriteData(Byte(99));
  H := nil;
  var J := TFileStream.Create('a.bin', fmOpenReadWrite);
  Show('read and write', J);
  J.Position := 7;
  var X: Byte := 0;
  J.ReadData(X);
  WriteLn(X);
  J := nil;

  // Failures of the host
  try
    TFileStream.Create('missing.bin', fmOpenRead);
  except
    on E: EFOpenError do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind = TIOErrorKind.ekNotFound);
  end;
  try
    TFileStream.Create('no/such/dir/x.bin', fmCreate);
  except
    on E: EFCreateError do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind = TIOErrorKind.ekNotFound);
  end;
  try
    TFileStream.Create('a.bin', 3);
  except
    on E: EFOpenError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // A stream in memory to and from a file
  var M := TMemoryStream.Create;
  M.WriteBuffer(B, 8);
  M.SaveToFile('m.bin');
  var N := TMemoryStream.Create;
  N.LoadFromFile('m.bin');
  Show('loaded', N);
  WriteLn(N.ToBytes[7], ' ', FileExists('m.bin'));
end.
