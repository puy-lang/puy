// The streams (docs/spec/stdlib/System.Classes/TStream.md). The files are the Rust handles of
// System.Classes.Native, or the files of a file system a program installed (TFileSystem of
// System.SysUtils); the streams in memory copy bytes with BytesCopy / BytesFill, which write a
// dynamic array of bytes in place (docs/spec/bytecode.md 2.22).

const
  // The blocks of TMemoryStream: the first is at least MinChunk bytes, each new one as large as
  // all before it (the stream doubles), up to MaxChunk bytes
  MinChunk = 256;
  MaxChunk = 64 * 1024 * 1024;
  // The bytes CopyFrom moves at a time between streams not in memory
  CopyChunk = 64 * 1024;

type
  // Which exception a failure of the host becomes
  TStreamFailure = (sfStream, sfRead, sfWrite, sfOpen, sfCreate);

// The stream exception for the host's failure E (its message, kind and code).
function StreamError(const E: EInOutError; Failure: TStreamFailure): EStreamError;
begin
  case Failure of
    TStreamFailure.sfRead:
      Result := EReadError.Create(E.Message);
    TStreamFailure.sfWrite:
      Result := EWriteError.Create(E.Message);
    TStreamFailure.sfOpen:
      Result := EFOpenError.Create(E.Message);
    TStreamFailure.sfCreate:
      Result := EFCreateError.Create(E.Message);
  else
    Result := EStreamError.Create(E.Message);
  end;
  Result.ErrorKind := E.ErrorKind;
  Result.ErrorCode := E.ErrorCode;
end;

procedure RaiseClosed;
begin
  raise EStreamError.Create('the stream is closed');
end;

procedure RaiseBeforeStart(const Pos: Int64);
begin
  raise EStreamError.Create('cannot seek to ' + IntToStr(Pos) + ', before the start of the stream');
end;

procedure RaiseShortRead(const Count: Int64);
begin
  raise EReadError.Create('the stream ended before ' + IntToStr(Count) + ' bytes were read');
end;

procedure RaiseShortWrite(const Count: Int64);
begin
  raise EWriteError.Create('the stream took fewer than ' + IntToStr(Count) + ' bytes');
end;

function Smaller(const A, B: Int64): Int64;
begin
  if A < B then
    Result := A
  else
    Result := B;
end;

{ TStream }

function TStream.GetPosition: Int64;
begin
  Result := Seek(0, TSeekOrigin.soCurrent);
end;

procedure TStream.SetPosition(const Pos: Int64);
begin
  Seek(Pos, TSeekOrigin.soBeginning);
end;

function TStream.GetSize: Int64;
begin
  var Pos := Seek(0, TSeekOrigin.soCurrent);
  Result := Seek(0, TSeekOrigin.soEnd);
  Seek(Pos, TSeekOrigin.soBeginning);
end;

procedure TStream.SetSize(const NewSize: Int64);
begin
  raise EStreamError.Create('the size of a ' + ClassName + ' cannot be set');
end;

function TStream.GetCanSeek: Boolean;
begin
  Result := True;
end;

class procedure TStream.CheckBufferRange(const Buffer: TBytes; Offset, Count: Integer);
begin
  if (Offset < 0) or (Count < 0) or (Int64(Offset) + Count > Length(Buffer)) then
    raise ERangeError.Create('bytes ' + IntToStr(Offset) + '..' +
      IntToStr(Int64(Offset) + Count - 1) + ' are out of a buffer of ' +
      IntToStr(Length(Buffer)) + ' bytes');
end;

function TStream.Read(var Buffer: TBytes; Count: Integer): Integer;
begin
  Result := Read(Buffer, 0, Count);
end;

function TStream.Write(const Buffer: TBytes; Count: Integer): Integer;
begin
  Result := Write(Buffer, 0, Count);
end;

procedure TStream.ReadBuffer(var Buffer: TBytes; Count: Integer);
begin
  ReadBuffer(Buffer, 0, Count);
end;

procedure TStream.ReadBuffer(var Buffer: TBytes; Offset, Count: Integer);
begin
  CheckBufferRange(Buffer, Offset, Count);
  // A stream may give fewer bytes than asked before its end
  var Done := 0;
  while Done < Count do
  begin
    var N := Read(Buffer, Offset + Done, Count - Done);
    if N <= 0 then
      RaiseShortRead(Count);
    Inc(Done, N);
  end;
end;

procedure TStream.WriteBuffer(const Buffer: TBytes; Count: Integer);
begin
  WriteBuffer(Buffer, 0, Count);
end;

procedure TStream.WriteBuffer(const Buffer: TBytes; Offset, Count: Integer);
begin
  CheckBufferRange(Buffer, Offset, Count);
  if Write(Buffer, Offset, Count) <> Count then
    RaiseShortWrite(Count);
end;

function TStream.CopyFrom(const Source: TStream; Count: Int64): Int64;
begin
  if Count < 0 then
    raise EStreamError.Create('cannot copy ' + IntToStr(Count) + ' bytes');
  if Count = 0 then
  begin
    Source.Position := 0;
    Count := Source.Size;
  end;
  Result := Count;
  if Source is TMemoryStream then
  begin
    // Straight from its blocks
    var M := TMemoryStream(Source);
    M.CheckOpen;
    if M.FPosition + Count > M.FSize then
      RaiseShortRead(Count);
    M.CopyTo(Self, M.FPosition, Count);
    M.FPosition := M.FPosition + Count;
    Exit;
  end;
  var Buffer: TBytes;
  SetLength(Buffer, Smaller(Count, CopyChunk));
  while Count > 0 do
  begin
    var N := Integer(Smaller(Count, CopyChunk));
    Source.ReadBuffer(Buffer, 0, N);
    WriteBuffer(Buffer, 0, N);
    Count := Count - N;
  end;
end;

procedure TStream.Close;
begin
end;

procedure TStream.WriteText(const S: string);
begin
  WriteText(S, nil);
end;

procedure TStream.WriteText(const S: string; AEncoding: TEncoding);
begin
  if AEncoding = nil then
    AEncoding := TEncoding.Default;
  if S = '' then
    Exit;
  var B := AEncoding.GetBytes(S);
  WriteBuffer(B, Length(B));
end;

function TStream.ReadBits(var Bits: UInt64; Size, Count: Integer): Integer;
begin
  if Count <= 0 then
    Exit(0);
  if Length(FScratch) < Count then
    SetLength(FScratch, Count);
  Result := Read(FScratch, 0, Count);
  for var I := 0 to Smaller(Result, Size) - 1 do
    Bits := (Bits and not (UInt64($FF) shl (8 * I))) or (UInt64(FScratch[I]) shl (8 * I));
end;

function TStream.WriteBits(Bits: UInt64; Size, Count: Integer): Integer;
begin
  if Count <= 0 then
    Exit(0);
  if Length(FScratch) < Count then
    SetLength(FScratch, Count);
  for var I := 0 to Count - 1 do
    if I < Size then
      FScratch[I] := (Bits shr (8 * I)) and $FF
    else
      FScratch[I] := 0;
  Result := Write(FScratch, 0, Count);
end;

type
  // Uses TFileSystem's protected factory to share the native handle adapter.
  THostFiles = class(TFileSystem)
  public
    class function Wrap(const Handle: TFileHandle): IFileHandle; static;
  end;

class function THostFiles.Wrap(const Handle: TFileHandle): IFileHandle;
begin
  Result := WrapHostFile(Handle);
end;

{ THandleStream }

constructor THandleStream.Create(const AHandle: TFileHandle);
begin
  inherited Create;
  FHandle := AHandle;
  if AHandle <> nil then
    FFile := THostFiles.Wrap(AHandle);
end;

destructor THandleStream.Destroy;
begin
  Close;
  inherited Destroy;
end;

procedure THandleStream.CheckOpen;
begin
  if FFile = nil then
    RaiseClosed;
end;

function THandleStream.Read(var Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  CheckOpen;
  CheckBufferRange(Buffer, Offset, Count);
  Result := 0;
  // The file system may give fewer bytes than asked before the end of the file
  while Result < Count do
  begin
    var Part: TBytes;
    try
      Part := FFile.Read(Count - Result);
    except
      on E: EInOutError do
        raise StreamError(E, TStreamFailure.sfRead);
    end;
    if Length(Part) = 0 then
      Break;
    // An installed file system that gives more than asked has moved past bytes the stream
    // cannot give back: an error rather than a silent loss
    if Length(Part) > Count - Result then
      raise EReadError.Create('cannot read file ''' + FFileName + ''': the file system gave ' +
        IntToStr(Length(Part)) + ' bytes for ' + IntToStr(Count - Result));
    BytesCopy(Buffer, Offset + Result, Part, 0, Length(Part));
    Inc(Result, Length(Part));
  end;
end;

function THandleStream.Write(const Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  CheckOpen;
  CheckBufferRange(Buffer, Offset, Count);
  if Count > 0 then
  try
    FFile.Write(Buffer, Offset, Count);
  except
    on E: EInOutError do
      raise StreamError(E, TStreamFailure.sfWrite);
  end;
  Result := Count;
end;

function THandleStream.Seek(const Offset: Int64; Origin: TSeekOrigin): Int64;
begin
  CheckOpen;
  if (Origin = TSeekOrigin.soBeginning) and (Offset < 0) then
    RaiseBeforeStart(Offset);
  try
    Result := FFile.Seek(Offset, TFileSeekOrigin(Ord(Origin)));
  except
    on E: EInOutError do
      raise StreamError(E, TStreamFailure.sfStream);
  end;
end;

function THandleStream.GetSize: Int64;
begin
  CheckOpen;
  try
    Result := FFile.GetSize;
  except
    on E: EInOutError do
      raise StreamError(E, TStreamFailure.sfStream);
  end;
end;

procedure THandleStream.SetSize(const NewSize: Int64);
begin
  CheckOpen;
  if NewSize < 0 then
    raise EStreamError.Create('cannot set the size to ' + IntToStr(NewSize));
  try
    FFile.SetSize(NewSize);
    // A position past the new end moves to it
    if FFile.Seek(0, TFileSeekOrigin.fsoCurrent) > NewSize then
      FFile.Seek(NewSize, TFileSeekOrigin.fsoBeginning);
  except
    on E: EInOutError do
      raise StreamError(E, TStreamFailure.sfStream);
  end;
end;

procedure THandleStream.Close;
begin
  if FFile = nil then
    Exit;
  var F := FFile;
  FFile := nil;
  FHandle := nil;
  try
    try
      F.Flush;
    except
      on E: EInOutError do
        raise StreamError(E, TStreamFailure.sfWrite);
    end;
  finally
    F.Close;
  end;
end;

{ TFileStream }

// The file AFileName opened as Mode asks (TFileStream.Create), by the installed file system if
// there is one; Handle is the host's file (nil for the installed file system's).
function OpenByMode(const AFileName: string; Mode: Word; out Handle: TFileHandle): IFileHandle;
begin
  var Creates := Mode and fmCreate = fmCreate;
  var How := Mode and 3;
  if Creates then
    How := 3
  else if How = 3 then
    raise EFOpenError.Create('cannot open file ''' + AFileName + ''': the mode ' +
      IntToStr(Mode) + ' is not a way to open a file');
  Handle := nil;
  try
    var FS := TFileSystem.Installed;
    if FS <> nil then
      Result := FS.OpenFile(AFileName, TFileOpenMode(How))
    else
    begin
      Handle := OpenFile(AFileName, How);
      Result := THostFiles.Wrap(Handle);
    end;
  except
    on E: EInOutError do
      if Creates then
        raise StreamError(E, TStreamFailure.sfCreate)
      else
        raise StreamError(E, TStreamFailure.sfOpen);
  end;
  // An installed file system that answers nil leaves nothing to read or write
  if Result = nil then
    raise EFOpenError.Create('cannot open file ''' + AFileName +
      ''': the file system gave no file');
end;

constructor TFileStream.Create(const AFileName: string; Mode: Word);
begin
  inherited Create(nil);
  FFile := OpenByMode(AFileName, Mode, FHandle);
  FFileName := AFileName;
end;

constructor TFileStream.Create(const AFileName: string; Mode: Word; Rights: Cardinal);
begin
  inherited Create(nil);
  FFile := OpenByMode(AFileName, Mode, FHandle);
  FFileName := AFileName;
end;

{ TMemoryStream }

procedure TMemoryStream.CheckOpen;
begin
  if FClosed then
    RaiseClosed;
end;

function TMemoryStream.GetSize: Int64;
begin
  CheckOpen;
  Result := FSize;
end;

// Room for NewCapacity bytes: new blocks of zeros (or the one block made longer).
procedure TMemoryStream.Reserve(const NewCapacity: Int64);
begin
  if NewCapacity <= FCapacity then
    Exit;
  if FSingle then
  begin
    if NewCapacity > MaxInt then
      raise EOutOfMemory.Create('a TBytesStream holds at most ' + IntToStr(MaxInt) + ' bytes');
    if Length(FChunks) = 0 then
    begin
      SetLength(FChunks, 1);
      SetLength(FStarts, 1);
      FStarts[0] := 0;
    end;
    // Double, unless more is asked
    var Cap := FCapacity * 2;
    if Cap < MinChunk then
      Cap := MinChunk;
    if Cap < NewCapacity then
      Cap := NewCapacity;
    if Cap > MaxInt then
      Cap := MaxInt;
    SetLength(FChunks[0], Cap);
    FCapacity := Cap;
    Exit;
  end;
  while FCapacity < NewCapacity do
  begin
    // As large as all the blocks before, or what is missing, within MinChunk..MaxChunk
    var Len := FCapacity;
    if Len < NewCapacity - FCapacity then
      Len := NewCapacity - FCapacity;
    if Len < MinChunk then
      Len := MinChunk;
    if Len > MaxChunk then
      Len := MaxChunk;
    var K := Length(FChunks);
    SetLength(FChunks, K + 1);
    SetLength(FStarts, K + 1);
    SetLength(FChunks[K], Len);
    FStarts[K] := FCapacity;
    FCapacity := FCapacity + Len;
  end;
end;

// The block that holds the byte at Pos (< FCapacity).
function TMemoryStream.ChunkAt(const Pos: Int64): Integer;
begin
  var Lo := 0;
  var Hi := Length(FStarts) - 1;
  while Lo < Hi do
  begin
    var Mid := (Lo + Hi + 1) div 2;
    if FStarts[Mid] <= Pos then
      Lo := Mid
    else
      Hi := Mid - 1;
  end;
  Result := Lo;
end;

// The Count bytes at Pos (within FCapacity) to Buffer from Offset.
procedure TMemoryStream.CopyOut(const Pos: Int64; var Buffer: TBytes; Offset, Count: Integer);
begin
  if Count <= 0 then
    Exit;
  var K := ChunkAt(Pos);
  var At := Integer(Pos - FStarts[K]);
  while Count > 0 do
  begin
    var N := Integer(Smaller(Count, Length(FChunks[K]) - At));
    BytesCopy(Buffer, Offset, FChunks[K], At, N);
    Inc(Offset, N);
    Dec(Count, N);
    Inc(K);
    At := 0;
  end;
end;

// The Count bytes of Buffer from Offset to Pos (within FCapacity).
procedure TMemoryStream.CopyIn(const Pos: Int64; const Buffer: TBytes; Offset, Count: Integer);
begin
  if Count <= 0 then
    Exit;
  var K := ChunkAt(Pos);
  var At := Integer(Pos - FStarts[K]);
  while Count > 0 do
  begin
    var N := Integer(Smaller(Count, Length(FChunks[K]) - At));
    BytesCopy(FChunks[K], At, Buffer, Offset, N);
    Inc(Offset, N);
    Dec(Count, N);
    Inc(K);
    At := 0;
  end;
end;

// Writes the Count bytes at From (within FSize) to Stream, block by block.
procedure TMemoryStream.CopyTo(const Stream: TStream; const From, Count: Int64);
begin
  if Count <= 0 then
    Exit;
  var Left := Count;
  var K := ChunkAt(From);
  var At := Integer(From - FStarts[K]);
  while Left > 0 do
  begin
    var N := Integer(Smaller(Left, Length(FChunks[K]) - At));
    Stream.WriteBuffer(FChunks[K], At, N);
    Left := Left - N;
    Inc(K);
    At := 0;
  end;
end;

procedure TMemoryStream.SetSize(const NewSize: Int64);
begin
  CheckOpen;
  if NewSize < 0 then
    raise EStreamError.Create('cannot set the size to ' + IntToStr(NewSize));
  if NewSize > FCapacity then
    Reserve(NewSize);
  if NewSize < FSize then
  begin
    // The bytes past the end are zeros, as a new block's: let the blocks past it go, and clear
    // the rest of the last one
    if FSingle then
    begin
      // Cut the one array: a shared one (the caller's ABytes) is copied and keeps its bytes;
      // growing again adds zeros
      if NewSize = 0 then
      begin
        FChunks := nil;
        FStarts := nil;
      end
      else
        SetLength(FChunks[0], Integer(NewSize));
      FCapacity := NewSize;
    end
    else
    begin
      var Keep := 0;
      if NewSize > 0 then
        Keep := ChunkAt(NewSize - 1) + 1;
      SetLength(FChunks, Keep);
      SetLength(FStarts, Keep);
      if Keep = 0 then
        FCapacity := 0
      else
      begin
        FCapacity := FStarts[Keep - 1] + Length(FChunks[Keep - 1]);
        var At := Integer(NewSize - FStarts[Keep - 1]);
        var Used := Integer(Smaller(FSize, FCapacity) - FStarts[Keep - 1]);
        BytesFill(FChunks[Keep - 1], At, Used - At, 0);
      end;
    end;
  end;
  FSize := NewSize;
  if FPosition > NewSize then
    FPosition := NewSize;
end;

function TMemoryStream.Read(var Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  CheckOpen;
  CheckBufferRange(Buffer, Offset, Count);
  Result := Integer(Smaller(Count, FSize - FPosition));
  if Result <= 0 then
    Exit(0);
  CopyOut(FPosition, Buffer, Offset, Result);
  FPosition := FPosition + Result;
end;

function TMemoryStream.Write(const Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  CheckOpen;
  CheckBufferRange(Buffer, Offset, Count);
  if Count = 0 then
    Exit(0);
  Reserve(FPosition + Count);
  CopyIn(FPosition, Buffer, Offset, Count);
  FPosition := FPosition + Count;
  if FPosition > FSize then
    FSize := FPosition;
  Result := Count;
end;

function TMemoryStream.Seek(const Offset: Int64; Origin: TSeekOrigin): Int64;
begin
  CheckOpen;
  case Origin of
    TSeekOrigin.soBeginning:
      Result := Offset;
    TSeekOrigin.soCurrent:
      Result := FPosition + Offset;
  else
    Result := FSize + Offset;
  end;
  if Result < 0 then
    RaiseBeforeStart(Result);
  FPosition := Result;
end;

procedure TMemoryStream.Close;
begin
  FChunks := nil;
  FStarts := nil;
  FCapacity := 0;
  FSize := 0;
  FPosition := 0;
  FClosed := True;
end;

procedure TMemoryStream.Clear;
begin
  CheckOpen;
  FChunks := nil;
  FStarts := nil;
  FCapacity := 0;
  FSize := 0;
  FPosition := 0;
end;

procedure TMemoryStream.LoadFromStream(const Stream: TStream);
begin
  CheckOpen;
  Stream.Position := 0;
  var N := Stream.Size;
  Clear;
  // One block of the size when it fits
  Reserve(N);
  if N > 0 then
    CopyFrom(Stream, N);
  FPosition := 0;
end;

procedure TMemoryStream.LoadFromFile(const FileName: string);
begin
  LoadFromStream(TFileStream.Create(FileName, fmOpenRead));
end;

procedure TMemoryStream.SaveToStream(const Stream: TStream);
begin
  CheckOpen;
  CopyTo(Stream, 0, FSize);
end;

procedure TMemoryStream.SaveToFile(const FileName: string);
begin
  var F := TFileStream.Create(FileName, fmCreate);
  SaveToStream(F);
  F.Close;
end;

function TMemoryStream.ToBytes: TBytes;
begin
  CheckOpen;
  if FSize > MaxInt then
    raise EOutOfMemory.Create('the ' + IntToStr(FSize) + ' bytes of the stream do not fit in ' +
      'one TBytes');
  SetLength(Result, Integer(FSize));
  CopyOut(0, Result, 0, Integer(FSize));
end;

{ TBytesStream }

constructor TBytesStream.Create;
begin
  inherited Create;
  FSingle := True;
end;

constructor TBytesStream.Create(const ABytes: TBytes);
begin
  inherited Create;
  FSingle := True;
  if Length(ABytes) > 0 then
  begin
    FChunks := [ABytes];
    FStarts := [0];
    FCapacity := Length(ABytes);
    FSize := Length(ABytes);
  end;
end;

function TBytesStream.GetBytes: TBytes;
begin
  CheckOpen;
  if FSize = 0 then
    Exit(nil);
  // Size bytes: the room past the end goes (the bytes are kept, not copied, when the array is
  // not shared)
  if FCapacity > FSize then
  begin
    SetLength(FChunks[0], Integer(FSize));
    FCapacity := FSize;
  end;
  Result := FChunks[0];
end;

{ TStringStream }

procedure TStringStream.Init(const AString: string; AEncoding: TEncoding;
  AOwnsEncoding: Boolean);
begin
  if AEncoding = nil then
  begin
    FEncoding := TEncoding.Default;
    FOwnsEncoding := False;
  end
  else
  begin
    FEncoding := AEncoding;
    FOwnsEncoding := AOwnsEncoding and not TEncoding.IsStandardEncoding(AEncoding);
  end;
  if AString <> '' then
  begin
    WriteString(AString);
    FPosition := 0;
  end;
end;

constructor TStringStream.Create;
begin
  inherited Create;
  Init('', nil, False);
end;

constructor TStringStream.Create(const AString: string);
begin
  inherited Create;
  Init(AString, nil, False);
end;

constructor TStringStream.Create(const AString: string; AEncoding: TEncoding;
  AOwnsEncoding: Boolean);
begin
  inherited Create;
  Init(AString, AEncoding, AOwnsEncoding);
end;

constructor TStringStream.Create(const ABytes: TBytes);
begin
  inherited Create;
  Init('', nil, False);
  WriteBuffer(ABytes, Length(ABytes));
  FPosition := 0;
end;

destructor TStringStream.Destroy;
begin
  if FOwnsEncoding then
    FEncoding.Free;
  inherited Destroy;
end;

function TStringStream.GetDataString: string;
begin
  Result := FEncoding.GetString(ToBytes);
end;

function TStringStream.GetBytes: TBytes;
begin
  Result := ToBytes;
end;

function TStringStream.ReadString(Count: Integer): string;
begin
  CheckOpen;
  var N := Integer(Smaller(Count, FSize - FPosition));
  if N <= 0 then
    Exit('');
  var B: TBytes;
  SetLength(B, N);
  CopyOut(FPosition, B, 0, N);
  FPosition := FPosition + N;
  Result := FEncoding.GetString(B);
end;

procedure TStringStream.WriteString(const AString: string);
begin
  var B := FEncoding.GetBytes(AString);
  WriteBuffer(B, Length(B));
end;

// The values of ReadData / WriteData as their bits: the integer casts keep the low bits
// (range checks off), the reals' bits are their IEEE 754 encodings.
{$R-}

function TStream.ReadData(var Buffer: Boolean): Integer;
begin
  Result := ReadData(Buffer, 1);
end;

function TStream.ReadData(var Buffer: Boolean; Count: Integer): Integer;
begin
  var Bits := UInt64(Ord(Buffer));
  Result := ReadBits(Bits, 1, Count);
  Buffer := Bits and $FF <> 0;
end;

function TStream.WriteData(const Buffer: Boolean): Integer;
begin
  Result := WriteBits(UInt64(Ord(Buffer)), 1, 1);
end;

function TStream.WriteData(const Buffer: Boolean; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Ord(Buffer)), 1, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Boolean);
begin
  ReadBufferData(Buffer, 1);
end;

procedure TStream.ReadBufferData(var Buffer: Boolean; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Boolean);
begin
  WriteBufferData(Buffer, 1);
end;

procedure TStream.WriteBufferData(const Buffer: Boolean; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: Char): Integer;
begin
  Result := ReadData(Buffer, 2);
end;

function TStream.ReadData(var Buffer: Char; Count: Integer): Integer;
begin
  var Bits := UInt64(Ord(Buffer));
  Result := ReadBits(Bits, 2, Count);
  Buffer := Char(Bits and $FFFF);
end;

function TStream.WriteData(const Buffer: Char): Integer;
begin
  Result := WriteBits(UInt64(Ord(Buffer)), 2, 2);
end;

function TStream.WriteData(const Buffer: Char; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Ord(Buffer)), 2, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Char);
begin
  ReadBufferData(Buffer, 2);
end;

procedure TStream.ReadBufferData(var Buffer: Char; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Char);
begin
  WriteBufferData(Buffer, 2);
end;

procedure TStream.WriteBufferData(const Buffer: Char; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: ShortInt): Integer;
begin
  Result := ReadData(Buffer, 1);
end;

function TStream.ReadData(var Buffer: ShortInt; Count: Integer): Integer;
begin
  var Bits := UInt64(Buffer);
  Result := ReadBits(Bits, 1, Count);
  Buffer := ShortInt(Bits);
end;

function TStream.WriteData(const Buffer: ShortInt): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 1, 1);
end;

function TStream.WriteData(const Buffer: ShortInt; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 1, Count);
end;

procedure TStream.ReadBufferData(var Buffer: ShortInt);
begin
  ReadBufferData(Buffer, 1);
end;

procedure TStream.ReadBufferData(var Buffer: ShortInt; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: ShortInt);
begin
  WriteBufferData(Buffer, 1);
end;

procedure TStream.WriteBufferData(const Buffer: ShortInt; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: Byte): Integer;
begin
  Result := ReadData(Buffer, 1);
end;

function TStream.ReadData(var Buffer: Byte; Count: Integer): Integer;
begin
  var Bits := UInt64(Buffer);
  Result := ReadBits(Bits, 1, Count);
  Buffer := Byte(Bits);
end;

function TStream.WriteData(const Buffer: Byte): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 1, 1);
end;

function TStream.WriteData(const Buffer: Byte; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 1, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Byte);
begin
  ReadBufferData(Buffer, 1);
end;

procedure TStream.ReadBufferData(var Buffer: Byte; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Byte);
begin
  WriteBufferData(Buffer, 1);
end;

procedure TStream.WriteBufferData(const Buffer: Byte; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: SmallInt): Integer;
begin
  Result := ReadData(Buffer, 2);
end;

function TStream.ReadData(var Buffer: SmallInt; Count: Integer): Integer;
begin
  var Bits := UInt64(Buffer);
  Result := ReadBits(Bits, 2, Count);
  Buffer := SmallInt(Bits);
end;

function TStream.WriteData(const Buffer: SmallInt): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 2, 2);
end;

function TStream.WriteData(const Buffer: SmallInt; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 2, Count);
end;

procedure TStream.ReadBufferData(var Buffer: SmallInt);
begin
  ReadBufferData(Buffer, 2);
end;

procedure TStream.ReadBufferData(var Buffer: SmallInt; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: SmallInt);
begin
  WriteBufferData(Buffer, 2);
end;

procedure TStream.WriteBufferData(const Buffer: SmallInt; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: Word): Integer;
begin
  Result := ReadData(Buffer, 2);
end;

function TStream.ReadData(var Buffer: Word; Count: Integer): Integer;
begin
  var Bits := UInt64(Buffer);
  Result := ReadBits(Bits, 2, Count);
  Buffer := Word(Bits);
end;

function TStream.WriteData(const Buffer: Word): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 2, 2);
end;

function TStream.WriteData(const Buffer: Word; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 2, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Word);
begin
  ReadBufferData(Buffer, 2);
end;

procedure TStream.ReadBufferData(var Buffer: Word; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Word);
begin
  WriteBufferData(Buffer, 2);
end;

procedure TStream.WriteBufferData(const Buffer: Word; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: Integer): Integer;
begin
  Result := ReadData(Buffer, 4);
end;

function TStream.ReadData(var Buffer: Integer; Count: Integer): Integer;
begin
  var Bits := UInt64(Buffer);
  Result := ReadBits(Bits, 4, Count);
  Buffer := Integer(Bits);
end;

function TStream.WriteData(const Buffer: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 4, 4);
end;

function TStream.WriteData(const Buffer: Integer; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 4, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Integer);
begin
  ReadBufferData(Buffer, 4);
end;

procedure TStream.ReadBufferData(var Buffer: Integer; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Integer);
begin
  WriteBufferData(Buffer, 4);
end;

procedure TStream.WriteBufferData(const Buffer: Integer; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: Cardinal): Integer;
begin
  Result := ReadData(Buffer, 4);
end;

function TStream.ReadData(var Buffer: Cardinal; Count: Integer): Integer;
begin
  var Bits := UInt64(Buffer);
  Result := ReadBits(Bits, 4, Count);
  Buffer := Cardinal(Bits);
end;

function TStream.WriteData(const Buffer: Cardinal): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 4, 4);
end;

function TStream.WriteData(const Buffer: Cardinal; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 4, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Cardinal);
begin
  ReadBufferData(Buffer, 4);
end;

procedure TStream.ReadBufferData(var Buffer: Cardinal; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Cardinal);
begin
  WriteBufferData(Buffer, 4);
end;

procedure TStream.WriteBufferData(const Buffer: Cardinal; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: Int64): Integer;
begin
  Result := ReadData(Buffer, 8);
end;

function TStream.ReadData(var Buffer: Int64; Count: Integer): Integer;
begin
  var Bits := UInt64(Buffer);
  Result := ReadBits(Bits, 8, Count);
  Buffer := Int64(Bits);
end;

function TStream.WriteData(const Buffer: Int64): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 8, 8);
end;

function TStream.WriteData(const Buffer: Int64; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 8, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Int64);
begin
  ReadBufferData(Buffer, 8);
end;

procedure TStream.ReadBufferData(var Buffer: Int64; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Int64);
begin
  WriteBufferData(Buffer, 8);
end;

procedure TStream.WriteBufferData(const Buffer: Int64; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: UInt64): Integer;
begin
  Result := ReadData(Buffer, 8);
end;

function TStream.ReadData(var Buffer: UInt64; Count: Integer): Integer;
begin
  var Bits := UInt64(Buffer);
  Result := ReadBits(Bits, 8, Count);
  Buffer := UInt64(Bits);
end;

function TStream.WriteData(const Buffer: UInt64): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 8, 8);
end;

function TStream.WriteData(const Buffer: UInt64; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(Buffer), 8, Count);
end;

procedure TStream.ReadBufferData(var Buffer: UInt64);
begin
  ReadBufferData(Buffer, 8);
end;

procedure TStream.ReadBufferData(var Buffer: UInt64; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: UInt64);
begin
  WriteBufferData(Buffer, 8);
end;

procedure TStream.WriteBufferData(const Buffer: UInt64; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: Single): Integer;
begin
  Result := ReadData(Buffer, 4);
end;

function TStream.ReadData(var Buffer: Single; Count: Integer): Integer;
begin
  var Bits := UInt64(SingleToBits(Buffer));
  Result := ReadBits(Bits, 4, Count);
  Buffer := BitsToSingle(Cardinal(Bits and $FFFFFFFF));
end;

function TStream.WriteData(const Buffer: Single): Integer;
begin
  Result := WriteBits(UInt64(SingleToBits(Buffer)), 4, 4);
end;

function TStream.WriteData(const Buffer: Single; Count: Integer): Integer;
begin
  Result := WriteBits(UInt64(SingleToBits(Buffer)), 4, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Single);
begin
  ReadBufferData(Buffer, 4);
end;

procedure TStream.ReadBufferData(var Buffer: Single; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Single);
begin
  WriteBufferData(Buffer, 4);
end;

procedure TStream.WriteBufferData(const Buffer: Single; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

function TStream.ReadData(var Buffer: Double): Integer;
begin
  Result := ReadData(Buffer, 8);
end;

function TStream.ReadData(var Buffer: Double; Count: Integer): Integer;
begin
  var Bits := DoubleToBits(Buffer);
  Result := ReadBits(Bits, 8, Count);
  Buffer := BitsToDouble(Bits);
end;

function TStream.WriteData(const Buffer: Double): Integer;
begin
  Result := WriteBits(DoubleToBits(Buffer), 8, 8);
end;

function TStream.WriteData(const Buffer: Double; Count: Integer): Integer;
begin
  Result := WriteBits(DoubleToBits(Buffer), 8, Count);
end;

procedure TStream.ReadBufferData(var Buffer: Double);
begin
  ReadBufferData(Buffer, 8);
end;

procedure TStream.ReadBufferData(var Buffer: Double; Count: Integer);
begin
  if ReadData(Buffer, Count) < Count then
    RaiseShortRead(Count);
end;

procedure TStream.WriteBufferData(const Buffer: Double);
begin
  WriteBufferData(Buffer, 8);
end;

procedure TStream.WriteBufferData(const Buffer: Double; Count: Integer);
begin
  if WriteData(Buffer, Count) < Count then
    RaiseShortWrite(Count);
end;

{$R+}
