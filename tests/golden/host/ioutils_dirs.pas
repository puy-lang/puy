// puy-test: test-fs
program IOUtilsDirs;

// TDirectory on the runner's file system in memory (working directory /work;
// spec/stdlib/System.IOUtils/TDirectory.md): the listings sorted by name with a directory's
// entries before its subdirectories', the search patterns (case kept apart outside Windows),
// Copy / Move / Delete of whole trees and the working directory.

uses
  System.SysUtils, System.IOUtils;

procedure Show(const What: string; const A: TArray<string>);
begin
  Write(What, ':');
  for var S in A do
    Write(' ', S);
  WriteLn;
end;

begin
  TDirectory.CreateDirectory('t/sub/deep');
  TDirectory.CreateDirectory('t/sub/deep');
  TDirectory.CreateDirectory('t/Abc');
  TFile.WriteAllText('t/b.txt', '');
  TFile.WriteAllText('t/a.TXT', '');
  TFile.WriteAllText('t/c.txt.bak', '');
  TFile.WriteAllText('t/noext', '');
  TFile.WriteAllText('t/sub/x.txt', '');
  TFile.WriteAllText('t/sub/deep/y.txt', '');
  TFile.WriteAllText('t/Abc/z.txt', '');
  Show('GetFiles', TDirectory.GetFiles('t'));
  Show('GetFiles *.txt', TDirectory.GetFiles('t', '*.txt'));
  Show('GetFiles *.TXT', TDirectory.GetFiles('t', '*.TXT'));
  Show('GetFiles ?.txt', TDirectory.GetFiles('t', '?.txt'));
  Show('GetFiles *.*', TDirectory.GetFiles('t', '*.*'));
  Show('GetFiles *t*', TDirectory.GetFiles('t', '*t*'));
  Show('GetFiles all *.txt', TDirectory.GetFiles('t', '*.txt', TSearchOption.soAllDirectories));
  Show('GetFiles all', TDirectory.GetFiles('t', TSearchOption.soAllDirectories));
  Show('GetDirectories', TDirectory.GetDirectories('t'));
  Show('GetDirectories all', TDirectory.GetDirectories('t', TSearchOption.soAllDirectories));
  Show('GetDirectories s*', TDirectory.GetDirectories('t', 's*'));
  Show('GetFileSystemEntries', TDirectory.GetFileSystemEntries('t'));
  Show('GetFileSystemEntries /work/t/sub', TDirectory.GetFileSystemEntries('/work/t/sub'));
  WriteLn('IsEmpty: ', TDirectory.IsEmpty('t'), ' ', TDirectory.IsEmpty('t/sub/deep'));
  TDirectory.CreateDirectory('empty');
  WriteLn('IsEmpty empty: ', TDirectory.IsEmpty('empty'));
  WriteLn('Exists: ', TDirectory.Exists('t/sub'), ' ', TDirectory.Exists('t/b.txt'), ' ',
    TDirectory.Exists('missing'));

  TDirectory.Copy('t', 'u');
  Show('Copy', TDirectory.GetFileSystemEntries('u', TSearchOption.soAllDirectories));
  TDirectory.Move('u', 'v');
  WriteLn('Move: ', TDirectory.Exists('u'), ' ', TDirectory.Exists('v/sub/deep'));
  TDirectory.Delete('empty');
  TDirectory.Delete('v', True);
  WriteLn('Delete: ', TDirectory.Exists('empty'), ' ', TDirectory.Exists('v'));

  WriteLn(TDirectory.GetCurrentDirectory);
  TDirectory.SetCurrentDirectory('t/sub');
  WriteLn(TDirectory.GetCurrentDirectory);
  Show('relative', TDirectory.GetFiles('.'));
  Show('parent', TDirectory.GetDirectories('..'));
end.
