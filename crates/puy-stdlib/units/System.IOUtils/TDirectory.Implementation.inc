// TDirectory (docs/spec/stdlib/System.IOUtils/TDirectory.md): the listings come sorted from
// FsListDir (System.IOUtils.Native's ListDir, or the installed file system's listing).

type
  // Which entries a listing keeps
  TEntryKinds = (ekFiles, ekDirectories, ekBoth);

// Whether Name matches Pattern: `*` any run of characters, `?` one (a UTF-16 code unit); `*.*`
// matches every name. Letters compare as the target's file names (Windows ignores case).
function MatchesPattern(const Name, Pattern: string): Boolean;
begin
  if (Pattern = '*') or (Pattern = '*.*') then
    Exit(True);
  var N := Name;
  var P := Pattern;
  if TOSVersion.Platform = TOSVersion.TPlatform.pfWindows then
  begin
    N := AnsiUpperCase(N);
    P := AnsiUpperCase(P);
  end;
  var I := 1;
  var J := 1;
  var Star := 0;
  var Mark := 0;
  while I <= Length(N) do
    if (J <= Length(P)) and (P[J] <> '*') and ((P[J] = '?') or (P[J] = N[I])) then
    begin
      Inc(I);
      Inc(J);
    end
    else if (J <= Length(P)) and (P[J] = '*') then
    begin
      Star := J;
      Mark := I;
      Inc(J);
    end
    else if Star > 0 then
    begin
      // Let the last `*` take one more character
      Inc(Mark);
      I := Mark;
      J := Star + 1;
    end
    else
      Exit(False);
  while (J <= Length(P)) and (P[J] = '*') do
    Inc(J);
  Result := J > Length(P);
end;

// The entries of the directory Path; a missing one is EDirectoryNotFoundException.
function ListEntries(const Path, Op: string): TArray<TNativeDirEntry>;
begin
  CheckPath(Path);
  try
    Result := FsListDir(Path);
  except
    on E: EInOutError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise DirectoryNotFound(Op, Path, E)
      else
        raise;
  end;
end;

// Whether the entry Kind at Full is a directory: a link counts as what it points to (a broken
// link as a file).
function IsDirectoryEntry(Kind: Integer; const Full: string): Boolean;
begin
  if Kind = 3 then
    Result := DirectoryExists(Full, True)
  else
    Result := Kind = 2;
end;

// Adds the entries of Path that are Kinds and match Pattern to Paths (Count of them so far);
// with All, those of the subdirectories too (not through links).
procedure Collect(const Path, Pattern: string; Kinds: TEntryKinds; All: Boolean;
  var Paths: TArray<string>; var Count: Integer);
begin
  var Entries := ListEntries(Path, 'list directory');
  var Subdirs: TArray<string> := nil;
  for var E in Entries do
  begin
    var Full := TPath.Combine(Path, E.Name);
    var IsDir := IsDirectoryEntry(E.Kind, Full);
    if (Kinds = TEntryKinds.ekBoth) or (IsDir = (Kinds = TEntryKinds.ekDirectories)) then
      if MatchesPattern(E.Name, Pattern) then
      begin
        if Count = Length(Paths) then
          SetLength(Paths, Count * 2 + 8);
        Paths[Count] := Full;
        Inc(Count);
      end;
    if All and (E.Kind = 2) then
    begin
      SetLength(Subdirs, Length(Subdirs) + 1);
      Subdirs[High(Subdirs)] := Full;
    end;
  end;
  for var D in Subdirs do
    Collect(D, Pattern, Kinds, All, Paths, Count);
end;

function List(const Path, Pattern: string; Kinds: TEntryKinds;
  SearchOption: TSearchOption): TArray<string>;
begin
  Result := nil;
  var Count := 0;
  Collect(Path, Pattern, Kinds, SearchOption = TSearchOption.soAllDirectories, Result, Count);
  SetLength(Result, Count);
end;

// Removes everything in the directory Path (links themselves, not what they point to).
procedure DeleteContents(const Path: string);
begin
  for var E in ListEntries(Path, 'delete directory') do
  begin
    var Full := TPath.Combine(Path, E.Name);
    if E.Kind = 2 then
    begin
      DeleteContents(Full);
      FsDeleteDir(Full);
    end
    else
      FsEraseFile(Full);
  end;
end;

class function TDirectory.Exists(const Path: string; FollowLink: Boolean): Boolean;
begin
  Result := DirectoryExists(Path, FollowLink);
end;

class procedure TDirectory.CreateDirectory(const Path: string);
begin
  CheckPath(Path);
  FsMakeDir(Path);
end;

class procedure TDirectory.Delete(const Path: string);
begin
  Delete(Path, False);
end;

class procedure TDirectory.Delete(const Path: string; const Recursive: Boolean);
begin
  CheckPath(Path);
  if not DirectoryExists(Path, False) then
    raise DirectoryNotFound('delete directory', Path, nil);
  if Recursive then
    DeleteContents(Path);
  FsDeleteDir(Path);
end;

class procedure TDirectory.Move(const SourceDirName, DestDirName: string);
begin
  CheckPath(SourceDirName);
  CheckPath(DestDirName);
  if not DirectoryExists(SourceDirName, False) then
    raise DirectoryNotFound('move directory', SourceDirName, nil);
  if PathExists(DestDirName) then
    raise AlreadyExists('move directory to', DestDirName);
  try
    FsMovePath(SourceDirName, DestDirName);
  except
    on E: EInOutError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise DirectoryNotFound('move directory to', DestDirName, E)
      else
        raise;
  end;
end;

// An installed file system may resolve a relative path against a different working directory.
function IsWithinInstalled(const FS: IFileSystem; const Dest, Source: string): Boolean;
begin
  var SourcePath := FS.Canonicalize(Source);
  var SourceLength := Length(SourcePath);

  // Dest may not exist yet. Check its nearest existing ancestor in the installed file system.
  var Count := Length(Dest);
  while True do
  begin
    var Candidate: string := '.';
    if Count > 0 then
      Candidate := System.Copy(Dest, 1, Count);
    try
      var Resolved := FS.Canonicalize(Candidate);
      if Resolved = SourcePath then
        Exit(True);
      Exit((Length(Resolved) > SourceLength) and Resolved.StartsWith(SourcePath) and
        ((SourcePath[SourceLength] in ['/', '\']) or
          (Resolved[SourceLength + 1] in ['/', '\'])));
    except
      on E: EInOutError do
        if E.ErrorKind <> TIOErrorKind.ekNotFound then
          raise;
    end;
    if Count = 0 then
      Break;
    while (Count > 0) and (Dest[Count] in ['/', '\']) do
      Dec(Count);
    while (Count > 0) and not (Dest[Count] in ['/', '\']) do
      Dec(Count);
  end;
  Result := False;
end;

// Whether Dest is Source or inside it, using the installed or the host path rules.
function IsWithin(const Dest, Source: string): Boolean;
begin
  var FS := TFileSystem.Installed;
  if FS <> nil then
    try
      Exit(IsWithinInstalled(FS, Dest, Source));
    except
      on ENotSupportedException do
      begin
        // Fall back to lexical comparison if the file system cannot canonicalize.
      end;
    end;
  var S := IncludeTrailingPathDelimiter(ExpandFileName(Source));
  var D := IncludeTrailingPathDelimiter(ExpandFileName(Dest));
  if TOSVersion.Platform = TOSVersion.TPlatform.pfWindows then
  begin
    S := AnsiUpperCase(S);
    D := AnsiUpperCase(D);
  end;
  Result := System.Copy(D, 1, Length(S)) = S;
end;

class procedure TDirectory.Copy(const SourceDirName, DestDirName: string);
begin
  var Entries := ListEntries(SourceDirName, 'copy directory');
  CheckPath(DestDirName);
  // Copying a directory into itself would never end
  if IsWithin(DestDirName, SourceDirName) then
    raise EArgumentException.Create('cannot copy directory ' + QuotedStr(SourceDirName) +
      ' into itself (' + QuotedStr(DestDirName) + ')');
  FsMakeDir(DestDirName);
  for var E in Entries do
  begin
    var Source := TPath.Combine(SourceDirName, E.Name);
    var Dest := TPath.Combine(DestDirName, E.Name);
    // Only real directories are entered (a link to one is refused by CopyFile)
    if E.Kind = 2 then
      Copy(Source, Dest)
    else
      FsCopyFile(Source, Dest);
  end;
end;

class function TDirectory.IsEmpty(const Path: string): Boolean;
begin
  Result := Length(ListEntries(Path, 'list directory')) = 0;
end;

class function TDirectory.GetFiles(const Path: string): TArray<string>;
begin
  Result := List(Path, '*', TEntryKinds.ekFiles, TSearchOption.soTopDirectoryOnly);
end;

class function TDirectory.GetFiles(const Path: string;
  const SearchOption: TSearchOption): TArray<string>;
begin
  Result := List(Path, '*', TEntryKinds.ekFiles, SearchOption);
end;

class function TDirectory.GetFiles(const Path, SearchPattern: string): TArray<string>;
begin
  Result := List(Path, SearchPattern, TEntryKinds.ekFiles, TSearchOption.soTopDirectoryOnly);
end;

class function TDirectory.GetFiles(const Path, SearchPattern: string;
  const SearchOption: TSearchOption): TArray<string>;
begin
  Result := List(Path, SearchPattern, TEntryKinds.ekFiles, SearchOption);
end;

class function TDirectory.GetDirectories(const Path: string): TArray<string>;
begin
  Result := List(Path, '*', TEntryKinds.ekDirectories, TSearchOption.soTopDirectoryOnly);
end;

class function TDirectory.GetDirectories(const Path: string;
  const SearchOption: TSearchOption): TArray<string>;
begin
  Result := List(Path, '*', TEntryKinds.ekDirectories, SearchOption);
end;

class function TDirectory.GetDirectories(const Path, SearchPattern: string): TArray<string>;
begin
  Result := List(Path, SearchPattern, TEntryKinds.ekDirectories,
    TSearchOption.soTopDirectoryOnly);
end;

class function TDirectory.GetDirectories(const Path, SearchPattern: string;
  const SearchOption: TSearchOption): TArray<string>;
begin
  Result := List(Path, SearchPattern, TEntryKinds.ekDirectories, SearchOption);
end;

class function TDirectory.GetFileSystemEntries(const Path: string): TArray<string>;
begin
  Result := List(Path, '*', TEntryKinds.ekBoth, TSearchOption.soTopDirectoryOnly);
end;

class function TDirectory.GetFileSystemEntries(const Path: string;
  const SearchOption: TSearchOption): TArray<string>;
begin
  Result := List(Path, '*', TEntryKinds.ekBoth, SearchOption);
end;

class function TDirectory.GetFileSystemEntries(const Path, SearchPattern: string): TArray<string>;
begin
  Result := List(Path, SearchPattern, TEntryKinds.ekBoth, TSearchOption.soTopDirectoryOnly);
end;

class function TDirectory.GetFileSystemEntries(const Path, SearchPattern: string;
  const SearchOption: TSearchOption): TArray<string>;
begin
  Result := List(Path, SearchPattern, TEntryKinds.ekBoth, SearchOption);
end;

class function TDirectory.GetCurrentDirectory: string;
begin
  Result := GetCurrentDir;
end;

class procedure TDirectory.SetCurrentDirectory(const Path: string);
begin
  CheckPath(Path);
  if not SetCurrentDir(Path) then
    if not DirectoryExists(Path) then
      raise DirectoryNotFound('change to directory', Path, nil)
    else
      raise EInOutError.Create('cannot change to directory ''' + Path + '''');
end;
