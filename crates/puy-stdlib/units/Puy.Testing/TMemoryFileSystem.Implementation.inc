// ===== The in-memory file system (docs/spec/stdlib/Puy.Testing/README.md chapter 7) =====

type
  // Each handle keeps the node alive, even after its directory entry is removed.
  TMemoryFile = class(TRefObject, IFileHandle)
  private
    FNode: TMemoryFileSystem.TNode;
    FPath: string;
    FMode: TFileOpenMode;
    FPosition: Int64;
    procedure CheckOpen;
    procedure CheckWrite;
  public
    constructor Create(Node: TMemoryFileSystem.TNode; const Path: string; Mode: TFileOpenMode);
    function Read(Count: Integer): TArray<Byte>;
    procedure Write(const Bytes: TArray<Byte>; Offset, Count: Integer);
    function Seek(const Offset: Int64; Origin: TFileSeekOrigin): Int64;
    function GetSize: Int64;
    procedure SetSize(const Size: Int64);
    procedure Flush;
    procedure Close;
  end;

function MemoryFileError(const Reason, Path: string; Kind: TIOErrorKind): EInOutError;
begin
  Result := EInOutError.Create('memory file system: ' + Reason + ' (' + QuotedStr(Path) + ')');
  Result.ErrorKind := Kind;
end;

function MemoryFileNow: Int64;
begin
  Result := TInstant.Now.ToUnixNanos;
end;

constructor TMemoryFileSystem.TNode.Create(AKind: TFileKind);
begin
  inherited Create;
  Kind := AKind;
  Created := MemoryFileNow;
  Accessed := Created;
  Modified := Created;
  if Kind = TFileKind.fkDirectory then
    Children := TDictionary<string, TNode>.Create;
end;

destructor TMemoryFileSystem.TNode.Destroy;
begin
  Children.Free;
  inherited Destroy;
end;

procedure TMemoryFileSystem.TNode.CheckWritable(const Path: string);
begin
  if ReadOnly then
    raise MemoryFileError('file is read-only', Path, TIOErrorKind.ekPermissionDenied);
end;

function TMemoryFileSystem.TNode.FindChild(const Name: string): TNode;
begin
  if not Children.TryGetValue(Name, Result) then
    Result := nil;
end;

procedure TMemoryFileSystem.TNode.AddChild(const Name: string; Node: TNode; const AtTime: Int64);
begin
  Children.Add(Name, Node);
  Modified := AtTime;
end;

constructor TMemoryFile.Create(Node: TMemoryFileSystem.TNode; const Path: string;
  Mode: TFileOpenMode);
begin
  inherited Create;
  FNode := Node;
  FPath := Path;
  FMode := Mode;
end;

procedure TMemoryFile.CheckOpen;
begin
  if FNode = nil then
    raise EInvalidOperation.Create('the file ' + QuotedStr(FPath) + ' is closed');
end;

procedure TMemoryFile.CheckWrite;
begin
  CheckOpen;
  if FMode = TFileOpenMode.omRead then
    raise MemoryFileError('file was opened for reading', FPath, TIOErrorKind.ekPermissionDenied);
  FNode.CheckWritable(FPath);
end;

function TMemoryFile.Read(Count: Integer): TArray<Byte>;
begin
  CheckOpen;
  if FMode = TFileOpenMode.omWrite then
    raise MemoryFileError('file was opened for writing', FPath, TIOErrorKind.ekPermissionDenied);
  if Count < 0 then
    raise ERangeError.Create('read count is negative');
  var Available := Length(FNode.Bytes) - FPosition;
  if (Available <= 0) or (Count = 0) then
    Exit(nil);
  if Count > Available then
    Count := Available;
  var AtTime := MemoryFileNow;
  Result := System.Copy(FNode.Bytes, FPosition, Count);
  Inc(FPosition, Count);
  FNode.Accessed := AtTime;
end;

procedure TMemoryFile.Write(const Bytes: TArray<Byte>; Offset, Count: Integer);
begin
  CheckWrite;
  if (Offset < 0) or (Count < 0) or (Offset > Length(Bytes) - Count) then
    raise ERangeError.Create('write range is outside the byte array');
  if Count = 0 then
    Exit;
  if FPosition > MaxInt - Count then
    raise ERangeError.Create('memory file size exceeds MaxInt');
  var AtTime := MemoryFileNow;
  var EndPosition := FPosition + Count;
  if EndPosition > Length(FNode.Bytes) then
    SetLength(FNode.Bytes, EndPosition);
  for var I := 0 to Count - 1 do
    FNode.Bytes[FPosition + I] := Bytes[Offset + I];
  FPosition := EndPosition;
  FNode.Modified := AtTime;
end;

function TMemoryFile.Seek(const Offset: Int64; Origin: TFileSeekOrigin): Int64;
begin
  CheckOpen;
  var Base: Int64 := 0;
  case Origin of
    TFileSeekOrigin.fsoCurrent: Base := FPosition;
    TFileSeekOrigin.fsoEnd: Base := Length(FNode.Bytes);
  end;
  if (Offset < -Base) or (Offset > High(Int64) - Base) then
    raise MemoryFileError('seek position is out of range', FPath, TIOErrorKind.ekOther);
  FPosition := Base + Offset;
  Result := FPosition;
end;

function TMemoryFile.GetSize: Int64;
begin
  CheckOpen;
  Result := Length(FNode.Bytes);
end;

procedure TMemoryFile.SetSize(const Size: Int64);
begin
  CheckWrite;
  if (Size < 0) or (Size > MaxInt) then
    raise ERangeError.Create('memory file size is out of range');
  if Size <> Length(FNode.Bytes) then
  begin
    var AtTime := MemoryFileNow;
    SetLength(FNode.Bytes, Size);
    FNode.Modified := AtTime;
  end;
end;

procedure TMemoryFile.Flush;
begin
  CheckOpen;
end;

procedure TMemoryFile.Close;
begin
  FNode := nil;
end;

function MemoryPathIsAbsolute(const Path: string): Boolean;
begin
  Result := (Path <> '') and ((Path[1] = '/') or (Path[1] = '\'));
end;

function TMemoryFileSystem.PathParts(const Path: string): TArray<string>;
begin
  if (Path = '') or (Pos(#0, Path) <> 0) then
    raise MemoryFileError('invalid path', Path, TIOErrorKind.ekOther);
  var FullPath := Path;
  if not MemoryPathIsAbsolute(Path) then
    FullPath := FWorkingDirectory + '/' + Path;
  Result := FullPath.Split(['/', '\']);
  // Compact the path in place, keeping the count of components accepted so far.
  var Count := 0;
  for var I := 0 to High(Result) do
  begin
    var Part := Result[I];
    if Part = '..' then
    begin
      if Count > 0 then
        Dec(Count);
    end
    else if (Part <> '') and (Part <> '.') then
    begin
      Result[Count] := Part;
      Inc(Count);
    end;
  end;
  SetLength(Result, Count);
end;

function TMemoryFileSystem.DirectoryAt(const Parts: TArray<string>; Count: Integer;
  const Path: string; CreateMissing: Boolean): TNode;
begin
  Result := FRoot;
  for var I := 0 to Count - 1 do
  begin
    var Child := Result.FindChild(Parts[I]);
    if Child = nil then
    begin
      if not CreateMissing then
        raise MemoryFileError('directory not found', Path, TIOErrorKind.ekNotFound);
      Child := TNode.Create(TFileKind.fkDirectory);
      Result.AddChild(Parts[I], Child, Child.Created);
    end;
    if Child.Kind <> TFileKind.fkDirectory then
      raise MemoryFileError('expected a directory', Path, TIOErrorKind.ekOther);
    Result := Child;
  end;
end;

function TMemoryFileSystem.NodeAt(const Parts: TArray<string>; const Path: string): TNode;
begin
  if Length(Parts) = 0 then
    Exit(FRoot);
  var Parent := DirectoryAt(Parts, High(Parts), Path);
  Result := Parent.FindChild(Parts[High(Parts)]);
  if Result = nil then
    raise MemoryFileError('path not found', Path, TIOErrorKind.ekNotFound);
end;

function TMemoryFileSystem.IsWithin(const Parts, Directory: TArray<string>): Boolean;
begin
  if Length(Parts) < Length(Directory) then
    Exit(False);
  for var I := 0 to High(Directory) do
    if Parts[I] <> Directory[I] then
      Exit(False);
  Result := True;
end;

procedure TMemoryFileSystem.Init(const WorkingDirectory: string);
begin
  if not MemoryPathIsAbsolute(WorkingDirectory) then
    raise EArgumentException.Create('the working directory must be an absolute path');
  var Parts := PathParts(WorkingDirectory);
  FRoot := TNode.Create(TFileKind.fkDirectory);
  DirectoryAt(Parts, Length(Parts), WorkingDirectory, True);
  FWorkingDirectory := '/' + TStringHelper.Join('/', Parts);
end;

constructor TMemoryFileSystem.Create;
begin
  inherited Create;
  Init('/');
end;

constructor TMemoryFileSystem.Create(const WorkingDirectory: string);
begin
  inherited Create;
  Init(WorkingDirectory);
end;

function TMemoryFileSystem.OpenFile(const Path: string; Mode: TFileOpenMode): IFileHandle;
begin
  var Parts := PathParts(Path);
  if Length(Parts) = 0 then
    raise MemoryFileError('cannot open a directory', Path, TIOErrorKind.ekIsDirectory);
  var Parent := DirectoryAt(Parts, High(Parts), Path);
  var Name := Parts[High(Parts)];
  var Node := Parent.FindChild(Name);
  if Node = nil then
  begin
    if Mode <> TFileOpenMode.omCreate then
      raise MemoryFileError('file not found', Path, TIOErrorKind.ekNotFound);
    Node := TNode.Create(TFileKind.fkFile);
    Parent.AddChild(Name, Node, Node.Created);
  end
  else
  begin
    if Node.Kind <> TFileKind.fkFile then
      raise MemoryFileError('cannot open a directory', Path, TIOErrorKind.ekIsDirectory);
    if Mode <> TFileOpenMode.omRead then
      Node.CheckWritable(Path);
    if Mode = TFileOpenMode.omCreate then
    begin
      Node.Modified := MemoryFileNow;
      Node.Bytes := nil;
    end;
  end;
  Result := TMemoryFile.Create(Node, Path, Mode);
end;

function TMemoryFileSystem.GetInfo(const Path: string; FollowLink: Boolean): TFileSystemInfo;
begin
  var Node := NodeAt(PathParts(Path), Path);
  Result := Default(TFileSystemInfo);
  Result.Kind := Node.Kind;
  Result.Size := Length(Node.Bytes);
  Result.ReadOnly := Node.ReadOnly;
  Result.HasCreated := True;
  Result.Created := Node.Created;
  Result.HasAccessed := True;
  Result.Accessed := Node.Accessed;
  Result.HasModified := True;
  Result.Modified := Node.Modified;
end;

function TMemoryFileSystem.ListDirectory(const Path: string): TArray<TFileSystemEntry>;
begin
  var Parts := PathParts(Path);
  var Node := DirectoryAt(Parts, Length(Parts), Path);
  SetLength(Result, Node.Children.Count);
  var I := 0;
  for var Entry in Node.Children do
  begin
    Result[I].Name := Entry.Key;
    Result[I].Kind := Entry.Value.Kind;
    Inc(I);
  end;
end;

procedure TMemoryFileSystem.CreateDirectory(const Path: string; Recursive: Boolean);
begin
  var Parts := PathParts(Path);
  if Recursive then
  begin
    DirectoryAt(Parts, Length(Parts), Path, True);
    Exit;
  end;
  if Length(Parts) = 0 then
    raise MemoryFileError('path already exists', Path, TIOErrorKind.ekAlreadyExists);
  var Parent := DirectoryAt(Parts, High(Parts), Path);
  var Name := Parts[High(Parts)];
  if Parent.FindChild(Name) <> nil then
    raise MemoryFileError('path already exists', Path, TIOErrorKind.ekAlreadyExists);
  var Node := TNode.Create(TFileKind.fkDirectory);
  Parent.AddChild(Name, Node, Node.Created);
end;

procedure TMemoryFileSystem.RemoveDirectory(const Path: string);
begin
  var Parts := PathParts(Path);
  if IsWithin(PathParts(FWorkingDirectory), Parts) then
    raise MemoryFileError('cannot remove the working directory or an ancestor', Path,
      TIOErrorKind.ekPermissionDenied);
  var Parent := DirectoryAt(Parts, High(Parts), Path);
  var Name := Parts[High(Parts)];
  var Node := Parent.FindChild(Name);
  if Node = nil then
    raise MemoryFileError('directory not found', Path, TIOErrorKind.ekNotFound);
  if Node.Kind <> TFileKind.fkDirectory then
    raise MemoryFileError('expected a directory', Path, TIOErrorKind.ekOther);
  if Node.Children.Count <> 0 then
    raise MemoryFileError('directory is not empty', Path, TIOErrorKind.ekOther);
  var AtTime := MemoryFileNow;
  Parent.Children.Remove(Name);
  Parent.Modified := AtTime;
end;

procedure TMemoryFileSystem.RemoveFile(const Path: string);
begin
  var Parts := PathParts(Path);
  if Length(Parts) = 0 then
    raise MemoryFileError('cannot remove a directory as a file', Path, TIOErrorKind.ekIsDirectory);
  var Parent := DirectoryAt(Parts, High(Parts), Path);
  var Name := Parts[High(Parts)];
  var Node := Parent.FindChild(Name);
  if Node = nil then
    raise MemoryFileError('file not found', Path, TIOErrorKind.ekNotFound);
  if Node.Kind <> TFileKind.fkFile then
    raise MemoryFileError('cannot remove a directory as a file', Path, TIOErrorKind.ekIsDirectory);
  var AtTime := MemoryFileNow;
  Parent.Children.Remove(Name);
  Parent.Modified := AtTime;
end;

procedure TMemoryFileSystem.Rename(const OldPath, NewPath: string);
begin
  var OldParts := PathParts(OldPath);
  var NewParts := PathParts(NewPath);
  if IsWithin(PathParts(FWorkingDirectory), OldParts) then
    raise MemoryFileError('cannot move the working directory or an ancestor', OldPath,
      TIOErrorKind.ekPermissionDenied);
  var OldParent := DirectoryAt(OldParts, High(OldParts), OldPath);
  var OldName := OldParts[High(OldParts)];
  var Node := OldParent.FindChild(OldName);
  if Node = nil then
    raise MemoryFileError('path not found', OldPath, TIOErrorKind.ekNotFound);
  if IsWithin(NewParts, OldParts) then
  begin
    if Length(NewParts) = Length(OldParts) then
      Exit;
    raise MemoryFileError('cannot move a path into itself', NewPath, TIOErrorKind.ekOther);
  end;
  if Length(NewParts) = 0 then
    raise MemoryFileError('path already exists', NewPath, TIOErrorKind.ekAlreadyExists);
  var NewParent := DirectoryAt(NewParts, High(NewParts), NewPath);
  var NewName := NewParts[High(NewParts)];
  if NewParent.FindChild(NewName) <> nil then
    raise MemoryFileError('path already exists', NewPath, TIOErrorKind.ekAlreadyExists);
  var AtTime := MemoryFileNow;
  NewParent.AddChild(NewName, Node, AtTime);
  OldParent.Children.Remove(OldName);
  OldParent.Modified := AtTime;
end;

procedure TMemoryFileSystem.CopyFile(const Source, Dest: string);
begin
  var Src := NodeAt(PathParts(Source), Source);
  if Src.Kind <> TFileKind.fkFile then
    raise MemoryFileError('cannot copy a directory as a file', Source, TIOErrorKind.ekIsDirectory);
  var Parts := PathParts(Dest);
  if Length(Parts) = 0 then
    raise MemoryFileError('cannot replace a directory', Dest, TIOErrorKind.ekIsDirectory);
  var Parent := DirectoryAt(Parts, High(Parts), Dest);
  var Name := Parts[High(Parts)];
  var Target := Parent.FindChild(Name);
  if Target = Src then
    Exit;
  if Target = nil then
  begin
    Target := TNode.Create(TFileKind.fkFile);
    Target.Bytes := System.Copy(Src.Bytes);
    Parent.AddChild(Name, Target, Target.Created);
  end
  else
  begin
    if Target.Kind <> TFileKind.fkFile then
      raise MemoryFileError('cannot replace a directory', Dest, TIOErrorKind.ekIsDirectory);
    Target.CheckWritable(Dest);
    var AtTime := MemoryFileNow;
    Target.Bytes := System.Copy(Src.Bytes);
    Target.Modified := AtTime;
  end;
end;

procedure TMemoryFileSystem.SetTime(const Path: string; Kind: TFileTimeKind; const Nanos: Int64);
begin
  var Node := NodeAt(PathParts(Path), Path);
  case Kind of
    TFileTimeKind.ftCreation: Node.Created := Nanos;
    TFileTimeKind.ftLastAccess: Node.Accessed := Nanos;
    TFileTimeKind.ftLastWrite: Node.Modified := Nanos;
  end;
end;

procedure TMemoryFileSystem.SetReadOnly(const Path: string; ReadOnly: Boolean);
begin
  NodeAt(PathParts(Path), Path).ReadOnly := ReadOnly;
end;

function TMemoryFileSystem.Canonicalize(const Path: string): string;
begin
  var Parts := PathParts(Path);
  NodeAt(Parts, Path);
  Result := '/' + TStringHelper.Join('/', Parts);
end;
