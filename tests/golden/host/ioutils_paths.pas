// puy-test: test-fs
program IOUtilsPaths;

// TPath outside Windows (spec/stdlib/System.IOUtils/TPath.md): only '/' separates, '\' and ':'
// are parts of names; the runner's fixed OS gives /tmp, /home/puy and the working directory
// /work. ioutils_paths_windows has the same calls with Windows's rules.

uses
  System.SysUtils, System.IOUtils;

procedure Show(const What, Value: string);
begin
  WriteLn(What, ': [', Value, ']');
end;

begin
  WriteLn(TPath.DirectorySeparatorChar, ' ', TPath.AltDirectorySeparatorChar, ' ',
    TPath.VolumeSeparatorChar, ' ', TPath.PathSeparator, ' ', TPath.ExtensionSeparatorChar);
  Show('Combine a b', TPath.Combine('a', 'b'));
  Show('Combine a/ b', TPath.Combine('a/', 'b'));
  Show('Combine a /b', TPath.Combine('a', '/b'));
  Show('Combine a C:\b', TPath.Combine('a', 'C:\b'));
  Show('Combine a b c', TPath.Combine('a', 'b', 'c'));
  Show('GetDirectoryName /a/b.txt', TPath.GetDirectoryName('/a/b.txt'));
  Show('GetDirectoryName /a', TPath.GetDirectoryName('/a'));
  Show('GetDirectoryName a\b.txt', TPath.GetDirectoryName('a\b.txt'));
  Show('GetFileName /a/b.tar.gz', TPath.GetFileName('/a/b.tar.gz'));
  Show('GetFileNameWithoutExtension /a.d/b', TPath.GetFileNameWithoutExtension('/a.d/b'));
  Show('GetExtension /a.d/b', TPath.GetExtension('/a.d/b'));
  Show('ChangeExtension /a.d/b', TPath.ChangeExtension('/a.d/b', 'x'));
  Show('ChangeExtension a.txt .', TPath.ChangeExtension('a.txt', '.'));
  WriteLn('IsPathRooted: ', TPath.IsPathRooted('/a'), ' ', TPath.IsPathRooted('a'), ' ',
    TPath.IsPathRooted('C:\a'), ' ', TPath.IsPathRooted('\a'));
  Show('GetPathRoot /a/b', TPath.GetPathRoot('/a/b'));
  Show('GetPathRoot C:\a', TPath.GetPathRoot('C:\a'));
  Show('GetFullPath a/../b', TPath.GetFullPath('a/../b'));
  Show('GetTempPath', TPath.GetTempPath);
  Show('GetHomePath', TPath.GetHomePath);
end.
