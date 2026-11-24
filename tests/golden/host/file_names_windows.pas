// puy-test: test-fs, os=windows
program FileNamesWindows;

// The file name functions of System.SysUtils (spec/stdlib/System.SysUtils/Files.md) on Windows
// (the runner's OS reports `windows`): '\' and '/' separate, ':' ends a drive, and a name that
// starts with two separators has a `\\server\share` drive.

uses
  System.SysUtils;

procedure Show(const F: string);
begin
  WriteLn('[', F, '] path=[', ExtractFilePath(F), '] dir=[', ExtractFileDir(F), '] name=[',
    ExtractFileName(F), '] ext=[', ExtractFileExt(F), '] drive=[', ExtractFileDrive(F), ']');
end;

begin
  WriteLn('[', PathDelim, '] [', DriveDelim, '] [', PathSep, ']');
  Show('/home/puy/a.txt');
  Show('rel/dir/archive.tar.gz');
  Show('/');
  Show('/a');
  Show('a/');
  Show('//a');
  Show('.bashrc');
  Show('dir.d/noext');
  Show('C:\dir\name.txt');
  Show('C:');
  Show('C:\');
  Show('C:name');
  Show('\\server\share\dir\f.txt');
  Show('\\server\share');
  Show('\\server');
  Show('mixed/sep\name');
  Show('');
  WriteLn(ChangeFileExt('a/b.txt', '.pas'), ' ', ChangeFileExt('a.d/b', '.x'),
    ' ', ChangeFileExt('a.txt', ''));
  WriteLn(ChangeFilePath('x/y/name.txt', '/new'), ' ', ChangeFilePath('name.txt', 'dir/'));
  WriteLn(IncludeTrailingPathDelimiter('a'), ' ', IncludeTrailingPathDelimiter('a/'), ' [',
    IncludeTrailingPathDelimiter(''), ']');
  WriteLn(ExcludeTrailingPathDelimiter('a/'), ' ', ExcludeTrailingPathDelimiter('a//'), ' [',
    ExcludeTrailingPathDelimiter('/'), ']');
  WriteLn(IsPathDelimiter('a/b', 2), ' ', IsPathDelimiter('a\b', 2), ' ', IsPathDelimiter('a', 0),
    ' ', IsPathDelimiter('a', 5));
  WriteLn(LastDelimiter('/.', 'a/b.c/d'), ' ', LastDelimiter('xyz', 'abc'), ' ',
    LastDelimiter('', 'abc'));
  WriteLn(ExpandFileName('C:\a\..\b\.\c'), ' ', ExpandFileName('C:x'), ' ',
    ExpandFileName('\\srv\sh\a\..\b'));
  WriteLn(ExpandFileName('\\server'), ' ', ExpandFileName('\top'));
end.
