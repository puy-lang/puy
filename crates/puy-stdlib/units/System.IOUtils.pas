unit System.IOUtils;

// Files, directories and paths (docs/spec/stdlib/System.IOUtils/README.md): TPath takes paths
// apart and puts them together by the target's rules (found at run time, as System.SysUtils's
// file names), TFile reads and writes whole files, TDirectory lists and creates directories.
// Written from the specification only, not from any other implementation.
//
// Split as System.Generics.Collections (docs/spec/collections.md 1.2): TPath, TFile and
// TDirectory each have their Interface / Implementation files, read only when a program names
// the type.

interface

uses
  System.SysUtils,
  System.Classes;

{$I System.IOUtils/TPath.Interface.inc}

{$I System.IOUtils/TFile.Interface.inc}

{$I System.IOUtils/TDirectory.Interface.inc}

implementation

uses
  // The Rust part of the unit (crates/puy-stdlib-ext; docs/design/stdlib.md 3.1)
  System.IOUtils.Native;

// The calls of the file system: the Rust functions of System.IOUtils.Native, or the methods of
// the file system a program installed (System.SysUtils's TFileSystem), whose exceptions pass through as
// the host's failures do.

type
  // Uses TFileSystem's protected conversion to keep the native kind mapping in one place.
  TNativeFileKinds = class(TFileSystem)
  public
    class function ToNative(Kind: TFileKind): Integer; static;
  end;

class function TNativeFileKinds.ToNative(Kind: TFileKind): Integer;
begin
  Result := NativeOfFileKind(Kind);
end;

// The key of a UTF-16 code unit that orders strings by their code points: the surrogates come
// after U+E000..U+FFFF.
function CodePointKey(C: Char): Integer;
begin
  Result := Ord(C);
  if (Result >= $D800) and (Result < $E000) then
    Inc(Result, $2000)
  else if Result >= $E000 then
    Dec(Result, $800);
end;

// Whether A comes before B in the order of the code points (as ListDir sorts).
function CodePointLess(const A, B: string): Boolean;
begin
  var N := Length(A);
  if Length(B) < N then
    N := Length(B);
  for var I := 1 to N do
    if A[I] <> B[I] then
      Exit(CodePointKey(A[I]) < CodePointKey(B[I]));
  Result := Length(A) < Length(B);
end;

// The entries of the directory Path, sorted by name in the order of the code points.
function FsListDir(const Path: string): TArray<TNativeDirEntry>;
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    Exit(ListDir(Path));
  var Entries := FS.ListDirectory(Path);
  SetLength(Result, Length(Entries));
  for var I := 0 to High(Entries) do
  begin
    Result[I] := Default(TNativeDirEntry);
    Result[I].Name := Entries[I].Name;
    Result[I].Kind := TNativeFileKinds.ToNative(Entries[I].Kind);
  end;
  // A bottom-up merge sort (a virtual file system may hold thousands of entries in a directory)
  var Other: TArray<TNativeDirEntry>;
  SetLength(Other, Length(Result));
  var Width := 1;
  while Width < Length(Result) do
  begin
    var Lo := 0;
    while Lo < Length(Result) do
    begin
      var Mid := Lo + Width;
      if Mid > Length(Result) then
        Mid := Length(Result);
      var Hi := Mid + Width;
      if Hi > Length(Result) then
        Hi := Length(Result);
      var I := Lo;
      var J := Mid;
      for var K := Lo to Hi - 1 do
        // Equal names keep their order (taking the left one first)
        if (I < Mid) and ((J >= Hi) or not CodePointLess(Result[J].Name, Result[I].Name)) then
        begin
          Other[K] := Result[I];
          Inc(I);
        end
        else
        begin
          Other[K] := Result[J];
          Inc(J);
        end;
      Lo := Hi;
    end;
    var T := Result;
    Result := Other;
    Other := T;
    Width := Width * 2;
  end;
end;

function FsFileInfo(const Path: string; FollowLink: Boolean): TNativeFileInfo;
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    Exit(FileInfo(Path, FollowLink));
  var Info := FS.GetInfo(Path, FollowLink);
  Result := Default(TNativeFileInfo);
  Result.Kind := TNativeFileKinds.ToNative(Info.Kind);
  Result.Size := Info.Size;
  Result.ReadOnly := Info.ReadOnly;
  Result.HasModified := Info.HasModified;
  Result.Modified := Info.Modified;
  Result.HasAccessed := Info.HasAccessed;
  Result.Accessed := Info.Accessed;
  Result.HasCreated := Info.HasCreated;
  Result.Created := Info.Created;
end;

// Which: 0 the creation, 1 the last access, 2 the last write.
procedure FsSetTime(const Path: string; Which: Integer; const Nanos: Int64);
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    System.IOUtils.Native.SetTime(Path, Which, Nanos)
  else
    FS.SetTime(Path, TFileTimeKind(Which), Nanos);
end;

procedure FsSetReadOnly(const Path: string; ReadOnly: Boolean);
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    SetReadOnly(Path, ReadOnly)
  else
    FS.SetReadOnly(Path, ReadOnly);
end;

procedure FsCopyFile(const Source, Dest: string);
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    CopyFile(Source, Dest)
  else
    FS.CopyFile(Source, Dest);
end;

procedure FsMovePath(const Source, Dest: string);
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    MovePath(Source, Dest)
  else
    FS.Rename(Source, Dest);
end;

procedure FsEraseFile(const Path: string);
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    EraseFile(Path)
  else
    FS.RemoveFile(Path);
end;

procedure FsDeleteDir(const Path: string);
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    DeleteDir(Path)
  else
    FS.RemoveDirectory(Path);
end;

// Creates a directory and its missing parents; an existing directory is not an error.
procedure FsMakeDir(const Path: string);
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    MakeDir(Path)
  else
    FS.CreateDirectory(Path, True);
end;

// EArgumentException for an empty path: no operation of TFile and TDirectory takes one.
procedure CheckPath(const Path: string);
begin
  if Path = '' then
    raise EArgumentException.Create('the path is empty');
end;

// The EFileNotFoundException of the operation Op (`read file`, ...) on Path; the kind and code
// of the host's failure E when there is one.
function FileNotFound(const Op, Path: string; const E: EInOutError): EFileNotFoundException;
begin
  Result := EFileNotFoundException.Create('cannot ' + Op + ' ''' + Path + ''': file not found');
  Result.ErrorKind := TIOErrorKind.ekNotFound;
  if E <> nil then
    Result.ErrorCode := E.ErrorCode;
end;

// The EDirectoryNotFoundException of the operation Op on Path.
function DirectoryNotFound(const Op, Path: string;
  const E: EInOutError): EDirectoryNotFoundException;
begin
  Result := EDirectoryNotFoundException.Create('cannot ' + Op + ' ''' + Path +
    ''': directory not found');
  Result.ErrorKind := TIOErrorKind.ekNotFound;
  if E <> nil then
    Result.ErrorCode := E.ErrorCode;
end;

// The EInOutError of the operation Op on Path for something already there.
function AlreadyExists(const Op, Path: string): EInOutError;
begin
  Result := EInOutError.Create('cannot ' + Op + ' ''' + Path + ''': already exists');
  Result.ErrorKind := TIOErrorKind.ekAlreadyExists;
end;

// Whether anything (a file, a directory, a link itself) is at Path.
function PathExists(const Path: string): Boolean;
begin
  Result := FileExists(Path, False) or DirectoryExists(Path, False);
end;

{$I System.IOUtils/TPath.Implementation.inc}

{$I System.IOUtils/TFile.Implementation.inc}

{$I System.IOUtils/TDirectory.Implementation.inc}

end.
