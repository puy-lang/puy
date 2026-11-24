// TPath (docs/spec/stdlib/System.IOUtils/TPath.md): the rules of System.SysUtils's file names.

class function TPath.DirectorySeparatorChar: Char;
begin
  Result := PathDelim[1];
end;

class function TPath.AltDirectorySeparatorChar: Char;
begin
  Result := '/';
end;

class function TPath.VolumeSeparatorChar: Char;
begin
  if DriveDelim = '' then
    Result := '/'
  else
    Result := DriveDelim[1];
end;

class function TPath.PathSeparator: Char;
begin
  Result := PathSep[1];
end;

class function TPath.ExtensionSeparatorChar: Char;
begin
  Result := '.';
end;

class function TPath.Combine(const Path1, Path2: string): string;
begin
  if (Path1 = '') or IsPathRooted(Path2) then
    Result := Path2
  else if Path2 = '' then
    Result := Path1
  else
    Result := IncludeTrailingPathDelimiter(Path1) + Path2;
end;

class function TPath.Combine(const Path1, Path2, Path3: string): string;
begin
  Result := Combine(Combine(Path1, Path2), Path3);
end;

class function TPath.GetDirectoryName(const FileName: string): string;
begin
  Result := ExtractFileDir(FileName);
end;

class function TPath.GetFileName(const FileName: string): string;
begin
  Result := ExtractFileName(FileName);
end;

class function TPath.GetFileNameWithoutExtension(const FileName: string): string;
begin
  var Name := ExtractFileName(FileName);
  Result := Copy(Name, 1, Length(Name) - Length(ExtractFileExt(Name)));
end;

class function TPath.GetExtension(const FileName: string): string;
begin
  Result := ExtractFileExt(ExtractFileName(FileName));
end;

class function TPath.ChangeExtension(const Path, Extension: string): string;
begin
  var Base := Copy(Path, 1, Length(Path) - Length(GetExtension(Path)));
  if (Extension <> '') and (Extension[1] = '.') then
    Result := Base + Extension
  else
    Result := Base + '.' + Extension;
end;

class function TPath.HasExtension(const Path: string): Boolean;
begin
  Result := GetExtension(Path) <> '';
end;

class function TPath.IsPathRooted(const Path: string): Boolean;
begin
  Result := IsPathDelimiter(Path, 1) or (ExtractFileDrive(Path) <> '');
end;

class function TPath.GetPathRoot(const Path: string): string;
begin
  Result := ExtractFileDrive(Path);
  if IsPathDelimiter(Path, Length(Result) + 1) then
    Result := Result + Path[Length(Result) + 1];
end;

class function TPath.GetFullPath(const Path: string): string;
begin
  CheckPath(Path);
  Result := ExpandFileName(Path);
end;

class function TPath.GetTempPath: string;
begin
  Result := TempDir;
end;

class function TPath.GetHomePath: string;
begin
  Result := HomeDir;
end;
