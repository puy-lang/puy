// puy-test: no-fs
program FsInstall;

// A file system written in Pascal installed in place of the host's
// (spec/stdlib/System.SysUtils/FileSystem.md): without the host's file system, the file
// functions of System.SysUtils, TFileStream, TFile / TDirectory and TFilePath all go through
// the installed one; its EInOutError (and ENotSupportedException) is False for System.SysUtils and the usual exceptions for
// the streams and System.IOUtils; after Uninstall the host's (missing) file system is back.

uses
  System.SysUtils, System.Classes, System.IOUtils, System.Generics.Collections, Puy.Paths;

type
  // A file or a directory of the file system in memory.
  TNode = class(TRefObject)
  public
    IsDir: Boolean;
    Data: TBytes;
    ReadOnly: Boolean;
    Modified: Int64;
  end;

  // An open file of the file system in memory.
  TMemFile = class(TRefObject, IFileHandle)
  private
    FPath: string;
    FNode: TNode;
    FWritable: Boolean;
    FPos: Int64;
  public
    constructor Create(const APath: string; ANode: TNode; AWritable: Boolean);
    function Read(Count: Integer): TArray<Byte>;
    procedure Write(const Bytes: TArray<Byte>; Offset, Count: Integer);
    function Seek(const Offset: Int64; Origin: TFileSeekOrigin): Int64;
    function GetSize: Int64;
    procedure SetSize(const Size: Int64);
    procedure Flush;
    procedure Close;
  end;

  // A file system in memory: the paths as written (without a trailing '/') are the keys; ''
  // is the root.
  TMemFS = class(TRefObject, IFileSystem)
  private
    FNodes: TDictionary<string, TNode>;
    FOrder: TList<string>;
    function Find(const Path: string): TNode;
    function Parent(const Path: string): TNode;
    procedure Add(const Path: string; Node: TNode);
    procedure Remove(const Path: string);
  public
    constructor Create;
    destructor Destroy; override;
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

function Fail(const Op, Path: string; Kind: TIOErrorKind): EInOutError;
begin
  Result := EInOutError.Create('memfs cannot ' + Op + ' ''' + Path + '''');
  Result.ErrorKind := Kind;
end;

// Path as a key: without `./` at the start and `/` at the end (`.` is the root).
function Norm(const Path: string): string;
begin
  Result := Path;
  if Result = '.' then
    Exit('');
  if Copy(Result, 1, 2) = './' then
    Result := Copy(Result, 3, MaxInt);
  while (Length(Result) > 0) and (Result[Length(Result)] = '/') do
    SetLength(Result, Length(Result) - 1);
end;

function DirOf(const Path: string): string;
begin
  var I := Length(Path);
  while (I > 0) and (Path[I] <> '/') do
    Dec(I);
  Result := Copy(Path, 1, I - 1);
end;

function NameOf(const Path: string): string;
begin
  Result := Copy(Path, Length(DirOf(Path)) + 2, MaxInt);
  if DirOf(Path) = '' then
    Result := Path;
end;

{ TMemFile }

constructor TMemFile.Create(const APath: string; ANode: TNode; AWritable: Boolean);
begin
  inherited Create;
  FPath := APath;
  FNode := ANode;
  FWritable := AWritable;
end;

function TMemFile.Read(Count: Integer): TArray<Byte>;
begin
  var N := Length(FNode.Data) - FPos;
  // greedy.bin breaks the contract: one byte more than asked
  if (N > Count) and (NameOf(FPath) <> 'greedy.bin') then
    N := Count;
  if N < 0 then
    N := 0;
  SetLength(Result, N);
  for var I := 0 to N - 1 do
    Result[I] := FNode.Data[FPos + I];
  Inc(FPos, N);
end;

procedure TMemFile.Write(const Bytes: TArray<Byte>; Offset, Count: Integer);
begin
  if not FWritable then
    raise Fail('write', FPath, TIOErrorKind.ekPermissionDenied);
  if FPos + Count > Length(FNode.Data) then
    SetLength(FNode.Data, FPos + Count);
  for var I := 0 to Count - 1 do
    FNode.Data[FPos + I] := Bytes[Offset + I];
  Inc(FPos, Count);
end;

function TMemFile.Seek(const Offset: Int64; Origin: TFileSeekOrigin): Int64;
begin
  case Origin of
    TFileSeekOrigin.fsoBeginning: FPos := Offset;
    TFileSeekOrigin.fsoCurrent: FPos := FPos + Offset;
    TFileSeekOrigin.fsoEnd: FPos := Length(FNode.Data) + Offset;
  end;
  Result := FPos;
end;

function TMemFile.GetSize: Int64;
begin
  Result := Length(FNode.Data);
end;

procedure TMemFile.SetSize(const Size: Int64);
begin
  SetLength(FNode.Data, Size);
end;

procedure TMemFile.Flush;
begin
end;

procedure TMemFile.Close;
begin
  WriteLn('  (memfs closed ', FPath, ')');
end;

{ TMemFS }

constructor TMemFS.Create;
begin
  inherited Create;
  FNodes := TDictionary<string, TNode>.Create;
  FOrder := TList<string>.Create;
  var Root := TNode.Create;
  Root.IsDir := True;
  FNodes.Add('', Root);
end;

destructor TMemFS.Destroy;
begin
  FOrder.Free;
  FNodes.Free;
  WriteLn('  (memfs destroyed)');
  inherited Destroy;
end;

function TMemFS.Find(const Path: string): TNode;
begin
  if not FNodes.TryGetValue(Norm(Path), Result) then
    Result := nil;
end;

// The directory Path goes in; EInOutError if there is none.
function TMemFS.Parent(const Path: string): TNode;
begin
  Result := Find(DirOf(Norm(Path)));
  if (Result = nil) or not Result.IsDir then
    raise Fail('find the directory of', Path, TIOErrorKind.ekNotFound);
end;

procedure TMemFS.Add(const Path: string; Node: TNode);
begin
  FNodes.AddOrSetValue(Norm(Path), Node);
  if not FOrder.Contains(Norm(Path)) then
    FOrder.Add(Norm(Path));
end;

procedure TMemFS.Remove(const Path: string);
begin
  FNodes.Remove(Norm(Path));
  FOrder.Remove(Norm(Path));
end;

function TMemFS.OpenFile(const Path: string; Mode: TFileOpenMode): IFileHandle;
begin
  var Node := Find(Path);
  if Mode = TFileOpenMode.omCreate then
  begin
    Parent(Path);
    if (Node <> nil) and Node.IsDir then
      raise Fail('create', Path, TIOErrorKind.ekIsDirectory);
    if Node = nil then
    begin
      Node := TNode.Create;
      Add(Path, Node);
    end;
    Node.Data := nil;
  end
  else if Node = nil then
    raise Fail('open', Path, TIOErrorKind.ekNotFound)
  else if Node.IsDir then
    raise Fail('open', Path, TIOErrorKind.ekIsDirectory);
  WriteLn('  (memfs open ', Path, ' ', Mode, ')');
  Result := TMemFile.Create(Path, Node, Mode <> TFileOpenMode.omRead);
end;

function TMemFS.GetInfo(const Path: string; FollowLink: Boolean): TFileSystemInfo;
begin
  var Node := Find(Path);
  if Node = nil then
    raise Fail('read the attributes of', Path, TIOErrorKind.ekNotFound);
  Result := Default(TFileSystemInfo);
  if Node.IsDir then
    Result.Kind := TFileKind.fkDirectory
  else
    Result.Kind := TFileKind.fkFile;
  Result.Size := Length(Node.Data);
  Result.ReadOnly := Node.ReadOnly;
  Result.HasModified := True;
  Result.Modified := Node.Modified;
end;

function TMemFS.ListDirectory(const Path: string): TArray<TFileSystemEntry>;
begin
  var Dir := Find(Path);
  if Dir = nil then
    raise Fail('list', Path, TIOErrorKind.ekNotFound);
  Result := nil;
  // Newest first: System.IOUtils sorts the names
  for var I := FOrder.Count - 1 downto 0 do
    if DirOf(FOrder[I]) = Norm(Path) then
    begin
      var E := Default(TFileSystemEntry);
      E.Name := NameOf(FOrder[I]);
      if FNodes[FOrder[I]].IsDir then
        E.Kind := TFileKind.fkDirectory
      else
        E.Kind := TFileKind.fkFile;
      Result := Result + [E];
    end;
end;

procedure TMemFS.CreateDirectory(const Path: string; Recursive: Boolean);
begin
  var Node := Find(Path);
  if Node <> nil then
  begin
    if Recursive and Node.IsDir then
      Exit;
    raise Fail('create directory', Path, TIOErrorKind.ekAlreadyExists);
  end;
  if Recursive and (Find(DirOf(Norm(Path))) = nil) then
    CreateDirectory(DirOf(Norm(Path)), True);
  Parent(Path);
  Node := TNode.Create;
  Node.IsDir := True;
  Add(Path, Node);
end;

procedure TMemFS.RemoveDirectory(const Path: string);
begin
  var Node := Find(Path);
  if (Node = nil) or not Node.IsDir then
    raise Fail('remove directory', Path, TIOErrorKind.ekNotFound);
  if Length(ListDirectory(Path)) > 0 then
    raise Fail('remove the non-empty directory', Path, TIOErrorKind.ekOther);
  Remove(Path);
end;

procedure TMemFS.RemoveFile(const Path: string);
begin
  if Path = 'unsupported' then
  begin
    var E := ENotSupportedException.Create('memfs cannot remove anything named unsupported');
    E.ErrorKind := TIOErrorKind.ekUnsupported;
    raise E;
  end;
  var Node := Find(Path);
  if Node = nil then
    raise Fail('remove', Path, TIOErrorKind.ekNotFound);
  if Node.IsDir then
    raise Fail('remove', Path, TIOErrorKind.ekIsDirectory);
  Remove(Path);
end;

procedure TMemFS.Rename(const OldPath, NewPath: string);
begin
  var Node := Find(OldPath);
  if Node = nil then
    raise Fail('rename', OldPath, TIOErrorKind.ekNotFound);
  Parent(NewPath);
  // The node and everything under it
  var Old := Norm(OldPath);
  for var Key in FOrder.ToArray do
    if (Key = Old) or (Copy(Key, 1, Length(Old) + 1) = Old + '/') then
    begin
      var Moved := FNodes[Key];
      Remove(Key);
      Add(Norm(NewPath) + Copy(Key, Length(Old) + 1, MaxInt), Moved);
    end;
end;

procedure TMemFS.CopyFile(const Source, Dest: string);
begin
  var Node := Find(Source);
  if (Node = nil) or Node.IsDir then
    raise Fail('copy', Source, TIOErrorKind.ekNotFound);
  Parent(Dest);
  var Copied := TNode.Create;
  Copied.Data := System.Copy(Node.Data);
  Add(Dest, Copied);
end;

procedure TMemFS.SetTime(const Path: string; Kind: TFileTimeKind; const Nanos: Int64);
begin
  var Node := Find(Path);
  if Node = nil then
    raise Fail('set the time of', Path, TIOErrorKind.ekNotFound);
  if Kind = TFileTimeKind.ftLastWrite then
    Node.Modified := Nanos;
end;

procedure TMemFS.SetReadOnly(const Path: string; ReadOnly: Boolean);
begin
  var Node := Find(Path);
  if Node = nil then
    raise Fail('set the attributes of', Path, TIOErrorKind.ekNotFound);
  Node.ReadOnly := ReadOnly;
end;

function TMemFS.Canonicalize(const Path: string): string;
begin
  if Find(Path) = nil then
    raise Fail('resolve', Path, TIOErrorKind.ekNotFound);
  Result := '/mem/' + Norm(Path);
end;

// Whether a file system is installed (a routine, so that no temporary of the main block holds it).
function IsInstalled: Boolean;
begin
  Result := TFileSystem.Installed <> nil;
end;

procedure InstallMemFS;
begin
  TFileSystem.Install(TMemFS.Create);
end;

procedure Show(const What: string; const A: TArray<string>);
begin
  Write(What, ':');
  for var S in A do
    Write(' ', S);
  WriteLn;
end;

procedure Attempt(const What: string; Op: Integer);
begin
  try
    case Op of
      0: TFile.ReadAllText('missing.txt');
      1: TFileStream.Create('missing.txt', fmOpenRead);
      2:
        begin
          var S := TFileStream.Create('docs/a.txt', fmOpenRead);
          S.Write([1], 0, 1);
        end;
      3: TDirectory.GetFiles('nowhere');
      4: TFilePath('missing').Resolve;
      5: TFile.WriteAllText('nowhere/x.txt', 'x');
      6: FileExists('docs/a.txt');
      7:
        begin
          var S := TFileStream.Create('docs/greedy.bin', fmOpenRead);
          var B: TBytes;
          SetLength(B, 2);
          S.Read(B, 0, 2);
        end;
    end;
    WriteLn(What, ': no exception');
  except
    on E: EInOutError do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message, ' (', E.ErrorKind, ')');
    on E: ENotSupportedException do
      WriteLn(What, ': ', E.ClassName, ' (', E.ErrorKind, ')');
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  WriteLn('installed: ', IsInstalled);
  InstallMemFS;
  WriteLn('installed: ', IsInstalled);

  WriteLn('-- System.SysUtils');
  WriteLn('CreateDir docs: ', CreateDir('docs'));
  WriteLn('CreateDir docs again: ', CreateDir('docs'));
  WriteLn('CreateDir x/y: ', CreateDir('x/y'));
  WriteLn('ForceDirectories a/b/c: ', ForceDirectories('a/b/c'));
  WriteLn('DirectoryExists a/b: ', DirectoryExists('a/b'));
  WriteLn('FileExists a/b: ', FileExists('a/b'));
  WriteLn('RemoveDir a: ', RemoveDir('a'));
  WriteLn('RemoveDir a/b/c: ', RemoveDir('a/b/c'));
  WriteLn('RenameFile a -> e: ', RenameFile('a', 'e'));
  WriteLn('DeleteFile unsupported: ', DeleteFile('unsupported'));
  WriteLn('DirectoryExists e/b: ', DirectoryExists('e/b'));

  WriteLn('-- TFileStream');
  var S := TFileStream.Create('docs/s.bin', fmCreate);
  S.Write([1, 2, 3, 4, 5], 0, 5);
  WriteLn('Size: ', S.Size, ' Position: ', S.Position);
  S.Position := 1;
  var B: TBytes;
  SetLength(B, 3);
  WriteLn('Read: ', S.Read(B, 0, 3), ' ', B[0], ' ', B[1], ' ', B[2]);
  S.Size := 2;
  WriteLn('Size: ', S.Size, ' Position: ', S.Position);
  WriteLn('Handle nil: ', S.Handle = nil);
  S.Close;
  WriteLn('FileExists docs/s.bin: ', FileExists('docs/s.bin'));

  WriteLn('-- System.IOUtils');
  TFile.WriteAllText('docs/a.txt', 'hello');
  WriteLn(TFile.ReadAllText('docs/a.txt'));
  WriteLn('GetSize: ', TFile.GetSize('docs/a.txt'));
  TFile.Copy('docs/a.txt', 'docs/c.txt');
  TFile.Move('docs/c.txt', 'docs/b.txt');
  WriteLn('Exists c b: ', TFile.Exists('docs/c.txt'), ' ', TFile.Exists('docs/b.txt'));
  TFile.SetLastWriteTimeUtc('docs/b.txt', EncodeDate(2026, 11, 21));
  WriteLn('LastWrite: ', FormatDateTime('yyyy-mm-dd', TFile.GetLastWriteTimeUtc('docs/b.txt')));
  TFile.SetAttributes('docs/b.txt', [TFileAttribute.faReadOnly]);
  WriteLn('ReadOnly: ', TFileAttribute.faReadOnly in TFile.GetAttributes('docs/b.txt'));
  Show('GetFiles docs', TDirectory.GetFiles('docs'));
  Show('GetDirectories .', TDirectory.GetDirectories('.'));
  TDirectory.Copy('docs', 'docs2');
  TDirectory.Move('docs2', 'docs3');
  Show('GetFiles docs3', TDirectory.GetFiles('docs3'));
  WriteLn('Exists docs2: ', TDirectory.Exists('docs2'));
  TDirectory.Delete('docs3', True);
  TDirectory.CreateDirectory('e/b/d');
  TFile.WriteAllText('e/b/d/x.txt', 'x');
  TDirectory.Delete('e', True);
  WriteLn('Exists e: ', TDirectory.Exists('e'));
  TFile.Delete('docs/b.txt');
  WriteLn('DeleteFile docs/s.bin: ', DeleteFile('docs/s.bin'));
  Show('GetFiles docs', TDirectory.GetFiles('docs'));

  WriteLn('-- TFilePath');
  var P: TFilePath := 'docs';
  WriteLn('Resolve: ', (P / 'a.txt').Resolve.ToString);
  WriteLn('IsDirectory: ', P.IsDirectory, ' IsFile: ', (P / 'a.txt').IsFile);
  WriteLn((P / 'a.txt').ReadAllText);

  WriteLn('-- failures');
  Attempt('ReadAllText missing', 0);
  Attempt('TFileStream missing', 1);
  Attempt('Write read-only stream', 2);
  Attempt('GetFiles nowhere', 3);
  Attempt('Resolve missing', 4);
  Attempt('WriteAllText nowhere/x.txt', 5);
  TFile.WriteAllBytes('docs/greedy.bin', [1, 2, 3]);
  Attempt('Read more than asked', 7);

  WriteLn('-- uninstalled');
  // The last reference goes: the file system is destroyed here
  TFileSystem.Uninstall;
  WriteLn('installed: ', IsInstalled);
  Attempt('FileExists docs/a.txt', 6);
  Attempt('ReadAllText missing.txt', 0);
end.
