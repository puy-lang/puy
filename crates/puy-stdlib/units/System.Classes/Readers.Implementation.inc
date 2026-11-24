// The readers and writers on streams (docs/spec/stdlib/System.Classes/Readers.md). Text is read a
// buffer of bytes at a time and decoded with TEncoding; a character cut by the end of a buffer
// waits for the next one.

const
  // The bytes TStreamReader reads at least at a time
  MinReadBuffer = 128;

// ERangeError unless the characters Index..Index + Count - 1 are in Buffer.
procedure CheckCharRange(const Buffer: TArray<Char>; Index, Count: Integer);
begin
  if (Index < 0) or (Count < 0) or (Int64(Index) + Count > Length(Buffer)) then
    raise ERangeError.Create('characters ' + IntToStr(Index) + '..' +
      IntToStr(Int64(Index) + Count - 1) + ' are out of a buffer of ' +
      IntToStr(Length(Buffer)) + ' characters');
end;

// The characters Index..Index + Count - 1 of Chars as a string.
function CharsToString(const Chars: TArray<Char>; Index, Count: Integer): string;
begin
  CheckCharRange(Chars, Index, Count);
  SetLength(Result, Count);
  for var I := 1 to Count do
    Result[I] := Chars[Index + I - 1];
end;

// How many of the bytes Start..Count - 1 of Bytes, from Start, are whole characters of
// Encoding: the bytes of a character cut by Count are left out.
function WholeCharBytes(const Encoding: TEncoding; const Bytes: TBytes;
  Start, Count: Integer): Integer;
begin
  Result := Count - Start;
  case Encoding.CodePage of
    65001:
      begin
        // The last byte that starts a sequence, and the length that byte announces
        var I := Count - 1;
        while (I >= Start) and (I > Count - 4) and (Bytes[I] and $C0 = $80) do
          Dec(I);
        if I < Start then
          Exit;
        var Need := 1;
        if Bytes[I] and $E0 = $C0 then
          Need := 2
        else if Bytes[I] and $F0 = $E0 then
          Need := 3
        else if Bytes[I] and $F8 = $F0 then
          Need := 4;
        if I + Need > Count then
          Result := I - Start;
      end;
    1200, 1201:
      Result := Result and not 1;
  end;
end;

{ TTextReader }

function TTextReader.ReadBlock(var Buffer: TArray<Char>; Index, Count: Integer): Integer;
begin
  CheckCharRange(Buffer, Index, Count);
  Result := 0;
  while Result < Count do
  begin
    var N := Read(Buffer, Index + Result, Count - Result);
    if N <= 0 then
      Break;
    Inc(Result, N);
  end;
end;

{ TTextWriter }

constructor TTextWriter.Create;
begin
  inherited Create;
  FNewLine := sLineBreak;
end;

procedure TTextWriter.Write(Value: Boolean);
begin
  Write(BoolToStr(Value, True));
end;

procedure TTextWriter.Write(Value: Char);
begin
  Write(string(Value));
end;

procedure TTextWriter.Write(const Value: TArray<Char>);
begin
  Write(CharsToString(Value, 0, Length(Value)));
end;

procedure TTextWriter.Write(const Value: TArray<Char>; Index, Count: Integer);
begin
  Write(CharsToString(Value, Index, Count));
end;

procedure TTextWriter.Write(Value: Integer);
begin
  Write(IntToStr(Value));
end;

procedure TTextWriter.Write(Value: Cardinal);
begin
  Write(IntToStr(Value));
end;

procedure TTextWriter.Write(Value: Int64);
begin
  Write(IntToStr(Value));
end;

procedure TTextWriter.Write(Value: UInt64);
begin
  Write(IntToStr(Value));
end;

procedure TTextWriter.Write(Value: Single);
begin
  Write(FloatToStr(Value));
end;

procedure TTextWriter.Write(Value: Double);
begin
  Write(FloatToStr(Value));
end;

procedure TTextWriter.Write(const Value: TObject);
begin
  Write(Value.ToString);
end;

procedure TTextWriter.WriteLine;
begin
  Write(FNewLine);
end;

procedure TTextWriter.WriteLine(const Value: string);
begin
  Write(Value + FNewLine);
end;

procedure TTextWriter.WriteLine(Value: Boolean);
begin
  Write(BoolToStr(Value, True) + FNewLine);
end;

procedure TTextWriter.WriteLine(Value: Char);
begin
  Write(Value + FNewLine);
end;

procedure TTextWriter.WriteLine(const Value: TArray<Char>);
begin
  Write(CharsToString(Value, 0, Length(Value)) + FNewLine);
end;

procedure TTextWriter.WriteLine(const Value: TArray<Char>; Index, Count: Integer);
begin
  Write(CharsToString(Value, Index, Count) + FNewLine);
end;

procedure TTextWriter.WriteLine(Value: Integer);
begin
  Write(IntToStr(Value) + FNewLine);
end;

procedure TTextWriter.WriteLine(Value: Cardinal);
begin
  Write(IntToStr(Value) + FNewLine);
end;

procedure TTextWriter.WriteLine(Value: Int64);
begin
  Write(IntToStr(Value) + FNewLine);
end;

procedure TTextWriter.WriteLine(Value: UInt64);
begin
  Write(IntToStr(Value) + FNewLine);
end;

procedure TTextWriter.WriteLine(Value: Single);
begin
  Write(FloatToStr(Value) + FNewLine);
end;

procedure TTextWriter.WriteLine(Value: Double);
begin
  Write(FloatToStr(Value) + FNewLine);
end;

procedure TTextWriter.WriteLine(const Value: TObject);
begin
  Write(Value.ToString + FNewLine);
end;

{ TStreamReader }

procedure TStreamReader.Init(const Stream: TStream; Encoding: TEncoding; DetectBOM: Boolean;
  BufferSize: Integer);
begin
  if Stream = nil then
    raise EArgumentException.Create('the stream of a TStreamReader is nil');
  FStream := Stream;
  if Encoding = nil then
    Encoding := TEncoding.Default;
  FEncoding := Encoding;
  FDetectBOM := DetectBOM;
  if BufferSize < MinReadBuffer then
    BufferSize := MinReadBuffer;
  SetLength(FBytes, BufferSize);
  FTextPos := 1;
end;

constructor TStreamReader.Create(const Stream: TStream);
begin
  inherited Create;
  Init(Stream, nil, True, 4096);
end;

constructor TStreamReader.Create(const Stream: TStream; DetectBOM: Boolean);
begin
  inherited Create;
  Init(Stream, nil, DetectBOM, 4096);
end;

constructor TStreamReader.Create(const Stream: TStream; Encoding: TEncoding;
  DetectBOM: Boolean; BufferSize: Integer);
begin
  inherited Create;
  Init(Stream, Encoding, DetectBOM, BufferSize);
end;

constructor TStreamReader.Create(const FileName: string);
begin
  inherited Create;
  Init(TFileStream.Create(FileName, fmOpenRead), nil, True, 4096);
end;

constructor TStreamReader.Create(const FileName: string; DetectBOM: Boolean);
begin
  inherited Create;
  Init(TFileStream.Create(FileName, fmOpenRead), nil, DetectBOM, 4096);
end;

constructor TStreamReader.Create(const FileName: string; Encoding: TEncoding;
  DetectBOM: Boolean; BufferSize: Integer);
begin
  inherited Create;
  Init(TFileStream.Create(FileName, fmOpenRead), Encoding, DetectBOM, BufferSize);
end;

procedure TStreamReader.CheckOpen;
begin
  if FStream = nil then
    raise EStreamError.Create('the reader is closed');
end;

procedure TStreamReader.Close;
begin
  if FStream = nil then
    Exit;
  var S := FStream;
  FStream := nil;
  FText := '';
  FTextPos := 1;
  FByteCount := 0;
  S.Close;
end;

procedure TStreamReader.DiscardBufferedData;
begin
  CheckOpen;
  FText := '';
  FTextPos := 1;
  FByteCount := 0;
  FEnded := False;
end;

// Reads more bytes after the FByteCount kept; False at the end of the stream.
function TStreamReader.ReadMore: Boolean;
begin
  if FByteCount = Length(FBytes) then
    SetLength(FBytes, 2 * Length(FBytes));
  var N := FStream.Read(FBytes, FByteCount, Length(FBytes) - FByteCount);
  Result := N > 0;
  if Result then
    Inc(FByteCount, N)
  else
    FEnded := True;
end;

// Decodes more text onto FText (dropping the text already read); False when the stream has no
// more.
function TStreamReader.Fill: Boolean;
begin
  if FTextPos > 1 then
  begin
    FText := Copy(FText, FTextPos, MaxInt);
    FTextPos := 1;
  end;
  Result := False;
  while not Result and not FEnded do
  begin
    ReadMore;
    var Start := 0;
    if not FStarted then
    begin
      // The longest byte order mark is 3 bytes
      while not FEnded and (FByteCount < 3) do
        ReadMore;
      FStarted := True;
      var Detected: TEncoding := nil;
      if not FDetectBOM then
        Detected := FEncoding;
      Start := TEncoding.GetBufferEncoding(Copy(FBytes, 0, FByteCount), Detected, FEncoding);
      FEncoding := Detected;
    end;
    var Whole := FByteCount - Start;
    if not FEnded then
      Whole := WholeCharBytes(FEncoding, FBytes, Start, FByteCount);
    if Whole > 0 then
    begin
      FText := FText + FEncoding.GetString(FBytes, Start, Whole);
      Result := True;
    end;
    // Keep the bytes of a cut character at the start
    var Kept := FByteCount - Start - Whole;
    for var I := 0 to Kept - 1 do
      FBytes[I] := FBytes[Start + Whole + I];
    FByteCount := Kept;
  end;
end;

function TStreamReader.GetEndOfStream: Boolean;
begin
  CheckOpen;
  Result := (FTextPos > Length(FText)) and not Fill;
end;

function TStreamReader.Peek: Integer;
begin
  CheckOpen;
  if (FTextPos > Length(FText)) and not Fill then
    Exit(-1);
  Result := Ord(FText[FTextPos]);
end;

function TStreamReader.Read: Integer;
begin
  CheckOpen;
  if (FTextPos > Length(FText)) and not Fill then
    Exit(-1);
  Result := Ord(FText[FTextPos]);
  Inc(FTextPos);
end;

function TStreamReader.Read(var Buffer: TArray<Char>; Index, Count: Integer): Integer;
begin
  CheckCharRange(Buffer, Index, Count);
  CheckOpen;
  if (Count = 0) or ((FTextPos > Length(FText)) and not Fill) then
    Exit(0);
  Result := Length(FText) - FTextPos + 1;
  if Result > Count then
    Result := Count;
  for var I := 0 to Result - 1 do
    Buffer[Index + I] := FText[FTextPos + I];
  Inc(FTextPos, Result);
end;

function TStreamReader.ReadLine: string;
begin
  CheckOpen;
  Result := '';
  if (FTextPos > Length(FText)) and not Fill then
    Exit;
  while True do
  begin
    var I := FTextPos;
    var Last := Length(FText);
    while (I <= Last) and (FText[I] <> #10) and (FText[I] <> #13) do
      Inc(I);
    Result := Result + Copy(FText, FTextPos, I - FTextPos);
    FTextPos := I;
    if I <= Last then
    begin
      Inc(FTextPos);
      // CR LF is one break, even when the LF is in the next buffer
      if (FText[I] = #13) and ((FTextPos <= Length(FText)) or Fill) and
        (FText[FTextPos] = #10) then
        Inc(FTextPos);
      Exit;
    end;
    if not Fill then
      Exit;
  end;
end;

function TStreamReader.ReadToEnd: string;
begin
  CheckOpen;
  Result := Copy(FText, FTextPos, MaxInt);
  FText := '';
  FTextPos := 1;
  while Fill do
  begin
    Result := Result + FText;
    FText := '';
  end;
end;

{ TStreamWriter }

procedure TStreamWriter.Init(const Stream: TStream; Encoding: TEncoding; BufferSize: Integer;
  AutoFlush: Boolean);
begin
  if Stream = nil then
    raise EArgumentException.Create('the stream of a TStreamWriter is nil');
  FStream := Stream;
  if Encoding = nil then
    Encoding := TEncoding.Default;
  FEncoding := Encoding;
  if BufferSize < 1 then
    BufferSize := 1;
  FBufferSize := BufferSize;
  FAutoFlush := AutoFlush;
  // The preamble only at the start: not in the middle of a stream written before
  if not Stream.CanSeek or (Stream.Position = 0) then
  begin
    var Preamble := Encoding.GetPreamble;
    if Length(Preamble) > 0 then
      Stream.WriteBuffer(Preamble, Length(Preamble));
  end;
end;

// A file stream to write: a new one, or the end of the file with Append.
function OpenToWrite(const FileName: string; Append: Boolean): TStream;
begin
  if Append and FileExists(FileName) then
  begin
    Result := TFileStream.Create(FileName, fmOpenWrite);
    Result.Seek(0, TSeekOrigin.soEnd);
  end
  else
    Result := TFileStream.Create(FileName, fmCreate);
end;

constructor TStreamWriter.Create(const Stream: TStream);
begin
  inherited Create;
  Init(Stream, nil, 4096, True);
end;

constructor TStreamWriter.Create(const Stream: TStream; Encoding: TEncoding;
  BufferSize: Integer);
begin
  inherited Create;
  Init(Stream, Encoding, BufferSize, True);
end;

constructor TStreamWriter.Create(const FileName: string; Append: Boolean);
begin
  inherited Create;
  Init(OpenToWrite(FileName, Append), nil, 4096, False);
end;

constructor TStreamWriter.Create(const FileName: string; Append: Boolean; Encoding: TEncoding;
  BufferSize: Integer);
begin
  inherited Create;
  Init(OpenToWrite(FileName, Append), Encoding, BufferSize, False);
end;

destructor TStreamWriter.Destroy;
begin
  if FStream <> nil then
    WriteBuffer(True);
  inherited Destroy;
end;

procedure TStreamWriter.CheckOpen;
begin
  if FStream = nil then
    raise EStreamError.Create('the writer is closed');
end;

// Writes the buffer to the stream; unless All, a high surrogate at its end waits for its pair
// (so that a pair written in two Writes is encoded as one character).
procedure TStreamWriter.WriteBuffer(All: Boolean);
begin
  var N := Length(FBuffer);
  if N = 0 then
    Exit;
  if not All and (FBuffer[N] >= #$D800) and (FBuffer[N] <= #$DBFF) then
    Dec(N);
  if N = 0 then
    Exit;
  var Text := FBuffer;
  FBuffer := Copy(Text, N + 1, MaxInt);
  if N < Length(Text) then
    SetLength(Text, N);
  FStream.WriteText(Text, FEncoding);
end;

procedure TStreamWriter.Close;
begin
  if FStream = nil then
    Exit;
  try
    WriteBuffer(True);
  finally
    var S := FStream;
    FStream := nil;
    FBuffer := '';
    S.Close;
  end;
end;

procedure TStreamWriter.Flush;
begin
  CheckOpen;
  WriteBuffer(True);
end;

procedure TStreamWriter.Write(const Value: string);
begin
  CheckOpen;
  FBuffer := FBuffer + Value;
  if FAutoFlush or (Length(FBuffer) >= FBufferSize) then
    WriteBuffer(False);
end;

{ TBinaryReader }

constructor TBinaryReader.Create(const Stream: TStream; AEncoding: TEncoding);
begin
  inherited Create;
  Init(Stream, AEncoding);
end;

constructor TBinaryReader.Create(const FileName: string; AEncoding: TEncoding);
begin
  inherited Create;
  Init(TFileStream.Create(FileName, fmOpenRead), AEncoding);
end;

procedure TBinaryReader.Init(const Stream: TStream; AEncoding: TEncoding);
begin
  if Stream = nil then
    raise EArgumentException.Create('the stream of a TBinaryReader is nil');
  FStream := Stream;
  if AEncoding = nil then
    AEncoding := TEncoding.Default;
  FEncoding := AEncoding;
end;

procedure TBinaryReader.CheckOpen;
begin
  if FStream = nil then
    raise EStreamError.Create('the reader is closed');
end;

procedure TBinaryReader.Close;
begin
  if FStream = nil then
    Exit;
  var S := FStream;
  FStream := nil;
  FPending := '';
  S.Close;
end;

// Reads the bytes of the next character and decodes them; returns its code unit, or -1 at the
// end. A character of two code units (or an invalid sequence decoded as several) leaves the
// others in FPending. Peeking reads without keeping them: the caller moves the position back.
function TBinaryReader.ReadCharCode(Peeking: Boolean): Integer;
var
  B: TBytes;
begin
  if FPending <> '' then
  begin
    Result := Ord(FPending[1]);
    if not Peeking then
      Delete(FPending, 1, 1);
    Exit;
  end;
  SetLength(B, 4);
  var N := FStream.Read(B, 0, 1);
  if N = 0 then
    Exit(-1);
  case FEncoding.CodePage of
    65001:
      begin
        var Need := 1;
        if B[0] and $E0 = $C0 then
          Need := 2
        else if B[0] and $F0 = $E0 then
          Need := 3
        else if B[0] and $F8 = $F0 then
          Need := 4;
        while N < Need do
        begin
          var Got := FStream.Read(B, N, Need - N);
          if Got <= 0 then
            Break;
          Inc(N, Got);
        end;
      end;
    1200, 1201:
      N := N + FStream.Read(B, 1, 1);
  end;
  var S := FEncoding.GetString(B, 0, N);
  Result := Ord(S[1]);
  if not Peeking then
    FPending := Copy(S, 2, MaxInt);
end;

function TBinaryReader.PeekChar: Integer;
begin
  CheckOpen;
  if FPending <> '' then
    Exit(Ord(FPending[1]));
  if not FStream.CanSeek then
    Exit(-1);
  var Pos := FStream.Position;
  try
    Result := ReadCharCode(True);
  finally
    // Peeking never moves the stream, also when decoding raises
    FStream.Position := Pos;
  end;
end;

function TBinaryReader.Read: Integer;
begin
  CheckOpen;
  Result := ReadCharCode(False);
end;

function TBinaryReader.Read(var Buffer: TArray<Char>; Index, Count: Integer): Integer;
begin
  CheckCharRange(Buffer, Index, Count);
  CheckOpen;
  Result := 0;
  while Result < Count do
  begin
    var C := ReadCharCode(False);
    if C < 0 then
      Break;
    Buffer[Index + Result] := Char(C);
    Inc(Result);
  end;
end;

function TBinaryReader.Read(var Buffer: TBytes; Index, Count: Integer): Integer;
begin
  CheckOpen;
  Result := FStream.Read(Buffer, Index, Count);
end;

function TBinaryReader.ReadBoolean: Boolean;
begin
  CheckOpen;
  Result := False;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadByte: Byte;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadSByte: ShortInt;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadShortInt: ShortInt;
begin
  Result := ReadSByte;
end;

function TBinaryReader.ReadInt16: SmallInt;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadSmallInt: SmallInt;
begin
  Result := ReadInt16;
end;

function TBinaryReader.ReadUInt16: Word;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadWord: Word;
begin
  Result := ReadUInt16;
end;

function TBinaryReader.ReadInt32: Integer;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadInteger: Integer;
begin
  Result := ReadInt32;
end;

function TBinaryReader.ReadUInt32: Cardinal;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadCardinal: Cardinal;
begin
  Result := ReadUInt32;
end;

function TBinaryReader.ReadInt64: Int64;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadUInt64: UInt64;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadSingle: Single;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadDouble: Double;
begin
  CheckOpen;
  Result := 0;
  FStream.ReadBufferData(Result);
end;

function TBinaryReader.ReadChar: Char;
begin
  CheckOpen;
  var C := ReadCharCode(False);
  if C < 0 then
    raise EReadError.Create('the stream ended before a character was read');
  Result := Char(C);
end;

// A 7-bit encoded integer: 7 bits a byte, the low ones first; the high bit of a byte is set
// when another follows.
function TBinaryReader.Read7BitEncodedInt: Integer;
begin
  var Value: UInt64 := 0;
  var Shift := 0;
  while True do
  begin
    var B: Byte := 0;
    FStream.ReadBufferData(B);
    Value := Value or (UInt64(B and $7F) shl Shift);
    if B and $80 = 0 then
      Break;
    Inc(Shift, 7);
    if Shift > 28 then
      raise EStreamError.Create('the length of the string is not a 7-bit encoded integer');
  end;
  if Value > MaxInt then
    raise EStreamError.Create('the length of the string is not a 7-bit encoded integer');
  Result := Integer(Value);
end;

function TBinaryReader.ReadString: string;
var
  B: TBytes;
begin
  CheckOpen;
  var N := Read7BitEncodedInt;
  if N = 0 then
    Exit('');
  SetLength(B, N);
  FStream.ReadBuffer(B, N);
  Result := FEncoding.GetString(B);
end;

function TBinaryReader.ReadBytes(Count: Integer): TBytes;
begin
  CheckOpen;
  if Count < 0 then
    raise ERangeError.Create('cannot read ' + IntToStr(Count) + ' bytes');
  SetLength(Result, Count);
  var Done := 0;
  while Done < Count do
  begin
    var N := FStream.Read(Result, Done, Count - Done);
    if N <= 0 then
      Break;
    Inc(Done, N);
  end;
  if Done < Count then
    SetLength(Result, Done);
end;

function TBinaryReader.ReadChars(Count: Integer): TArray<Char>;
begin
  if Count < 0 then
    raise ERangeError.Create('cannot read ' + IntToStr(Count) + ' characters');
  SetLength(Result, Count);
  var N := Read(Result, 0, Count);
  if N < Count then
    SetLength(Result, N);
end;

{ TBinaryWriter }

constructor TBinaryWriter.Create(const Stream: TStream; AEncoding: TEncoding);
begin
  inherited Create;
  Init(Stream, AEncoding);
end;

constructor TBinaryWriter.Create(const FileName: string; Append: Boolean;
  AEncoding: TEncoding);
begin
  inherited Create;
  Init(OpenToWrite(FileName, Append), AEncoding);
end;

procedure TBinaryWriter.Init(const Stream: TStream; AEncoding: TEncoding);
begin
  if Stream = nil then
    raise EArgumentException.Create('the stream of a TBinaryWriter is nil');
  FStream := Stream;
  if AEncoding = nil then
    AEncoding := TEncoding.Default;
  FEncoding := AEncoding;
end;

procedure TBinaryWriter.CheckOpen;
begin
  if FStream = nil then
    raise EStreamError.Create('the writer is closed');
end;

procedure TBinaryWriter.Close;
begin
  if FStream = nil then
    Exit;
  var S := FStream;
  FStream := nil;
  S.Close;
end;

procedure TBinaryWriter.Flush;
begin
  CheckOpen;
end;

function TBinaryWriter.Seek(Offset: Integer; Origin: TSeekOrigin): Int64;
begin
  CheckOpen;
  Result := FStream.Seek(Offset, Origin);
end;

procedure TBinaryWriter.WriteBytes(const Bytes: TBytes);
begin
  if Length(Bytes) > 0 then
    FStream.WriteBuffer(Bytes, Length(Bytes));
end;

procedure TBinaryWriter.Write7BitEncodedInt(Value: Integer);
begin
  var V := Cardinal(Value);
  while V >= $80 do
  begin
    FStream.WriteBufferData(Byte((V and $7F) or $80));
    V := V shr 7;
  end;
  FStream.WriteBufferData(Byte(V));
end;

procedure TBinaryWriter.Write(Value: Boolean);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: Byte);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: ShortInt);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: SmallInt);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: Word);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: Integer);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: Cardinal);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: Int64);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: UInt64);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: Single);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: Double);
begin
  CheckOpen;
  FStream.WriteBufferData(Value);
end;

procedure TBinaryWriter.Write(Value: Char);
begin
  CheckOpen;
  WriteBytes(FEncoding.GetBytes(string(Value)));
end;

procedure TBinaryWriter.Write(const Value: string);
begin
  CheckOpen;
  var B := FEncoding.GetBytes(Value);
  Write7BitEncodedInt(Length(B));
  WriteBytes(B);
end;

procedure TBinaryWriter.Write(const Value: TBytes);
begin
  CheckOpen;
  WriteBytes(Value);
end;

procedure TBinaryWriter.Write(const Value: TBytes; Index, Count: Integer);
begin
  CheckOpen;
  FStream.WriteBuffer(Value, Index, Count);
end;

procedure TBinaryWriter.Write(const Value: TArray<Char>);
begin
  CheckOpen;
  WriteBytes(FEncoding.GetBytes(CharsToString(Value, 0, Length(Value))));
end;

procedure TBinaryWriter.Write(const Value: TArray<Char>; Index, Count: Integer);
begin
  CheckOpen;
  WriteBytes(FEncoding.GetBytes(CharsToString(Value, Index, Count)));
end;
