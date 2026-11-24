// puy-test: no-fs, clock=100
program MemoryFileTimes;

// A memory file takes its times from the isolate's clock, including changes made after opening
// a handle. Explicit overrides change the requested time only.

uses
  System.SysUtils, System.IOUtils, Puy.Testing;

procedure ShowTimes(const FS: IFileSystem; const Path: string);
begin
  var Info := FS.GetInfo(Path, True);
  WriteLn(Path, ': ', Info.Created, ' ', Info.Accessed, ' ', Info.Modified);
end;

begin
  var FS: IFileSystem := TMemoryFileSystem.Create;
  TFileSystem.Install(FS);
  ShowTimes(FS, '/');
  TTestClock.Freeze(TInstant.FromUnixNanos(1000));
  var H := FS.OpenFile('file', TFileOpenMode.omCreate);
  ShowTimes(FS, 'file');
  TTestClock.Advance(1);
  H.Write([7], 0, 1);
  ShowTimes(FS, 'file');
  TTestClock.Advance(1);
  H.Seek(0, TFileSeekOrigin.fsoBeginning);
  H.Read(1);
  ShowTimes(FS, 'file');
  FS.SetTime('file', TFileTimeKind.ftCreation, 11);
  FS.SetTime('file', TFileTimeKind.ftLastAccess, 12);
  FS.SetTime('file', TFileTimeKind.ftLastWrite, 13);
  ShowTimes(FS, 'file');
  WriteLn('IOUtils last write: ', TFile.GetLastWriteInstant('file').ToUnixNanos);
  H.Close;

  FS.CreateDirectory('dest', False);
  TTestClock.Advance(1);
  FS.Rename('file', 'dest/file');
  ShowTimes(FS, '/');
  ShowTimes(FS, 'dest');
  ShowTimes(FS, 'dest/file');
end.
