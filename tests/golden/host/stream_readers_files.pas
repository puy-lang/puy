// puy-test: test-fs
program StreamReadersFiles;

// The readers and writers on files of the runner's file system in memory
// (spec/stdlib/System.Classes/Readers.md): TStreamWriter by file name buffers until Close,
// Append writes at the end with no second preamble, TStreamReader by file name detects the
// BOM, TBinaryWriter / TBinaryReader by file name, and a missing file.

uses
  System.SysUtils, System.Classes;

procedure ShowFile(const FileName: string);
begin
  var F := TFileStream.Create(FileName, fmOpenRead);
  var B: TBytes;
  SetLength(B, F.Size);
  F.ReadBuffer(B, Length(B));
  for var I := 0 to High(B) do
    Write(IntToHex(B[I], 2), ' ');
  WriteLn('(', Length(B), ')');
end;

begin
  // A writer on a file name buffers: the file is empty until Close
  var W := TStreamWriter.Create('a.txt');
  WriteLn(W.AutoFlush, ' ', W.BaseStream is TFileStream);
  W.WriteLine('one');
  W.Write('twö');
  ShowFile('a.txt');
  W.Close;
  ShowFile('a.txt');

  // Append: at the end, and the preamble only for a new file
  W := TStreamWriter.Create('b.txt', True, TEncoding.UTF8);
  W.WriteLine('first');
  W := nil;
  W := TStreamWriter.Create('b.txt', True, TEncoding.UTF8);
  W.WriteLine('second');
  W := nil;
  ShowFile('b.txt');
  // Without Append the file starts again
  W := TStreamWriter.Create('b.txt', False, TEncoding.Unicode, 2);
  W.Write('hi');
  W := nil;
  ShowFile('b.txt');

  // A reader on a file name detects the BOM
  var R := TStreamReader.Create('b.txt');
  WriteLn(R.ReadLine, ' ', R.CurrentEncoding.EncodingName, ' ', R.EndOfStream);
  R := TStreamReader.Create('a.txt', TEncoding.Default);
  while not R.EndOfStream do
    WriteLn('[', R.ReadLine, ']');
  R.Close;
  R := TStreamReader.Create('a.txt', False);
  WriteLn(R.ReadToEnd.Length);

  // TBinaryWriter / TBinaryReader by file name
  var BW := TBinaryWriter.Create('c.bin');
  BW.Write(Integer(258));
  BW.Write('ok');
  BW.Close;
  BW := TBinaryWriter.Create('c.bin', True);
  BW.Write(Byte(9));
  BW := nil;
  ShowFile('c.bin');
  var BR := TBinaryReader.Create('c.bin');
  WriteLn(BR.ReadInt32, ' ', BR.ReadString, ' ', BR.ReadByte);

  // A missing file
  try
    TStreamReader.Create('missing.txt');
  except
    on E: EFOpenError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    TBinaryReader.Create('missing.bin');
  except
    on E: EFOpenError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    TStreamWriter.Create('no/dir/x.txt');
  except
    on E: EFCreateError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
