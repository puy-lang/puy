// TFile (docs/spec/stdlib/System.IOUtils/TFile.md): whole files through TFileStream; the
// attributes and the times through System.IOUtils.Native.

const
  // The bytes TFileLineEnumerator reads at a time
  LineChunk = 64 * 1024;
  // The nanoseconds of a day
  DayNanos = 86400000000000;

// Opens the existing file Path (to read unless Way says otherwise); a missing file is
// EFileNotFoundException.
function OpenToRead(const Path, Op: string; Way: Word = fmOpenRead): TFileStream;
begin
  CheckPath(Path);
  try
    Result := TFileStream.Create(Path, Way);
  except
    on E: EFOpenError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise FileNotFound(Op, Path, nil)
      else
        raise;
  end;
end;

// Creates Path (or cuts it to 0 bytes) to write, or with Append opens it at its end (creating
// it if missing); a missing directory is EDirectoryNotFoundException.
function OpenToWrite(const Path, Op: string; Append: Boolean): TFileStream;
begin
  CheckPath(Path);
  try
    if Append and FileExists(Path) then
    begin
      Result := TFileStream.Create(Path, fmOpenWrite);
      Result.Seek(0, TSeekOrigin.soEnd);
    end
    else
      Result := TFileStream.Create(Path, fmCreate);
  except
    on E: EStreamError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise DirectoryNotFound(Op, Path, nil)
      else
        raise;
  end;
end;

// Writes Bytes to Path (at its end with Append) and closes it.
procedure WriteBytesTo(const Path, Op: string; const Bytes: TBytes; Append: Boolean);
begin
  var S := OpenToWrite(Path, Op, Append);
  try
    S.WriteBuffer(Bytes, Length(Bytes));
  finally
    S.Close;
  end;
end;

// The text of Bytes: in Encoding (skipping its preamble), or with Encoding nil in the encoding
// of its BOM (skipped), UTF-8 without one.
function DecodeText(const Bytes: TBytes; Encoding: TEncoding): string;
begin
  var Skip := TEncoding.GetBufferEncoding(Bytes, Encoding);
  Result := Encoding.GetString(Bytes, Skip, Length(Bytes) - Skip);
end;

// The bytes of Contents in Encoding after its preamble.
function EncodeWithPreamble(const Contents: string; const Encoding: TEncoding): TBytes;
begin
  var Preamble := Encoding.GetPreamble;
  var Body := Encoding.GetBytes(Contents);
  SetLength(Result, Length(Preamble) + Length(Body));
  BytesCopy(Result, 0, Preamble, 0, Length(Preamble));
  BytesCopy(Result, Length(Preamble), Body, 0, Length(Body));
end;

// The lines of Text: split at CR LF, LF and CR; a line end at the end makes no empty line.
function SplitLines(const Text: string): TArray<string>;
begin
  Result := nil;
  var Count := 0;
  var Start := 1;
  var I := 1;
  while I <= Length(Text) do
  begin
    if (Text[I] = #10) or (Text[I] = #13) then
    begin
      if Count = Length(Result) then
        SetLength(Result, Count * 2 + 4);
      Result[Count] := System.Copy(Text, Start, I - Start);
      Inc(Count);
      if (Text[I] = #13) and (I < Length(Text)) and (Text[I + 1] = #10) then
        Inc(I);
      Start := I + 1;
    end;
    Inc(I);
  end;
  if Start <= Length(Text) then
  begin
    SetLength(Result, Count + 1);
    Result[Count] := System.Copy(Text, Start, MaxInt);
    Inc(Count);
  end;
  SetLength(Result, Count);
end;

// The text of Lines, each followed by sLineBreak.
function JoinLines(const Lines: TArray<string>): string;
begin
  Result := '';
  for var Line in Lines do
    Result := Result + Line + sLineBreak;
end;

// One time of Path in nanoseconds since the UNIX epoch: Which 0 the creation, 1 the last access,
// 2 the last write.
function GetTimeNanos(const Path: string; Which: Integer): Int64;
const
  Names: array[0..2] of string = ('creation', 'last access', 'last write');
begin
  CheckPath(Path);
  var Info: TNativeFileInfo;
  try
    Info := FsFileInfo(Path, True);
  except
    on E: EInOutError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise FileNotFound('read the attributes of', Path, E)
      else
        raise;
  end;
  var Has: Boolean;
  case Which of
    0:
      begin
        Has := Info.HasCreated;
        Result := Info.Created;
      end;
    1:
      begin
        Has := Info.HasAccessed;
        Result := Info.Accessed;
      end;
  else
    Has := Info.HasModified;
    Result := Info.Modified;
  end;
  if not Has then
  begin
    var E := ENotSupportedException.Create('the file system does not keep the ' + Names[Which] +
      ' time of ''' + Path + '''');
    E.ErrorKind := TIOErrorKind.ekUnsupported;
    raise E;
  end;
end;

// Sets one time of Path (Which as GetTimeNanos) to Nanos since the UNIX epoch.
procedure SetTimeNanos(const Path: string; Which: Integer; const Nanos: Int64);
begin
  CheckPath(Path);
  try
    FsSetTime(Path, Which, Nanos);
  except
    on E: EInOutError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise FileNotFound('set the time of', Path, E)
      else
        raise;
  end;
end;

// The local time of the instant Time (UTC), as an instant of the same clock reading in UTC.
function UtcToLocal(const Time: TInstant): TInstant;
begin
  Result := Time.AddSeconds(LocalOffset(Time.ToUnixNanos));
end;

// Nanos moved by Delta, kept within Int64.
function ShiftClamped(const Nanos, Delta: Int64): Int64;
begin
  if (Delta > 0) and (Nanos > High(Int64) - Delta) then
    Result := High(Int64)
  else if (Delta < 0) and (Nanos < Low(Int64) - Delta) then
    Result := Low(Int64)
  else
    Result := Nanos + Delta;
end;

// The instant (UTC) of the local time Local (given as the instant of the same clock reading in
// UTC). Both a local time a change of the offset makes occur twice and one it skips are read
// with the offset in force before the change: the first is the earlier instant, the second moves
// on by the size of the skip (spec/stdlib/System.IOUtils/TFile.md 4).
function LocalToUtc(const Local: TInstant): TInstant;
begin
  // The offsets before and after any change near Local (an offset is less than a day, and a
  // zone does not change twice within two days)
  var L := Local.ToUnixNanos;
  var Before := LocalOffset(ShiftClamped(L, -DayNanos));
  var After := LocalOffset(ShiftClamped(L, DayNanos));
  Result := Local.AddSeconds(-Before);
  if (After = Before) or (LocalOffset(Result.ToUnixNanos) = Before) then
    Exit;
  var Later := Local.AddSeconds(-After);
  if LocalOffset(Later.ToUnixNanos) = After then
    Result := Later;
end;

// The instant of the TDateTime Time, to the millisecond (TInstant.FromDateTime), in the local
// time with Local and in UTC without; EArgumentException out of the range of TInstant.
function DateTimeToNanos(const Time: TDateTime; Local: Boolean): Int64;
begin
  try
    var T := TInstant.FromDateTime(Time);
    if Local then
      T := LocalToUtc(T);
    Result := T.ToUnixNanos;
  except
    on ERangeError do
      raise EArgumentException.Create('the time ' + FloatToStr(Time) +
        ' is out of the range of file times');
  end;
end;

function GetTime(const Path: string; Which: Integer; Local: Boolean): TDateTime;
begin
  var T := TInstant.FromUnixNanos(GetTimeNanos(Path, Which));
  if Local then
    T := UtcToLocal(T);
  Result := T.ToDateTime;
end;

procedure SetTime(const Path: string; Which: Integer; const Time: TDateTime; Local: Boolean);
begin
  CheckPath(Path);
  SetTimeNanos(Path, Which, DateTimeToNanos(Time, Local));
end;

{ TFileLineEnumerator }

constructor TFileLineEnumerator.Create(const AStream: TFileStream; const AEncoding: TEncoding);
begin
  inherited Create;
  FStream := AStream;
  FEncoding := AEncoding;
  SetLength(FBuffer, LineChunk);
end;

// Reads the next bytes into the buffer; False at the end of the file (and closes it).
function TFileLineEnumerator.Fill: Boolean;
begin
  FPos := 0;
  FLen := 0;
  if FStream = nil then
    Exit(False);
  FLen := FStream.Read(FBuffer, 0, LineChunk);
  if not FStarted then
  begin
    FStarted := True;
    // A BOM is at most 3 bytes
    var Head: TBytes;
    if FLen < 4 then
      SetLength(Head, FLen)
    else
      SetLength(Head, 4);
    BytesCopy(Head, 0, FBuffer, 0, Length(Head));
    FPos := TEncoding.GetBufferEncoding(Head, FEncoding);
    FWide := FEncoding is TUnicodeEncoding;
    FBigEndian := FEncoding is TBigEndianUnicodeEncoding;
  end;
  if FLen = 0 then
  begin
    FStream.Close;
    FStream := nil;
  end;
  Result := FLen > 0;
end;

function TFileLineEnumerator.UnitAt(P: Integer): Integer;
begin
  if not FWide then
    Result := FBuffer[P]
  else if FBigEndian then
    Result := FBuffer[P] * 256 + FBuffer[P + 1]
  else
    Result := FBuffer[P] + FBuffer[P + 1] * 256;
end;

function TFileLineEnumerator.MoveNext: Boolean;
var
  Line: TBytes;
  Found: Boolean;
  Step, U: Integer;

  // Adds the bytes Start..Start + Count - 1 of the buffer to Line.
  procedure Take(Start, Count: Integer);
  begin
    var N := Length(Line);
    SetLength(Line, N + Count);
    BytesCopy(Line, N, FBuffer, Start, Count);
    Found := True;
  end;

begin
  Line := nil;
  Found := False;
  while True do
  begin
    if FWide then
      Step := 2
    else
      Step := 1;
    if FPos + Step > FLen then
    begin
      // An odd byte at the end of UTF-16 goes with the line, which decodes it as U+FFFD
      if FPos < FLen then
        Take(FPos, FLen - FPos);
      if not Fill then
        Break;
      Continue;
    end;
    U := UnitAt(FPos);
    // The LF of a CR LF split between two reads
    if FSkipLF then
    begin
      FSkipLF := False;
      if U = 10 then
      begin
        Inc(FPos, Step);
        Continue;
      end;
    end;
    var Start := FPos;
    while (FPos + Step <= FLen) and (U <> 10) and (U <> 13) do
    begin
      Inc(FPos, Step);
      if FPos + Step <= FLen then
        U := UnitAt(FPos);
    end;
    if FPos > Start then
      Take(Start, FPos - Start);
    // Stopped at a line end (not at the end of the buffer)
    if FPos + Step <= FLen then
    begin
      Inc(FPos, Step);
      FSkipLF := U = 13;
      Found := True;
      Break;
    end;
  end;
  if Found then
    FCurrent := FEncoding.GetString(Line, 0, Length(Line));
  Result := Found;
end;

{ TFileLines }

constructor TFileLines.Create(const APath: string; const AEncoding: TEncoding;
  const AFirst: TFileStream);
begin
  inherited Create;
  FPath := APath;
  FEncoding := AEncoding;
  FFirst := AFirst;
end;

function TFileLines.GetEnumerator: TFileLineEnumerator;
begin
  var S := FFirst;
  FFirst := nil;
  if S = nil then
    S := OpenToRead(FPath, 'read file');
  Result := TFileLineEnumerator.Create(S, FEncoding);
end;

{ TFile }

class function TFile.Exists(const Path: string; FollowLink: Boolean): Boolean;
begin
  Result := FileExists(Path, FollowLink);
end;

class function TFile.ReadAllBytes(const Path: string): TBytes;
begin
  var S := OpenToRead(Path, 'read file');
  try
    var Size := S.Size;
    if Size > MaxInt then
      raise EOutOfMemory.Create('the file ''' + Path + ''' has more than ' + IntToStr(MaxInt) +
        ' bytes');
    SetLength(Result, Size);
    S.ReadBuffer(Result, Size);
  finally
    S.Close;
  end;
end;

class procedure TFile.WriteAllBytes(const Path: string; const Bytes: TBytes);
begin
  WriteBytesTo(Path, 'write file', Bytes, False);
end;

class function TFile.ReadAllText(const Path: string): string;
begin
  Result := DecodeText(ReadAllBytes(Path), nil);
end;

class function TFile.ReadAllText(const Path: string; const Encoding: TEncoding): string;
begin
  if Encoding = nil then
    raise EArgumentException.Create('the encoding is nil');
  Result := DecodeText(ReadAllBytes(Path), Encoding);
end;

class procedure TFile.WriteAllText(const Path, Contents: string);
begin
  WriteBytesTo(Path, 'write file', TEncoding.UTF8.GetBytes(Contents), False);
end;

class procedure TFile.WriteAllText(const Path, Contents: string; const Encoding: TEncoding);
begin
  if Encoding = nil then
    raise EArgumentException.Create('the encoding is nil');
  WriteBytesTo(Path, 'write file', EncodeWithPreamble(Contents, Encoding), False);
end;

class procedure TFile.AppendAllText(const Path, Contents: string);
begin
  WriteBytesTo(Path, 'append to file', TEncoding.UTF8.GetBytes(Contents), True);
end;

class procedure TFile.AppendAllText(const Path, Contents: string; const Encoding: TEncoding);
begin
  if Encoding = nil then
    raise EArgumentException.Create('the encoding is nil');
  WriteBytesTo(Path, 'append to file', Encoding.GetBytes(Contents), True);
end;

class function TFile.ReadAllLines(const Path: string): TArray<string>;
begin
  Result := SplitLines(ReadAllText(Path));
end;

class function TFile.ReadAllLines(const Path: string; const Encoding: TEncoding): TArray<string>;
begin
  Result := SplitLines(ReadAllText(Path, Encoding));
end;

class procedure TFile.WriteAllLines(const Path: string; const Contents: TArray<string>);
begin
  WriteAllText(Path, JoinLines(Contents));
end;

class procedure TFile.WriteAllLines(const Path: string; const Contents: TArray<string>;
  const Encoding: TEncoding);
begin
  WriteAllText(Path, JoinLines(Contents), Encoding);
end;

class function TFile.ReadLines(const Path: string): TFileLines;
begin
  Result := TFileLines.Create(Path, nil, OpenToRead(Path, 'read file'));
end;

class function TFile.ReadLines(const Path: string; const Encoding: TEncoding): TFileLines;
begin
  if Encoding = nil then
    raise EArgumentException.Create('the encoding is nil');
  Result := TFileLines.Create(Path, Encoding, OpenToRead(Path, 'read file'));
end;

class procedure TFile.Delete(const Path: string);
begin
  CheckPath(Path);
  FsEraseFile(Path);
end;

class procedure TFile.Copy(const SourceFileName, DestFileName: string; Overwrite: Boolean);
begin
  CheckPath(SourceFileName);
  CheckPath(DestFileName);
  if not FileExists(SourceFileName) then
    raise FileNotFound('copy file', SourceFileName, nil);
  if not Overwrite and PathExists(DestFileName) then
    raise AlreadyExists('copy file to', DestFileName);
  try
    FsCopyFile(SourceFileName, DestFileName);
  except
    on E: EInOutError do
      if (E.ErrorKind = TIOErrorKind.ekNotFound) and FileExists(SourceFileName) then
        raise DirectoryNotFound('copy file to', DestFileName, E)
      else
        raise;
  end;
end;

class procedure TFile.Move(const SourceFileName, DestFileName: string);
begin
  CheckPath(SourceFileName);
  CheckPath(DestFileName);
  if not FileExists(SourceFileName, False) then
    raise FileNotFound('move file', SourceFileName, nil);
  if PathExists(DestFileName) then
    raise AlreadyExists('move file to', DestFileName);
  try
    FsMovePath(SourceFileName, DestFileName);
  except
    on E: EInOutError do
      if (E.ErrorKind = TIOErrorKind.ekNotFound) and FileExists(SourceFileName, False) then
        raise DirectoryNotFound('move file to', DestFileName, E)
      else
        raise;
  end;
end;

class function TFile.Create(const Path: string): TFileStream;
begin
  Result := OpenToWrite(Path, 'create file', False);
end;

class function TFile.Open(const Path: string; Mode: TFileMode): TFileStream;
begin
  if Mode = TFileMode.fmAppend then
    Result := Open(Path, Mode, TFileAccess.faWrite)
  else
    Result := Open(Path, Mode, TFileAccess.faReadWrite);
end;

class function TFile.Open(const Path: string; Mode: TFileMode; Access: TFileAccess): TFileStream;
const
  Ways: array[TFileAccess] of Word = (fmOpenRead, fmOpenWrite, fmOpenReadWrite);
begin
  CheckPath(Path);
  case Mode of
    TFileMode.fmCreateNew:
      begin
        if PathExists(Path) then
          raise AlreadyExists('create file', Path);
        Result := OpenToWrite(Path, 'create file', False);
      end;
    TFileMode.fmCreate:
      Result := OpenToWrite(Path, 'create file', False);
    TFileMode.fmTruncate:
      begin
        if not FileExists(Path) then
          raise FileNotFound('open file', Path, nil);
        Result := OpenToWrite(Path, 'open file', False);
      end;
    TFileMode.fmAppend:
      Result := OpenToWrite(Path, 'append to file', True);
    TFileMode.fmOpenOrCreate:
      if FileExists(Path) then
        Result := OpenToRead(Path, 'open file', Ways[Access])
      else
        Result := OpenToWrite(Path, 'create file', False);
  else
    Result := OpenToRead(Path, 'open file', Ways[Access]);
  end;
end;

class function TFile.OpenRead(const Path: string): TFileStream;
begin
  Result := Open(Path, TFileMode.fmOpen, TFileAccess.faRead);
end;

class function TFile.OpenWrite(const Path: string): TFileStream;
begin
  Result := Open(Path, TFileMode.fmOpenOrCreate, TFileAccess.faWrite);
end;

class function TFile.GetSize(const Path: string): Int64;
begin
  CheckPath(Path);
  try
    Result := FsFileInfo(Path, True).Size;
  except
    on E: EInOutError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise FileNotFound('read the size of', Path, E)
      else
        raise;
  end;
end;

class function TFile.GetAttributes(const Path: string; FollowLink: Boolean): TFileAttributes;
begin
  CheckPath(Path);
  var Info: TNativeFileInfo;
  try
    Info := FsFileInfo(Path, FollowLink);
  except
    on E: EInOutError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise FileNotFound('read the attributes of', Path, E)
      else
        raise;
  end;
  Result := [];
  if Info.ReadOnly then
    Include(Result, TFileAttribute.faReadOnly);
  if Info.Kind = 2 then
    Include(Result, TFileAttribute.faDirectory)
  else if Info.Kind = 3 then
    Include(Result, TFileAttribute.faSymLink);
  if Result = [] then
    Result := [TFileAttribute.faNormal];
end;

class procedure TFile.SetAttributes(const Path: string; const Attributes: TFileAttributes);
begin
  CheckPath(Path);
  try
    FsSetReadOnly(Path, TFileAttribute.faReadOnly in Attributes);
  except
    on E: EInOutError do
      if E.ErrorKind = TIOErrorKind.ekNotFound then
        raise FileNotFound('set the attributes of', Path, E)
      else
        raise;
  end;
end;

class function TFile.GetCreationTimeUtc(const Path: string): TDateTime;
begin
  Result := GetTime(Path, 0, False);
end;

class function TFile.GetLastAccessTimeUtc(const Path: string): TDateTime;
begin
  Result := GetTime(Path, 1, False);
end;

class function TFile.GetLastWriteTimeUtc(const Path: string): TDateTime;
begin
  Result := GetTime(Path, 2, False);
end;

class procedure TFile.SetCreationTimeUtc(const Path: string; const CreationTime: TDateTime);
begin
  SetTime(Path, 0, CreationTime, False);
end;

class procedure TFile.SetLastAccessTimeUtc(const Path: string; const LastAccessTime: TDateTime);
begin
  SetTime(Path, 1, LastAccessTime, False);
end;

class procedure TFile.SetLastWriteTimeUtc(const Path: string; const LastWriteTime: TDateTime);
begin
  SetTime(Path, 2, LastWriteTime, False);
end;

class function TFile.GetCreationTime(const Path: string): TDateTime;
begin
  Result := GetTime(Path, 0, True);
end;

class function TFile.GetLastAccessTime(const Path: string): TDateTime;
begin
  Result := GetTime(Path, 1, True);
end;

class function TFile.GetLastWriteTime(const Path: string): TDateTime;
begin
  Result := GetTime(Path, 2, True);
end;

class procedure TFile.SetCreationTime(const Path: string; const CreationTime: TDateTime);
begin
  SetTime(Path, 0, CreationTime, True);
end;

class procedure TFile.SetLastAccessTime(const Path: string; const LastAccessTime: TDateTime);
begin
  SetTime(Path, 1, LastAccessTime, True);
end;

class procedure TFile.SetLastWriteTime(const Path: string; const LastWriteTime: TDateTime);
begin
  SetTime(Path, 2, LastWriteTime, True);
end;

class function TFile.GetCreationInstant(const Path: string): TInstant;
begin
  Result := TInstant.FromUnixNanos(GetTimeNanos(Path, 0));
end;

class function TFile.GetLastAccessInstant(const Path: string): TInstant;
begin
  Result := TInstant.FromUnixNanos(GetTimeNanos(Path, 1));
end;

class function TFile.GetLastWriteInstant(const Path: string): TInstant;
begin
  Result := TInstant.FromUnixNanos(GetTimeNanos(Path, 2));
end;

class procedure TFile.SetCreationInstant(const Path: string; const CreationInstant: TInstant);
begin
  SetTimeNanos(Path, 0, CreationInstant.ToUnixNanos);
end;

class procedure TFile.SetLastAccessInstant(const Path: string;
  const LastAccessInstant: TInstant);
begin
  SetTimeNanos(Path, 1, LastAccessInstant.ToUnixNanos);
end;

class procedure TFile.SetLastWriteInstant(const Path: string; const LastWriteInstant: TInstant);
begin
  SetTimeNanos(Path, 2, LastWriteInstant.ToUnixNanos);
end;
