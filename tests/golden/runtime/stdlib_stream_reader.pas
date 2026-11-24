program StdlibStreamReader;

// TStreamReader on streams in memory (spec/stdlib/System.Classes/Readers.md): the three line
// breaks, a CR LF and a UTF-8 character cut by the end of a buffer, the byte order marks with
// and without DetectBOM, Peek / Read / ReadBlock / ReadToEnd / EndOfStream, the end of the
// text, DiscardBufferedData, Close and the errors.

uses
  System.SysUtils, System.Classes;

type
  // Gives at most 3 bytes a read, so that characters, breaks and byte order marks are cut
  TTrickleStream = class(TStream)
  private
    FInner: TStream;
  public
    constructor Create(const B: TBytes);
    function Read(var Buffer: TBytes; Offset, Count: Integer): Integer; override;
    function Write(const Buffer: TBytes; Offset, Count: Integer): Integer; override;
    function Seek(const Offset: Int64; Origin: TSeekOrigin): Int64; override;
  end;

constructor TTrickleStream.Create(const B: TBytes);
begin
  inherited Create;
  FInner := TBytesStream.Create(B);
end;

function TTrickleStream.Read(var Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  if Count > 3 then
    Count := 3;
  Result := FInner.Read(Buffer, Offset, Count);
end;

function TTrickleStream.Write(const Buffer: TBytes; Offset, Count: Integer): Integer;
begin
  Result := FInner.Write(Buffer, Offset, Count);
end;

function TTrickleStream.Seek(const Offset: Int64; Origin: TSeekOrigin): Int64;
begin
  Result := FInner.Seek(Offset, Origin);
end;

function Stream(const B: TBytes): TStream;
begin
  Result := TBytesStream.Create(B);
end;

function Utf8(const S: string): TStream;
begin
  Result := Stream(TEncoding.Default.GetBytes(S));
end;

procedure Lines(const R: TStreamReader);
begin
  while not R.EndOfStream do
    Write('[', R.ReadLine, ']');
  WriteLn(' ', R.CurrentEncoding.EncodingName);
end;

begin
  // LF, CR LF and CR; an empty line; the last line without a break
  Lines(TStreamReader.Create(Utf8('a'#10'b'#13#10'c'#13'd'#10#10'e')));
  // A break at the end makes no empty last line; ReadLine at the end is ''
  var R := TStreamReader.Create(Utf8('x'#13#10));
  WriteLn('[', R.ReadLine, '] ', R.EndOfStream, ' [', R.ReadLine, '] ', R.Peek, ' ', R.Read);
  Lines(TStreamReader.Create(Utf8('')));

  // A CR LF cut by the end of the buffer (128 bytes) is one break; so is a CR LF at the start
  // of the next read
  var S := StringOfChar('a', 127) + #13#10'b'#13 + StringOfChar('c', 125) + #13#10'd';
  R := TStreamReader.Create(Utf8(S), TEncoding.Default, False, 16);
  while not R.EndOfStream do
    Write(Length(R.ReadLine), ' ');
  WriteLn;
  // A character of 3 and one of 4 bytes cut by the end of the buffer
  S := StringOfChar('a', 127) + '日' + StringOfChar('b', 125) + '😀z';
  R := TStreamReader.Create(Utf8(S), nil, False, 128);
  var T := R.ReadToEnd;
  WriteLn(Length(T), ' ', T = S, ' ', R.EndOfStream);

  // A stream that gives 3 bytes at a time: UTF-8 and UTF-16 characters, a surrogate pair and a
  // CR LF cut between reads
  S := 'aé日😀'#13#10'b'#13'c';
  Lines(TStreamReader.Create(TTrickleStream.Create(TEncoding.UTF8.GetBytes(S))));
  var B := TEncoding.Unicode.GetPreamble + TEncoding.Unicode.GetBytes(S);
  Lines(TStreamReader.Create(TTrickleStream.Create(B)));
  T := TStreamReader.Create(TTrickleStream.Create(B)).ReadToEnd;
  WriteLn(Length(T), ' ', T = S);

  // The byte order marks: UTF-8, UTF-16LE and UTF-16BE are detected and skipped
  B := TEncoding.UTF8.GetPreamble + TEncoding.UTF8.GetBytes('é1'#10'2');
  Lines(TStreamReader.Create(Stream(B)));
  B := TEncoding.Unicode.GetPreamble + TEncoding.Unicode.GetBytes('é1'#10'2');
  Lines(TStreamReader.Create(Stream(B)));
  B := TEncoding.BigEndianUnicode.GetPreamble + TEncoding.BigEndianUnicode.GetBytes('é1'#10'2');
  Lines(TStreamReader.Create(Stream(B), True));
  // Without a BOM: the encoding given (UTF-16LE here), else UTF-8
  Lines(TStreamReader.Create(Stream(TEncoding.Unicode.GetBytes('ab')), TEncoding.Unicode, True));
  Lines(TStreamReader.Create(Utf8('é')));
  // Without DetectBOM: the encoding's own preamble is skipped, another BOM is text
  B := TEncoding.UTF8.GetPreamble + TEncoding.UTF8.GetBytes('x');
  R := TStreamReader.Create(Stream(B), TEncoding.UTF8);
  WriteLn(Length(R.ReadToEnd));
  R := TStreamReader.Create(Stream(B), False);
  T := R.ReadToEnd;
  WriteLn(Length(T), ' ', Ord(T[1]) = $FEFF);
  // A BOM cut short: the bytes are text
  B := [$EF, $BB];
  R := TStreamReader.Create(Stream(B));
  T := R.ReadToEnd;
  WriteLn(Length(T), ' ', Ord(T[1]), ' ', R.CurrentEncoding.EncodingName);

  // Peek, Read, the buffer forms and ReadToEnd after a ReadLine
  R := TStreamReader.Create(Utf8('héllo wörld'#10'rest'#10'more'));
  WriteLn(R.Peek, ' ', R.Read, ' ', R.Peek);
  var C: TArray<Char>;
  SetLength(C, 6);
  WriteLn(R.Read(C, 1, 3), ' ', C[1], C[2], C[3]);
  WriteLn(R.ReadBlock(C, 0, 6), ' ', C[0], C[1], C[2], C[3], C[4], C[5]);
  WriteLn('[', R.ReadLine, '] [', R.ReadToEnd, '] ', R.EndOfStream, ' ', R.Read(C, 0, 6));
  try
    R.Read(C, 4, 3);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // DiscardBufferedData after moving the stream's position
  var M := Utf8('one'#10'two'#10'three');
  R := TStreamReader.Create(M);
  WriteLn(R.ReadLine, ' ', M.Position);
  M.Position := 4;
  R.DiscardBufferedData;
  WriteLn(R.ReadLine, ' ', R.ReadLine);
  WriteLn(R.BaseStream = M);

  // Close closes the stream; the reader cannot be used afterwards; Close again does nothing
  R.Close;
  R.Close;
  WriteLn(R.BaseStream = nil);
  try
    M.Position := 0;
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    R.ReadLine;
  except
    on E: EStreamError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    var None: TStream := nil;
    TStreamReader.Create(None);
  except
    on E: EArgumentException do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // Invalid UTF-8 becomes U+FFFD with Default, EEncodingError with UTF8Strict
  B := [$61, $FF, $62];
  T := TStreamReader.Create(Stream(B)).ReadToEnd;
  WriteLn(Length(T), ' ', Ord(T[2]));
  try
    TStreamReader.Create(Stream(B), TEncoding.UTF8Strict).ReadLine;
  except
    on E: EEncodingError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
