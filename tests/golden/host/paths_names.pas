// puy-test: test-fs
program PathsNames;

// TFilePath taken apart and put together outside Windows (spec/stdlib/Puy.Paths/README.md):
// `/` and Join as TPath.Combine, Parent / Name / Stem / Extension with the separators at the
// end left out, WithName / WithExtension, IsAbsolute, and Normalize by name only.

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

procedure Attempt(const What: string; Op: Integer);
begin
  var P: TFilePath := 'a/b.txt';
  try
    case Op of
      0: P := TFilePath('/').WithName('x');
      1: P := P.WithName('');
      2: P := P.WithName('c/d');
      3: P := P.WithName('..');
      4: P := TFilePath('').WithExtension('.x');
      5: P := P.WithExtension('.');
      6: P := P.WithExtension('x/y');
      7: P := TFilePath('a/.bashrc').WithExtension('');
    end;
    WriteLn(What, ': [', P.ToString, ']');
  except
    on E: EArgumentException do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  var Dir: TFilePath := 'src';
  Show('src / main.pas', Dir / 'main.pas');
  Show('src / lib / a.pas', Dir / 'lib' / 'a.pas');
  Show('src/ / a', TFilePath('src/') / 'a');
  Show('src / /etc', Dir / '/etc');
  Show('src / empty', Dir / '');
  Show('empty / a', TFilePath('') / 'a');
  Show('string / path', 'top' / Dir);
  Show('path / path', Dir / TFilePath('b'));
  Show('Join', Dir.Join('x').Join('y.txt'));
  Parts('a/b/c.txt');
  Parts('a/b/c.tar.gz');
  Parts('a/b/');
  Parts('a//b//');
  Parts('c.txt');
  Parts('/a');
  Parts('/');
  Parts('');
  Parts('.bashrc');
  Parts('a.');
  Parts('.');
  Parts('..');
  Parts('a\b.txt');
  Show('WithName', TFilePath('a/b.txt').WithName('c.pas'));
  Show('WithName trailing', TFilePath('a/b/').WithName('c'));
  Show('WithExtension pas', TFilePath('a/b.txt').WithExtension('pas'));
  Show('WithExtension .pas', TFilePath('a/b.txt').WithExtension('.pas'));
  Show('WithExtension empty', TFilePath('a/b.tar.gz').WithExtension(''));
  Show('WithExtension none', TFilePath('a.d/b').WithExtension('.x'));
  Attempt('WithName of /', 0);
  Attempt('WithName empty', 1);
  Attempt('WithName c/d', 2);
  Attempt('WithName ..', 3);
  Attempt('WithExtension of empty', 4);
  Attempt('WithExtension .', 5);
  Attempt('WithExtension x/y', 6);
  Attempt('WithExtension empty of .bashrc', 7);
  Show('WithExtension txt of .bashrc', TFilePath('a/.bashrc').WithExtension('txt'));
  WriteLn('IsAbsolute: ', TFilePath('/a').IsAbsolute, ' ', TFilePath('a').IsAbsolute, ' ',
    TFilePath('\a').IsAbsolute, ' ', TFilePath('C:/a').IsAbsolute);
  Show('Normalize a/./b/../c//', TFilePath('a/./b/../c//').Normalize);
  Show('Normalize ../a/../../b', TFilePath('../a/../../b').Normalize);
  Show('Normalize /../a/..', TFilePath('/../a/..').Normalize);
  Show('Normalize //a///b', TFilePath('//a///b').Normalize);
  Show('Normalize a/..', TFilePath('a/..').Normalize);
  Show('Normalize ./', TFilePath('./').Normalize);
  Show('Normalize empty', TFilePath('').Normalize);
  Show('Normalize a\..\b', TFilePath('a\..\b').Normalize);
  Show('kept as written', TFilePath('a//./b/'));
end.
