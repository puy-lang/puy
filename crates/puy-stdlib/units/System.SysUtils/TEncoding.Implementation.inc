// TEncoding (docs/spec/stdlib/System.SysUtils/TEncoding.md). The conversions themselves, and the
// check of a byte range, are the Rust functions of System.SysUtils.Native.

type
  // US-ASCII (code page 20127): TEncoding.ASCII.
  TASCIIEncoding = class(TEncoding)
  private
    function Encode(const S: string): TBytes; override;
    function Decode(const Bytes: TBytes; Index, Count: Integer): string; override;
    function GetCodePage: Cardinal; override;
    function GetEncodingName: string; override;
    function GetIsSingleByte: Boolean; override;
  public
    function Clone: TEncoding; override;
    function GetPreamble: TBytes; override;
  end;

// Whether Buffer starts with the non-empty Preamble.
function StartsWithPreamble(const Buffer, Preamble: TBytes): Boolean;
begin
  if (Length(Preamble) = 0) or (Length(Buffer) < Length(Preamble)) then
    Exit(False);
  for var I := 0 to High(Preamble) do
    if Buffer[I] <> Preamble[I] then
      Exit(False);
  Result := True;
end;

{ TEncoding }

class function TEncoding.GetUTF8: TEncoding;
begin
  if FUTF8 = nil then
    FUTF8 := TUTF8Encoding.Create;
  Result := FUTF8;
end;

class function TEncoding.GetUTF8Strict: TEncoding;
begin
  if FUTF8Strict = nil then
  begin
    var E := TUTF8Encoding.Create;
    E.FStrict := True;
    FUTF8Strict := E;
  end;
  Result := FUTF8Strict;
end;

class function TEncoding.GetDefault: TEncoding;
begin
  if FDefault = nil then
  begin
    var E := TUTF8Encoding.Create;
    E.FNoPreamble := True;
    FDefault := E;
  end;
  Result := FDefault;
end;

class function TEncoding.GetUnicode: TEncoding;
begin
  if FUnicode = nil then
    FUnicode := TUnicodeEncoding.Create;
  Result := FUnicode;
end;

class function TEncoding.GetBigEndianUnicode: TEncoding;
begin
  if FBigEndianUnicode = nil then
    FBigEndianUnicode := TBigEndianUnicodeEncoding.Create;
  Result := FBigEndianUnicode;
end;

class function TEncoding.GetASCII: TEncoding;
begin
  if FASCII = nil then
    FASCII := TASCIIEncoding.Create;
  Result := FASCII;
end;

class procedure TEncoding.FreeStandard;
begin
  FreeAndNil(FUTF8);
  FreeAndNil(FUTF8Strict);
  FreeAndNil(FDefault);
  FreeAndNil(FUnicode);
  FreeAndNil(FBigEndianUnicode);
  FreeAndNil(FASCII);
end;

function TEncoding.GetIsSingleByte: Boolean;
begin
  Result := False;
end;

function TEncoding.GetBytes(const S: string): TBytes;
begin
  Result := Encode(S);
end;

function TEncoding.GetByteCount(const S: string): Integer;
begin
  Result := Length(Encode(S));
end;

function TEncoding.GetString(const Bytes: TBytes): string;
begin
  Result := Decode(Bytes, 0, Length(Bytes));
end;

function TEncoding.GetString(const Bytes: TBytes; ByteIndex, ByteCount: Integer): string;
begin
  // The conversion checks the range (System.SysUtils.Native)
  Result := Decode(Bytes, ByteIndex, ByteCount);
end;

function TEncoding.GetCharCount(const Bytes: TBytes): Integer;
begin
  Result := Length(Decode(Bytes, 0, Length(Bytes)));
end;

class function TEncoding.Convert(Source, Destination: TEncoding; const Bytes: TBytes): TBytes;
begin
  Result := Destination.GetBytes(Source.GetString(Bytes));
end;

class function TEncoding.Convert(Source, Destination: TEncoding; const Bytes: TBytes;
  StartIndex, Count: Integer): TBytes;
begin
  Result := Destination.GetBytes(Source.GetString(Bytes, StartIndex, Count));
end;

class function TEncoding.GetBufferEncoding(const Buffer: TBytes;
  var AEncoding: TEncoding): Integer;
begin
  Result := GetBufferEncoding(Buffer, AEncoding, Default);
end;

class function TEncoding.GetBufferEncoding(const Buffer: TBytes; var AEncoding: TEncoding;
  ADefaultEncoding: TEncoding): Integer;
begin
  if AEncoding <> nil then
  begin
    var Preamble := AEncoding.GetPreamble;
    if StartsWithPreamble(Buffer, Preamble) then
      Result := Length(Preamble)
    else
      Result := 0;
    Exit;
  end;
  // The byte order marks, UTF-8's first
  var Candidates: TArray<TEncoding> := [UTF8, Unicode, BigEndianUnicode];
  for var E in Candidates do
  begin
    var Preamble := E.GetPreamble;
    if StartsWithPreamble(Buffer, Preamble) then
    begin
      AEncoding := E;
      Exit(Length(Preamble));
    end;
  end;
  AEncoding := ADefaultEncoding;
  Result := 0;
end;

class function TEncoding.GetEncoding(CodePage: Integer): TEncoding;
begin
  case CodePage of
    65001: Result := TUTF8Encoding.Create;
    1200: Result := TUnicodeEncoding.Create;
    1201: Result := TBigEndianUnicodeEncoding.Create;
    20127: Result := TASCIIEncoding.Create;
  else
    raise ENotSupportedException.Create('code page ' + IntToStr(CodePage) +
      ' is not supported');
  end;
end;

class function TEncoding.IsStandardEncoding(AEncoding: TEncoding): Boolean;
begin
  Result := (AEncoding <> nil) and ((AEncoding = FUTF8) or (AEncoding = FUTF8Strict) or
    (AEncoding = FDefault) or (AEncoding = FUnicode) or (AEncoding = FBigEndianUnicode) or
    (AEncoding = FASCII));
end;

{ TUTF8Encoding }

function TUTF8Encoding.Encode(const S: string): TBytes;
begin
  if FStrict then
    Result := System.SysUtils.Native.EncodeUtf8Strict(S)
  else
    Result := System.SysUtils.Native.EncodeUtf8(S);
end;

function TUTF8Encoding.Decode(const Bytes: TBytes; Index, Count: Integer): string;
begin
  if FStrict then
    Result := System.SysUtils.Native.DecodeUtf8Strict(Bytes, Index, Count)
  else
    Result := System.SysUtils.Native.DecodeUtf8(Bytes, Index, Count);
end;

function TUTF8Encoding.GetCodePage: Cardinal;
begin
  Result := 65001;
end;

function TUTF8Encoding.GetEncodingName: string;
begin
  Result := 'UTF-8';
end;

function TUTF8Encoding.Clone: TEncoding;
begin
  var E := TUTF8Encoding.Create;
  E.FStrict := FStrict;
  E.FNoPreamble := FNoPreamble;
  Result := E;
end;

function TUTF8Encoding.GetPreamble: TBytes;
begin
  if FNoPreamble then
    Result := nil
  else
    Result := [$EF, $BB, $BF];
end;

{ TUnicodeEncoding }

function TUnicodeEncoding.Encode(const S: string): TBytes;
begin
  Result := System.SysUtils.Native.EncodeUtf16(S, False);
end;

function TUnicodeEncoding.Decode(const Bytes: TBytes; Index, Count: Integer): string;
begin
  Result := System.SysUtils.Native.DecodeUtf16(Bytes, Index, Count, False);
end;

function TUnicodeEncoding.GetCodePage: Cardinal;
begin
  Result := 1200;
end;

function TUnicodeEncoding.GetEncodingName: string;
begin
  Result := 'UTF-16LE';
end;

function TUnicodeEncoding.Clone: TEncoding;
begin
  Result := TUnicodeEncoding.Create;
end;

function TUnicodeEncoding.GetPreamble: TBytes;
begin
  Result := [$FF, $FE];
end;

{ TBigEndianUnicodeEncoding }

function TBigEndianUnicodeEncoding.Encode(const S: string): TBytes;
begin
  Result := System.SysUtils.Native.EncodeUtf16(S, True);
end;

function TBigEndianUnicodeEncoding.Decode(const Bytes: TBytes; Index, Count: Integer): string;
begin
  Result := System.SysUtils.Native.DecodeUtf16(Bytes, Index, Count, True);
end;

function TBigEndianUnicodeEncoding.GetCodePage: Cardinal;
begin
  Result := 1201;
end;

function TBigEndianUnicodeEncoding.GetEncodingName: string;
begin
  Result := 'UTF-16BE';
end;

function TBigEndianUnicodeEncoding.Clone: TEncoding;
begin
  Result := TBigEndianUnicodeEncoding.Create;
end;

function TBigEndianUnicodeEncoding.GetPreamble: TBytes;
begin
  Result := [$FE, $FF];
end;

{ TASCIIEncoding }

function TASCIIEncoding.Encode(const S: string): TBytes;
begin
  Result := System.SysUtils.Native.EncodeAscii(S);
end;

function TASCIIEncoding.Decode(const Bytes: TBytes; Index, Count: Integer): string;
begin
  Result := System.SysUtils.Native.DecodeAscii(Bytes, Index, Count);
end;

function TASCIIEncoding.GetCodePage: Cardinal;
begin
  Result := 20127;
end;

function TASCIIEncoding.GetEncodingName: string;
begin
  Result := 'US-ASCII';
end;

function TASCIIEncoding.GetIsSingleByte: Boolean;
begin
  Result := True;
end;

function TASCIIEncoding.Clone: TEncoding;
begin
  Result := TASCIIEncoding.Create;
end;

function TASCIIEncoding.GetPreamble: TBytes;
begin
  Result := nil;
end;
