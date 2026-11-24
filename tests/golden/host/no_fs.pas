// puy-test: no-fs
program NoFs;

// Without `Fs` and `Os` (an embedding that granted neither; spec/stdlib/System.SysUtils/Files.md,
// TOSVersion.md), the file functions raise ENotSupportedException with ErrorKind ekNoPermission,
// while the values the OS would give are fixed ones: the working directory '/', the platform
// pfEmbedded and version 0. The file names still work ('/' separates).

uses
  System.SysUtils;

procedure Attempt(const What: string; Op: Integer);
begin
  try
    case Op of
      0: WriteLn(FileExists('a.txt'));
      1: WriteLn(DirectoryExists('a'));
      2: WriteLn(DeleteFile('a.txt'));
      3: WriteLn(RenameFile('a', 'b'));
      4: WriteLn(CreateDir('a'));
      5: WriteLn(ForceDirectories('a/b'));
      6: WriteLn(RemoveDir('a'));
      7: WriteLn(SetCurrentDir('a'));
    end;
  except
    on E: ENotSupportedException do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message, ' (', E.ErrorKind, ')');
  end;
end;

begin
  Attempt('FileExists', 0);
  Attempt('DirectoryExists', 1);
  Attempt('DeleteFile', 2);
  Attempt('RenameFile', 3);
  Attempt('CreateDir', 4);
  Attempt('ForceDirectories', 5);
  Attempt('RemoveDir', 6);
  Attempt('SetCurrentDir', 7);
  WriteLn(GetCurrentDir);
  WriteLn(ExpandFileName('a/../b'));
  WriteLn(TOSVersion.Platform, ' ', TOSVersion.Architecture, ' [', TOSVersion.Name, '] ',
    TOSVersion.Major, ' ', TOSVersion.Check(0), ' ', TOSVersion.Check(1));
  WriteLn(PathDelim, ' [', DriveDelim, '] ', PathSep);
  WriteLn(ExtractFileName('dir\name.txt'));
end.
