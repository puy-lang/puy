unit Puy.Paths;

// The path type TFilePath (docs/spec/stdlib/Puy.Paths/README.md): a path as a value that holds
// the string it was given, joined with `/`, taken apart by the target's rules (found at run time,
// as System.SysUtils's file names), and compared as a string. The files and directories it names
// are read and listed through System.IOUtils; only Resolve asks the host through
// Puy.Paths.Native. Written from the specification only, not from any other implementation.

interface

uses
  System.SysUtils,
  System.IOUtils;

type
  // A path. The value is the string it was made from; Normalize and Resolve make new values.
  // Two paths are equal when their strings are (on every target).
  TFilePath = record
  private
    FPath: string;
    function Trimmed: string;
    function NameOrFail(const Op: string): string;
    class function Make(const S: string): TFilePath; static;
    class function ToPaths(const Names: TArray<string>): TArray<TFilePath>; static;
  public
    function ToString: string;
    function IsEmpty: Boolean;

    // Joining and taking apart (by name only; the file system is not asked)
    function Join(const Other: TFilePath): TFilePath;
    function Parent: TFilePath;
    function Name: string;
    function Stem: string;
    function Extension: string;
    function WithName(const AName: string): TFilePath;
    function WithExtension(const AExtension: string): TFilePath;
    function IsAbsolute: Boolean;
    function Normalize: TFilePath;
    // The absolute path with the symbolic links resolved; the path must exist
    function Resolve: TFilePath;

    // The file system (System.IOUtils)
    function Exists: Boolean;
    function IsFile: Boolean;
    function IsDirectory: Boolean;
    function ReadAllText: string; overload;
    function ReadAllText(const Encoding: TEncoding): string; overload;
    procedure WriteAllText(const Contents: string); overload;
    procedure WriteAllText(const Contents: string; const Encoding: TEncoding); overload;
    function ReadAllBytes: TBytes;
    procedure WriteAllBytes(const Bytes: TBytes);
    function ReadAllLines: TArray<string>; overload;
    function ReadAllLines(const Encoding: TEncoding): TArray<string>; overload;
    procedure WriteAllLines(const Contents: TArray<string>); overload;
    procedure WriteAllLines(const Contents: TArray<string>; const Encoding: TEncoding); overload;
    procedure CreateDirectory;
    // The entries of the directory, as TDirectory lists them (sorted by name)
    function Entries: TArray<TFilePath>; overload;
    function Entries(const Pattern: string): TArray<TFilePath>; overload;
    function Entries(const Option: TSearchOption): TArray<TFilePath>; overload;
    function Entries(const Pattern: string; const Option: TSearchOption): TArray<TFilePath>;
      overload;
    function Files: TArray<TFilePath>; overload;
    function Files(const Pattern: string): TArray<TFilePath>; overload;
    function Files(const Option: TSearchOption): TArray<TFilePath>; overload;
    function Files(const Pattern: string; const Option: TSearchOption): TArray<TFilePath>;
      overload;
    function Directories: TArray<TFilePath>; overload;
    function Directories(const Pattern: string): TArray<TFilePath>; overload;
    function Directories(const Option: TSearchOption): TArray<TFilePath>; overload;
    function Directories(const Pattern: string; const Option: TSearchOption): TArray<TFilePath>;
      overload;

    function GetHashCode: Int64;

    class operator Implicit(const S: string): TFilePath;
    class operator Implicit(const P: TFilePath): string;
    class operator Divide(const A, B: TFilePath): TFilePath;
    class operator Divide(const A: TFilePath; const B: string): TFilePath;
    class operator Divide(const A: string; const B: TFilePath): TFilePath;
    class operator Equal(const A, B: TFilePath): Boolean;
    class operator NotEqual(const A, B: TFilePath): Boolean;
  end;

implementation

uses
  // The Rust part of the unit (crates/puy-stdlib-ext; docs/design/stdlib.md 3.1)
  Puy.Paths.Native;

// Path with every symbolic link, `.` and `..` resolved by the host's file system, or by the one
// a program installed (whose exceptions pass through).
function FsCanonicalPath(const Path: string): string;
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    Result := CanonicalPath(Path)
  else
    Result := FS.Canonicalize(Path);
end;

function OnWindows: Boolean;
begin
  Result := TOSVersion.Platform = TOSVersion.TPlatform.pfWindows;
end;

// The root of S: TPath.GetPathRoot, except that two separators with no share after them (`\\`,
// `\\server`) are the root `\\` on Windows, as ExpandFileName takes them.
function RootOf(const S: string): string;
begin
  Result := TPath.GetPathRoot(S);
  if OnWindows and (Length(Result) = 1) and IsPathDelimiter(S, 2) then
    Result := Copy(S, 1, 2);
end;

// Whether N can be one name of a path: not empty, not `.` or `..`, with no separator (or `:` on
// Windows).
function IsOneName(const N: string): Boolean;
begin
  Result := (N <> '') and (N <> '.') and (N <> '..') and (ExtractFileName(N) = N);
end;

class function TFilePath.Make(const S: string): TFilePath;
begin
  Result := Default(TFilePath);
  Result.FPath := S;
end;

class function TFilePath.ToPaths(const Names: TArray<string>): TArray<TFilePath>;
begin
  SetLength(Result, Length(Names));
  for var I := 0 to High(Names) do
    Result[I] := Make(Names[I]);
end;

// The path without the separators at its end (those of the root stay).
function TFilePath.Trimmed: string;
begin
  Result := FPath;
  var Keep := Length(RootOf(Result));
  while (Length(Result) > Keep) and IsPathDelimiter(Result, Length(Result)) do
    SetLength(Result, Length(Result) - 1);
end;

// The name, or EArgumentException for a path that has none (`/`, ``, `.`, `..`) when the
// operation Op makes a new one.
function TFilePath.NameOrFail(const Op: string): string;
begin
  Result := Name;
  if not IsOneName(Result) then
    raise EArgumentException.Create('cannot ' + Op + ' of ''' + FPath + ''': it has no name');
end;

function TFilePath.ToString: string;
begin
  Result := FPath;
end;

function TFilePath.IsEmpty: Boolean;
begin
  Result := FPath = '';
end;

function TFilePath.Join(const Other: TFilePath): TFilePath;
begin
  Result := Make(TPath.Combine(FPath, Other.FPath));
end;

function TFilePath.Parent: TFilePath;
begin
  var T := Trimmed;
  if ExtractFileName(T) = '' then
    // A root (or the empty path) is its own parent
    Exit(Make(T));
  var D := Make(ExtractFileDir(T)).Trimmed;
  if D = '' then
    D := '.';
  Result := Make(D);
end;

function TFilePath.Name: string;
begin
  Result := ExtractFileName(Trimmed);
end;

function TFilePath.Extension: string;
begin
  var N := Name;
  if (N = '.') or (N = '..') then
    Result := ''
  else
    Result := ExtractFileExt(N);
end;

function TFilePath.Stem: string;
begin
  var N := Name;
  Result := Copy(N, 1, Length(N) - Length(Extension));
end;

function TFilePath.WithName(const AName: string): TFilePath;
begin
  var N := NameOrFail('change the name');
  if not IsOneName(AName) then
    raise EArgumentException.Create('invalid file name ''' + AName + '''');
  var T := Trimmed;
  Result := Make(Copy(T, 1, Length(T) - Length(N)) + AName);
end;

function TFilePath.WithExtension(const AExtension: string): TFilePath;
begin
  NameOrFail('change the extension');
  var E := AExtension;
  if (E <> '') and (E[1] <> '.') then
    E := '.' + E;
  if (E = '.') or (ExtractFileName(E) <> E) then
    raise EArgumentException.Create('invalid extension ''' + AExtension + '''');
  // A name that is all extension (.bashrc) would be left with no name
  if (E = '') and (Stem = '') then
    raise EArgumentException.Create('cannot remove the extension of ''' + FPath +
      ''': it is the whole name');
  var T := Trimmed;
  Result := Make(Copy(T, 1, Length(T) - Length(Extension)) + E);
end;

function TFilePath.IsAbsolute: Boolean;
begin
  if OnWindows then
    Result := (IsPathDelimiter(FPath, 1) and IsPathDelimiter(FPath, 2))
      or ((Length(FPath) >= 3) and (FPath[2] = ':') and IsPathDelimiter(FPath, 3))
  else
    Result := IsPathDelimiter(FPath, 1);
end;

function TFilePath.Normalize: TFilePath;
begin
  if FPath = '' then
    Exit(Self);
  var Root := RootOf(FPath);
  var Rest := Copy(FPath, Length(Root) + 1, MaxInt);
  // A root with a separator (`/`, `C:\`, `\\server\share`) has nothing above it; `C:` (the
  // working directory of the drive) does
  var Anchored := (Root <> '') and (IsPathDelimiter(Root, 1) or
    IsPathDelimiter(Root, Length(Root)));
  if OnWindows then
    Root := StringReplace(Root, '/', '\', [TReplaceFlag.rfReplaceAll]);
  // The names of Rest, with `.` dropped and `..` taking off the name before it
  var Parts: TArray<string> := nil;
  var Part := '';
  for var I := 1 to Length(Rest) + 1 do
    if (I > Length(Rest)) or IsPathDelimiter(Rest, I) then
    begin
      if Part = '..' then
      begin
        if (Length(Parts) > 0) and (Parts[High(Parts)] <> '..') then
          SetLength(Parts, Length(Parts) - 1)
        else if not Anchored then
          Parts := Parts + ['..'];
      end
      else if (Part <> '') and (Part <> '.') then
        Parts := Parts + [Part];
      Part := '';
    end
    else
      Part := Part + Rest[I];
  var S := Root;
  for var I := 0 to High(Parts) do
  begin
    if I > 0 then
      S := S + PathDelim;
    S := S + Parts[I];
  end;
  if S = '' then
    S := '.';
  Result := Make(S);
end;

function TFilePath.Resolve: TFilePath;
begin
  if FPath = '' then
    raise EArgumentException.Create('the path is empty');
  try
    Result := Make(FsCanonicalPath(FPath));
  except
    on E: EInOutError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
      begin
        var F := EFileNotFoundException.Create('cannot resolve path ''' + FPath +
          ''': file not found');
        F.ErrorKind := TIOErrorKind.ekNotFound;
        F.ErrorCode := E.ErrorCode;
        raise F;
      end
      else
        raise;
  end;
end;

function TFilePath.Exists: Boolean;
begin
  Result := TFile.Exists(FPath) or TDirectory.Exists(FPath);
end;

function TFilePath.IsFile: Boolean;
begin
  Result := TFile.Exists(FPath);
end;

function TFilePath.IsDirectory: Boolean;
begin
  Result := TDirectory.Exists(FPath);
end;

function TFilePath.ReadAllText: string;
begin
  Result := TFile.ReadAllText(FPath);
end;

function TFilePath.ReadAllText(const Encoding: TEncoding): string;
begin
  Result := TFile.ReadAllText(FPath, Encoding);
end;

procedure TFilePath.WriteAllText(const Contents: string);
begin
  TFile.WriteAllText(FPath, Contents);
end;

procedure TFilePath.WriteAllText(const Contents: string; const Encoding: TEncoding);
begin
  TFile.WriteAllText(FPath, Contents, Encoding);
end;

function TFilePath.ReadAllBytes: TBytes;
begin
  Result := TFile.ReadAllBytes(FPath);
end;

procedure TFilePath.WriteAllBytes(const Bytes: TBytes);
begin
  TFile.WriteAllBytes(FPath, Bytes);
end;

function TFilePath.ReadAllLines: TArray<string>;
begin
  Result := TFile.ReadAllLines(FPath);
end;

function TFilePath.ReadAllLines(const Encoding: TEncoding): TArray<string>;
begin
  Result := TFile.ReadAllLines(FPath, Encoding);
end;

procedure TFilePath.WriteAllLines(const Contents: TArray<string>);
begin
  TFile.WriteAllLines(FPath, Contents);
end;

procedure TFilePath.WriteAllLines(const Contents: TArray<string>; const Encoding: TEncoding);
begin
  TFile.WriteAllLines(FPath, Contents, Encoding);
end;

procedure TFilePath.CreateDirectory;
begin
  TDirectory.CreateDirectory(FPath);
end;

function TFilePath.Entries: TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetFileSystemEntries(FPath));
end;

function TFilePath.Entries(const Pattern: string): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetFileSystemEntries(FPath, Pattern));
end;

function TFilePath.Entries(const Option: TSearchOption): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetFileSystemEntries(FPath, Option));
end;

function TFilePath.Entries(const Pattern: string; const Option: TSearchOption): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetFileSystemEntries(FPath, Pattern, Option));
end;

function TFilePath.Files: TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetFiles(FPath));
end;

function TFilePath.Files(const Pattern: string): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetFiles(FPath, Pattern));
end;

function TFilePath.Files(const Option: TSearchOption): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetFiles(FPath, Option));
end;

function TFilePath.Files(const Pattern: string; const Option: TSearchOption): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetFiles(FPath, Pattern, Option));
end;

function TFilePath.Directories: TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetDirectories(FPath));
end;

function TFilePath.Directories(const Pattern: string): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetDirectories(FPath, Pattern));
end;

function TFilePath.Directories(const Option: TSearchOption): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetDirectories(FPath, Option));
end;

function TFilePath.Directories(const Pattern: string;
  const Option: TSearchOption): TArray<TFilePath>;
begin
  Result := ToPaths(TDirectory.GetDirectories(FPath, Pattern, Option));
end;

function TFilePath.GetHashCode: Int64;
begin
  Result := KeyHash(FPath);
end;

class operator TFilePath.Implicit(const S: string): TFilePath;
begin
  Result := Make(S);
end;

class operator TFilePath.Implicit(const P: TFilePath): string;
begin
  Result := P.FPath;
end;

class operator TFilePath.Divide(const A, B: TFilePath): TFilePath;
begin
  Result := A.Join(B);
end;

class operator TFilePath.Divide(const A: TFilePath; const B: string): TFilePath;
begin
  Result := Make(TPath.Combine(A.FPath, B));
end;

class operator TFilePath.Divide(const A: string; const B: TFilePath): TFilePath;
begin
  Result := Make(TPath.Combine(A, B.FPath));
end;

class operator TFilePath.Equal(const A, B: TFilePath): Boolean;
begin
  Result := A.FPath = B.FPath;
end;

class operator TFilePath.NotEqual(const A, B: TFilePath): Boolean;
begin
  Result := A.FPath <> B.FPath;
end;

end.
