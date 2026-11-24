// puy-test: test-fs, os=windows
program PathsNamesWindows;

// TFilePath on Windows (spec/stdlib/Puy.Paths/README.md): '\' and '/' separate, ':' ends a
// drive, \\server\share is a root; IsAbsolute needs a drive with a separator or a UNC name, and
// Normalize writes '\'.

uses
  System.SysUtils, Puy.Paths;

procedure Show(const What: string; const P: TFilePath);
begin
  WriteLn(What, ': [', P.ToString, ']');
end;

procedure Parts(const S: string);
begin
  var P: TFilePath := S;
  WriteLn('[', S, '] parent [', P.Parent.ToString, '] name [', P.Name, '] stem [', P.Stem,
    '] extension [', P.Extension, ']');
end;

begin
  var Dir: TFilePath := 'C:\src';
  Show('C:\src / a.pas', Dir / 'a.pas');
  Show('C:\src / D:x', Dir / 'D:x');
  Show('C:\src / \x', Dir / '\x');
  Show('a/ / b', TFilePath('a/') / 'b');
  Parts('C:\a\b.txt');
  Parts('C:/a/b.txt');
  Parts('C:\');
  Parts('C:a');
  Parts('C:');
  Parts('\\srv\share\a.txt');
  Parts('\\srv\share\');
  Parts('a\b\');
  WriteLn('IsAbsolute: ', TFilePath('C:\a').IsAbsolute, ' ', TFilePath('C:/a').IsAbsolute, ' ',
    TFilePath('\\srv\share').IsAbsolute, ' ', TFilePath('\a').IsAbsolute, ' ',
    TFilePath('C:a').IsAbsolute, ' ', TFilePath('a').IsAbsolute);
  Show('WithName x.y', TFilePath('a\b').WithName('x.y'));
  try
    TFilePath('a\b').WithName('c:d');
  except
    on E: EArgumentException do
      WriteLn('WithName c:d: ', E.Message);
  end;
  Show('Normalize C:/a/./b/../c', TFilePath('C:/a/./b/../c').Normalize);
  Show('Normalize C:\..\a', TFilePath('C:\..\a').Normalize);
  Show('Normalize C:..\a', TFilePath('C:..\a').Normalize);
  Show('Normalize //srv/share/a/../b', TFilePath('//srv/share/a/../b').Normalize);
  Show('Normalize \\srv', TFilePath('\\srv').Normalize);
  Show('Normalize \a\..\b', TFilePath('\a\..\b').Normalize);
  Show('Normalize a/b\c', TFilePath('a/b\c').Normalize);
end.
