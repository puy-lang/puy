// puy-test: test-fs
program PathsFiles;

// TFilePath with the runner's file system in memory (working directory /work;
// spec/stdlib/Puy.Paths/README.md): reading and writing through System.IOUtils, Exists / IsFile /
// IsDirectory, the listings as paths, Resolve (absolute, `.` and `..` resolved, the path must
// exist), the implicit conversions to and from string, and paths as the keys of a dictionary.

uses
  System.SysUtils, System.IOUtils, System.Generics.Collections, Puy.Paths;

procedure Show(const What: string; const A: TArray<TFilePath>);
begin
  Write(What, ':');
  for var P in A do
    Write(' ', P.ToString);
  WriteLn;
end;

procedure Attempt(const What: string; Op: Integer);
begin
  try
    case Op of
      0: TFilePath('missing/x').Resolve;
      1: TFilePath('').Resolve;
      2: TFilePath('missing.txt').ReadAllText;
    end;
    WriteLn(What, ': no exception');
  except
    on E: EInOutError do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message, ' (', E.ErrorKind, ')');
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  var Root: TFilePath := 'proj';
  (Root / 'src' / 'lib').CreateDirectory;
  (Root / 'src' / 'main.pas').WriteAllText('program Main;');
  (Root / 'src' / 'lib' / 'a.pas').WriteAllLines(['unit A;', 'end.']);
  (Root / 'data.bin').WriteAllBytes([1, 2, 3]);
  (Root / 'README.md').WriteAllText('héllo', TEncoding.UTF8);
  WriteLn((Root / 'src' / 'main.pas').ReadAllText);
  WriteLn(Length((Root / 'src' / 'lib' / 'a.pas').ReadAllLines));
  WriteLn(Length((Root / 'data.bin').ReadAllBytes));
  WriteLn((Root / 'README.md').ReadAllText(TEncoding.UTF8));
  WriteLn('Exists: ', Root.Exists, ' ', (Root / 'data.bin').Exists, ' ',
    (Root / 'none').Exists, ' ', TFilePath('').Exists);
  WriteLn('IsFile: ', Root.IsFile, ' ', (Root / 'data.bin').IsFile);
  WriteLn('IsDirectory: ', Root.IsDirectory, ' ', (Root / 'data.bin').IsDirectory);
  Show('Entries', Root.Entries);
  Show('Entries *.md', Root.Entries('*.md'));
  Show('Files', Root.Files);
  Show('Files all *.pas', Root.Files('*.pas', TSearchOption.soAllDirectories));
  Show('Files all', Root.Files(TSearchOption.soAllDirectories));
  Show('Directories', Root.Directories);
  Show('Directories all', Root.Directories(TSearchOption.soAllDirectories));
  Show('Directories s*', Root.Directories('s*'));
  Show('Entries all', Root.Entries(TSearchOption.soAllDirectories));
  Show('Entries all *.pas', Root.Entries('*.pas', TSearchOption.soAllDirectories));
  WriteLn('Resolve: ', TFilePath('proj/src/../data.bin').Resolve.ToString);
  WriteLn('Resolve .: ', TFilePath('.').Resolve.ToString);
  Attempt('Resolve missing', 0);
  Attempt('Resolve empty', 1);
  Attempt('ReadAllText missing', 2);

  // Strings and paths convert both ways
  var S: string := Root / 'data.bin';
  WriteLn(S, ' ', TFile.Exists(Root / 'data.bin'), ' ', string(Root), ' ', Length(Root.ToString));
  // Equal as strings: the same file written two ways is two paths
  WriteLn('=: ', TFilePath('a/b') = 'a/b', ' ', TFilePath('a/b') = TFilePath('a//b'), ' ',
    TFilePath('a/b') <> 'a/b/', ' ', TFilePath('a//b').Normalize = TFilePath('a/b'));
  var Sizes := TDictionary<TFilePath, Integer>.Create;
  for var P in Root.Files(TSearchOption.soAllDirectories) do
    Sizes.Add(P, Length(P.ReadAllBytes));
  WriteLn(Sizes.Count, ' ', Sizes[Root / 'data.bin'], ' ', Sizes.ContainsKey('proj/data.bin'),
    ' ', Sizes.ContainsKey('proj//data.bin'));
  var Seen := THashSet<TFilePath>.Create;
  Seen.Add('x');
  Seen.Add(TFilePath('x'));
  Seen.Add('y');
  WriteLn(Seen.Count, ' ', TFilePath('x').GetHashCode = TFilePath('x').GetHashCode);
  Sizes.Free;
  Seen.Free;
end.
