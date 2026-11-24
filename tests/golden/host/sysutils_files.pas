// puy-test: test-fs
program SysUtilsFiles;

// The file functions of System.SysUtils (spec/stdlib/System.SysUtils/Files.md) on the runner's
// file system in memory (working directory /work): each failure of the host is False, as in
// Delphi; the working directory, ExpandFileName and TOSVersion with the runner's fixed OS.

uses
  System.SysUtils;

procedure Show(const What: string; B: Boolean);
begin
  WriteLn(What, ': ', B);
end;

begin
  WriteLn(GetCurrentDir);
  Show('CreateDir a', CreateDir('a'));
  Show('CreateDir a again', CreateDir('a'));
  Show('CreateDir x/y (no parent)', CreateDir('x/y'));
  Show('ForceDirectories a/b/c', ForceDirectories('a/b/c'));
  Show('ForceDirectories a/b/c again', ForceDirectories('a/b/c'));
  Show('ForceDirectories /work/d/', ForceDirectories('/work/d/'));
  Show('ForceDirectories empty', ForceDirectories(''));
  Show('DirectoryExists a/b', DirectoryExists('a/b'));
  Show('DirectoryExists /work/a/b/c', DirectoryExists('/work/a/b/c'));
  Show('FileExists a/b', FileExists('a/b'));
  Show('FileExists missing', FileExists('missing.txt'));
  Show('DirectoryExists empty', DirectoryExists(''));
  Show('RemoveDir a (not empty)', RemoveDir('a'));
  Show('DeleteFile a/b/c (a directory)', DeleteFile('a/b/c'));
  Show('DeleteFile missing', DeleteFile('missing.txt'));
  Show('RenameFile a -> e', RenameFile('a', 'e'));
  Show('DirectoryExists e/b/c', DirectoryExists('e/b/c'));
  Show('DirectoryExists a', DirectoryExists('a'));
  Show('RenameFile missing', RenameFile('missing', 'f'));
  Show('RemoveDir e/b/c', RemoveDir('e/b/c'));
  Show('RemoveDir e/b/c again', RemoveDir('e/b/c'));

  Show('SetCurrentDir e/b', SetCurrentDir('e/b'));
  WriteLn(GetCurrentDir);
  WriteLn(ExpandFileName('x.txt'));
  WriteLn(ExpandFileName('../../y/./z.txt'));
  WriteLn(ExpandFileName('/a/../../b/'));
  WriteLn(ExpandFileName('..'));
  Show('SetCurrentDir missing', SetCurrentDir('missing'));
  Show('SetCurrentDir ..', SetCurrentDir('..'));
  WriteLn(GetCurrentDir);

  WriteLn(TOSVersion.ToString);
  WriteLn(TOSVersion.Platform, ' ', TOSVersion.Architecture, ' ', TOSVersion.Name, ' ',
    TOSVersion.Major, '.', TOSVersion.Minor, '.', TOSVersion.Build, ' ',
    TOSVersion.ServicePackMajor, ' ', TOSVersion.ServicePackMinor);
  WriteLn(TOSVersion.Check(1), ' ', TOSVersion.Check(2), ' ', TOSVersion.Check(1, 2), ' ',
    TOSVersion.Check(1, 3), ' ', TOSVersion.Check(1, 2, 0), ' ', TOSVersion.Check(1, 2, 1), ' ',
    TOSVersion.Check(0, 9, 9));
  if TOSVersion.Platform = TOSVersion.TPlatform.pfLinux then
    WriteLn('on Linux');
end.
