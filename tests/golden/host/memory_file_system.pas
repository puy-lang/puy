// puy-test: no-fs, clock=42
program MemoryFileSystem;

// The installed Pascal file system supplies the standard path and directory operations with no
// host Fs or Os, including aliases of the same path and copying a directory tree.

uses
  System.SysUtils, System.IOUtils, Puy.Paths, Puy.Testing;

procedure Show(const Names: TArray<string>);
begin
  for var Name in Names do
    WriteLn(Name);
end;

begin
  var FS: IFileSystem := TMemoryFileSystem.Create('/work/session');
  TFileSystem.Install(FS);
  WriteLn('isolated cwd: ', DirectoryExists('.'), ' ', TFilePath('.').Resolve.ToString);
  WriteLn('root: ', TFilePath('/../../../').Resolve.ToString);
  WriteLn('create: ', CreateDir('docs'), ' ', CreateDir('./docs'));
  WriteLn('parents: ', CreateDir('missing/child'), ' ', ForceDirectories('docs/sub/deep'));
  TFile.WriteAllText('docs/z.txt', 'z');
  TFile.WriteAllText('docs/a.txt', 'hello');
  WriteLn('resolve: ', TFilePath('docs//absent/../sub\..\./a.txt/').Resolve.ToString);
  WriteLn('path read: ', TFilePath('.\docs\a.txt').ReadAllText);
  WriteLn('case: ', FileExists('docs/A.txt'));
  Show(TDirectory.GetFiles('docs'));

  TDirectory.Copy('docs', '/work/session/second');
  TDirectory.Move('second', 'third');
  TFile.WriteAllText('third/a.txt', 'different');
  WriteLn('copy: ', TFile.ReadAllText('docs/a.txt'), ' ', TFile.ReadAllText('third/a.txt'));
  TFile.Copy('docs/a.txt', 'third/a.txt', True);
  WriteLn('overwrite: ', TFile.ReadAllText('third/a.txt'));
  WriteLn('rename collision: ', RenameFile('third/a.txt', 'third/z.txt'));
  WriteLn('rename into itself: ', RenameFile('third', 'third/sub/tree'));
  WriteLn('renamed tree: ', DirectoryExists('third/sub/deep'));
  try
    TDirectory.Copy('docs', '\work\session\docs\new\deep');
  except
    on EArgumentException do
      WriteLn('copy into itself rejected: ', not DirectoryExists('docs/new'));
  end;
  WriteLn('delete: ', DeleteFile('third/z.txt'), ' ', RemoveDir('third'), ' ', RemoveDir('/'));
  TDirectory.Delete('third', True);
  WriteLn('deleted tree: ', not DirectoryExists('third'));
  try
    TFileSystem.Host.GetInfo('/work/session/docs/a.txt', True);
  except
    on E: ENotSupportedException do
      WriteLn('host unavailable: ', E.ErrorKind);
  end;
  TFileSystem.Uninstall;
  WriteLn('uninstalled: ', TFileSystem.Installed = nil);
end.
