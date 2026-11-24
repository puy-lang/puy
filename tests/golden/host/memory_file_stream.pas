// puy-test: no-fs, clock=100
program MemoryFileStream;

// Handles share the file's bytes but keep their own positions, including after unlinking.
// Read and write modes and read-only attributes are checked by the installed file system.

uses
  System.SysUtils, System.Classes, System.IOUtils, Puy.Testing;

begin
  TFileSystem.Install(TMemoryFileSystem.Create);
  var A := TFileStream.Create('file', fmCreate);
  var B := TFileStream.Create('file', fmOpenRead);
  A.Write([10, 20], 0, 2);
  A.Position := 4;
  A.Write([50], 0, 1);
  var Bytes: TBytes := [99, 99, 99, 99, 99, 99];
  WriteLn('handle: ', A.Handle = nil, ' positions: ', A.Position, ' ', B.Position);
  WriteLn('read: ', B.Read(Bytes, 0, Length(Bytes)), ' ', Bytes[0], ' ', Bytes[1], ' ',
    Bytes[2], ' ', Bytes[3], ' ', Bytes[4], ' ', Bytes[5]);
  B.Seek(-1, TSeekOrigin.soEnd);
  A.Size := 2;
  WriteLn('resized: ', A.Size, ' ', A.Position, ' ', B.Position);
  try
    B.Size := 7;
  except
    on E: EStreamError do
      WriteLn('read-only handle: ', E.ErrorKind);
  end;
  var C := TFileStream.Create('file', fmOpenWrite);
  try
    C.Read(Bytes, 0, 1);
  except
    on E: EReadError do
      WriteLn('write-only handle: ', E.ErrorKind);
  end;
  C.Close;

  TFile.SetAttributes('file', [TFileAttribute.faReadOnly]);
  WriteLn('attribute: ', TFileAttribute.faReadOnly in TFile.GetAttributes('file'));
  try
    A.Write([7], 0, 1);
  except
    on E: EWriteError do
      WriteLn('read-only file: ', E.ErrorKind);
  end;
  TFile.SetAttributes('file', []);
  A.Write([30], 0, 1);

  WriteLn('rename: ', RenameFile('file', 'renamed'), ' ', DeleteFile('renamed'));
  TFile.WriteAllBytes('renamed', [90]);
  TFileSystem.Uninstall;
  B.Position := 2;
  B.Read(Bytes, 0, 1);
  WriteLn('old contents: ', A.Size, ' ', Bytes[0]);
  A.Close;
  B.Close;
end.
