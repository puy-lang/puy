// Files, directories and file names (docs/spec/stdlib/System.SysUtils/Files.md). The calls of
// the host are the Rust functions of System.SysUtils.Native; while a program has installed a
// file system (TFileSystem, at the end of Files.Interface.inc), the Fs... functions call it
// instead.

var
  // The installed file system (nil: the host's).
  InstalledFS: IFileSystem;

class procedure TFileSystem.Install(const FS: IFileSystem);
begin
  InstalledFS := FS;
end;

class procedure TFileSystem.Uninstall;
begin
  InstalledFS := nil;
end;

class function TFileSystem.Installed: IFileSystem;
begin
  Result := InstalledFS;
end;

// What Path names, as System.SysUtils.Native.PathKind: 0 nothing (or a path the file system
// cannot read), 1 a file, 2 a directory, 3 a symbolic link, 4 anything else.
function FsPathKind(const Path: string; FollowLink: Boolean): Integer;
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    Exit(PathKind(Path, FollowLink));
  try
    Result := TFileSystem.NativeOfFileKind(FS.GetInfo(Path, FollowLink).Kind);
  except
    on EInOutError do
      Result := 0;
    on ENotSupportedException do
      Result := 0;
  end;
end;

// The Fs... functions below answer False where the installed file system raises an
// EInOutError or an ENotSupportedException, as the host's failures are answered.

function FsRemoveFile(const Path: string): Boolean;
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    Exit(RemoveFile(Path));
  Result := True;
  try
    FS.RemoveFile(Path);
  except
    on EInOutError do
      Result := False;
    on ENotSupportedException do
      Result := False;
  end;
end;

function FsRename(const OldName, NewName: string): Boolean;
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    Exit(Rename(OldName, NewName));
  Result := True;
  try
    FS.Rename(OldName, NewName);
  except
    on EInOutError do
      Result := False;
    on ENotSupportedException do
      Result := False;
  end;
end;

function FsCreateDir(const Path: string; Recursive: Boolean): Boolean;
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    Exit(System.SysUtils.Native.CreateDir(Path, Recursive));
  Result := True;
  try
    FS.CreateDirectory(Path, Recursive);
  except
    on EInOutError do
      Result := False;
    on ENotSupportedException do
      Result := False;
  end;
end;

function FsRemoveDir(const Path: string): Boolean;
begin
  var FS := TFileSystem.Installed;
  if FS = nil then
    Exit(System.SysUtils.Native.RemoveDir(Path));
  Result := True;
  try
    FS.RemoveDirectory(Path);
  except
    on EInOutError do
      Result := False;
    on ENotSupportedException do
      Result := False;
  end;
end;

function OnWindows: Boolean;
begin
  Result := TOSVersion.Platform = TOSVersion.TPlatform.pfWindows;
end;

function PathDelim: string;
begin
  if OnWindows then
    Result := '\'
  else
    Result := '/';
end;

function DriveDelim: string;
begin
  if OnWindows then
    Result := ':'
  else
    Result := '';
end;

function PathSep: string;
begin
  if OnWindows then
    Result := ';'
  else
    Result := ':';
end;

// The characters that end the directory part of a file name.
function DirEnds: string;
begin
  if OnWindows then
    Result := '\/:'
  else
    Result := '/';
end;

function IsDelim(C: Char): Boolean;
begin
  Result := (C = '/') or ((C = '\') and OnWindows);
end;

function LastDelimiter(const Delimiters, S: string): Integer;
begin
  for var I := Length(S) downto 1 do
    if Pos(S[I], Delimiters) > 0 then
      Exit(I);
  Result := 0;
end;

function IsPathDelimiter(const S: string; Index: Integer): Boolean;
begin
  Result := (Index >= 1) and (Index <= Length(S)) and IsDelim(S[Index]);
end;

function IncludeTrailingPathDelimiter(const S: string): string;
begin
  if IsPathDelimiter(S, Length(S)) then
    Result := S
  else
    Result := S + PathDelim;
end;

function ExcludeTrailingPathDelimiter(const S: string): string;
begin
  if IsPathDelimiter(S, Length(S)) then
    Result := Copy(S, 1, Length(S) - 1)
  else
    Result := S;
end;

function ExtractFileDrive(const FileName: string): string;
begin
  Result := '';
  if not OnWindows then
    Exit;
  if (Length(FileName) >= 2) and (FileName[2] = ':') then
    Exit(Copy(FileName, 1, 2));
  // `\\server\share`: up to the separator after the share
  if IsPathDelimiter(FileName, 1) and IsPathDelimiter(FileName, 2) then
  begin
    var Seps := 0;
    var I := 3;
    while I <= Length(FileName) do
    begin
      if IsDelim(FileName[I]) then
      begin
        Inc(Seps);
        if Seps = 2 then
          Break;
      end;
      Inc(I);
    end;
    if Seps >= 1 then
      Result := Copy(FileName, 1, I - 1);
  end;
end;

function ExtractFilePath(const FileName: string): string;
begin
  Result := Copy(FileName, 1, LastDelimiter(DirEnds, FileName));
end;

function ExtractFileDir(const FileName: string): string;
begin
  var I := LastDelimiter(DirEnds, FileName);
  // Keep the separator of a root (`/`, `C:\`) and of a run of separators (`\\`)
  if (I > 1) and IsDelim(FileName[I]) and not IsDelim(FileName[I - 1])
    and not ((FileName[I - 1] = ':') and OnWindows) then
    Dec(I);
  Result := Copy(FileName, 1, I);
end;

function ExtractFileName(const FileName: string): string;
begin
  Result := Copy(FileName, LastDelimiter(DirEnds, FileName) + 1, MaxInt);
end;

function ExtractFileExt(const FileName: string): string;
begin
  var I := LastDelimiter('.' + DirEnds, FileName);
  if (I > 0) and (FileName[I] = '.') then
    Result := Copy(FileName, I, MaxInt)
  else
    Result := '';
end;

function ChangeFileExt(const FileName, Extension: string): string;
begin
  var I := LastDelimiter('.' + DirEnds, FileName);
  if (I = 0) or (FileName[I] <> '.') then
    I := Length(FileName) + 1;
  Result := Copy(FileName, 1, I - 1) + Extension;
end;

function ChangeFilePath(const FileName, Path: string): string;
begin
  Result := IncludeTrailingPathDelimiter(Path) + ExtractFileName(FileName);
end;

function ExpandFileName(const FileName: string): string;
begin
  var Root: string;
  var Rest: string;
  var Drive := ExtractFileDrive(FileName);
  if Drive <> '' then
  begin
    // `C:\a` and `\\server\share\a` are absolute; `C:a` is taken as `C:\a`
    Root := Drive + PathDelim;
    Rest := Copy(FileName, Length(Drive) + 1, MaxInt);
  end
  else if OnWindows and IsPathDelimiter(FileName, 1) and IsPathDelimiter(FileName, 2) then
  begin
    // `\\server` without a share is not a drive, but stays a UNC name
    Root := PathDelim + PathDelim;
    Rest := Copy(FileName, 3, MaxInt);
  end
  else if IsPathDelimiter(FileName, 1) then
  begin
    // The root of the working directory's drive
    Root := ExtractFileDrive(GetCurrentDir) + PathDelim;
    Rest := FileName;
  end
  else
  begin
    var Cwd := GetCurrentDir;
    Drive := ExtractFileDrive(Cwd);
    Root := Drive + PathDelim;
    Rest := Copy(Cwd, Length(Drive) + 1, MaxInt) + PathDelim + FileName;
  end;
  // The parts of Rest, with `.` dropped and `..` taking off the part before it
  var Parts: TArray<string> := nil;
  var Part := '';
  for var I := 1 to Length(Rest) + 1 do
    if (I > Length(Rest)) or IsDelim(Rest[I]) then
    begin
      if Part = '..' then
      begin
        if Length(Parts) > 0 then
          SetLength(Parts, Length(Parts) - 1);
      end
      else if (Part <> '') and (Part <> '.') then
      begin
        SetLength(Parts, Length(Parts) + 1);
        Parts[High(Parts)] := Part;
      end;
      Part := '';
    end
    else
      Part := Part + Rest[I];
  Result := Root;
  for var I := 0 to High(Parts) do
  begin
    if I > 0 then
      Result := Result + PathDelim;
    Result := Result + Parts[I];
  end;
  // A name that ends with a separator keeps it
  if (Length(Parts) > 0) and IsPathDelimiter(FileName, Length(FileName)) then
    Result := Result + PathDelim;
end;

function FileExists(const FileName: string; FollowLink: Boolean): Boolean;
begin
  // A file, a link itself (without FollowLink) or anything else that is not a directory
  Result := FsPathKind(FileName, FollowLink) in [1, 3, 4];
end;

function DirectoryExists(const Directory: string; FollowLink: Boolean): Boolean;
begin
  Result := FsPathKind(Directory, FollowLink) = 2;
end;

function DeleteFile(const FileName: string): Boolean;
begin
  Result := FsRemoveFile(FileName);
end;

function RenameFile(const OldName, NewName: string): Boolean;
begin
  Result := FsRename(OldName, NewName);
end;

function CreateDir(const Dir: string): Boolean;
begin
  Result := FsCreateDir(Dir, False);
end;

function ForceDirectories(const Dir: string): Boolean;
begin
  if Dir = '' then
    Exit(False);
  Result := FsCreateDir(Dir, True);
end;

function RemoveDir(const Dir: string): Boolean;
begin
  Result := FsRemoveDir(Dir);
end;

function GetCurrentDir: string;
begin
  Result := CurrentDir;
end;

function SetCurrentDir(const Dir: string): Boolean;
begin
  Result := System.SysUtils.Native.SetCurrentDir(Dir);
end;

// Convert between the two representations at the file-system boundary.
class function TFileSystem.NativeOfFileKind(Kind: TFileKind): Integer;
begin
  Result := Ord(Kind) + 1;
end;

function FileKindOfNative(Kind: Integer): TFileKind;
begin
  Result := TFileKind(Kind - 1);
end;

type
  // Shared by the host adapter and THandleStream.
  THostFile = class(TRefObject, IFileHandle)
  private
    FHandle: TFileHandle;
  public
    constructor Create(const AHandle: TFileHandle);
    function Read(Count: Integer): TArray<Byte>;
    procedure Write(const Bytes: TArray<Byte>; Offset, Count: Integer);
    function Seek(const Offset: Int64; Origin: TFileSeekOrigin): Int64;
    function GetSize: Int64;
    procedure SetSize(const Size: Int64);
    procedure Flush;
    procedure Close;
  end;

constructor THostFile.Create(const AHandle: TFileHandle);
begin
  inherited Create;
  FHandle := AHandle;
end;

function THostFile.Read(Count: Integer): TArray<Byte>;
begin
  Result := FHandle.Read(Count);
end;

procedure THostFile.Write(const Bytes: TArray<Byte>; Offset, Count: Integer);
begin
  FHandle.Write(Bytes, Offset, Count);
end;

function THostFile.Seek(const Offset: Int64; Origin: TFileSeekOrigin): Int64;
begin
  Result := FHandle.Seek(Offset, Ord(Origin));
end;

function THostFile.GetSize: Int64;
begin
  Result := FHandle.Size;
end;

procedure THostFile.SetSize(const Size: Int64);
begin
  FHandle.SetSize(Size);
end;

procedure THostFile.Flush;
begin
  FHandle.Flush;
end;

procedure THostFile.Close;
begin
  FHandle.Close;
end;

class function TFileSystem.WrapHostFile(const Handle: TFileHandle): IFileHandle;
begin
  Result := THostFile.Create(Handle);
end;

type
  // Explicit access to the host, even inside an installed file system.
  THostFileSystem = class(TRefObject, IFileSystem)
  public
    function OpenFile(const Path: string; Mode: TFileOpenMode): IFileHandle;
    function GetInfo(const Path: string; FollowLink: Boolean): TFileSystemInfo;
    function ListDirectory(const Path: string): TArray<TFileSystemEntry>;
    procedure CreateDirectory(const Path: string; Recursive: Boolean);
    procedure RemoveDirectory(const Path: string);
    procedure RemoveFile(const Path: string);
    procedure Rename(const OldPath, NewPath: string);
    procedure CopyFile(const Source, Dest: string);
    procedure SetTime(const Path: string; Kind: TFileTimeKind; const Nanos: Int64);
    procedure SetReadOnly(const Path: string; ReadOnly: Boolean);
    function Canonicalize(const Path: string): string;
  end;

class function TFileSystem.Host: IFileSystem;
begin
  Result := THostFileSystem.Create;
end;

function THostFileSystem.OpenFile(const Path: string; Mode: TFileOpenMode): IFileHandle;
begin
  Result := TFileSystem.WrapHostFile(System.Classes.Native.OpenFile(Path, Ord(Mode)));
end;

function THostFileSystem.GetInfo(const Path: string; FollowLink: Boolean): TFileSystemInfo;
begin
  var Info := System.IOUtils.Native.FileInfo(Path, FollowLink);
  Result := Default(TFileSystemInfo);
  Result.Kind := FileKindOfNative(Info.Kind);
  Result.Size := Info.Size;
  Result.ReadOnly := Info.ReadOnly;
  Result.HasModified := Info.HasModified;
  Result.Modified := Info.Modified;
  Result.HasAccessed := Info.HasAccessed;
  Result.Accessed := Info.Accessed;
  Result.HasCreated := Info.HasCreated;
  Result.Created := Info.Created;
end;

function THostFileSystem.ListDirectory(const Path: string): TArray<TFileSystemEntry>;
begin
  var Entries := System.IOUtils.Native.ListDir(Path);
  SetLength(Result, Length(Entries));
  for var I := 0 to High(Entries) do
  begin
    Result[I].Name := Entries[I].Name;
    Result[I].Kind := FileKindOfNative(Entries[I].Kind);
  end;
end;

procedure THostFileSystem.CreateDirectory(const Path: string; Recursive: Boolean);
begin
  System.FileSystem.Native.CreateDirectory(Path, Recursive);
end;

procedure THostFileSystem.RemoveDirectory(const Path: string);
begin
  System.IOUtils.Native.DeleteDir(Path);
end;

procedure THostFileSystem.RemoveFile(const Path: string);
begin
  System.IOUtils.Native.EraseFile(Path);
end;

procedure THostFileSystem.Rename(const OldPath, NewPath: string);
begin
  System.IOUtils.Native.MovePath(OldPath, NewPath);
end;

procedure THostFileSystem.CopyFile(const Source, Dest: string);
begin
  System.IOUtils.Native.CopyFile(Source, Dest);
end;

procedure THostFileSystem.SetTime(const Path: string; Kind: TFileTimeKind; const Nanos: Int64);
begin
  System.IOUtils.Native.SetTime(Path, Ord(Kind), Nanos);
end;

procedure THostFileSystem.SetReadOnly(const Path: string; ReadOnly: Boolean);
begin
  System.IOUtils.Native.SetReadOnly(Path, ReadOnly);
end;

function THostFileSystem.Canonicalize(const Path: string): string;
begin
  Result := System.FileSystem.Native.CanonicalPath(Path);
end;
