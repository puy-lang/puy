type
  // The base of the readers of text (docs/spec/stdlib/System.Classes/Readers.md): a descendant
  // overrides Close, Peek, the two Reads, ReadLine and ReadToEnd. Reference counted, as the
  // streams.
  TTextReader = class(TRefObject)
  public
    procedure Close; virtual; abstract;
    // The next character as its code unit without reading it; -1 at the end.
    function Peek: Integer; virtual; abstract;
    // Reads the next character as its code unit; -1 at the end.
    function Read: Integer; overload; virtual; abstract;
    // Reads up to Count characters into Buffer from Index; returns how many were read (0 at the
    // end, fewer than Count when not all are at hand yet). ERangeError if Index..Index + Count - 1
    // are not in Buffer.
    function Read(var Buffer: TArray<Char>; Index, Count: Integer): Integer; overload; virtual;
      abstract;
    // Read with a buffer that reads until Count characters or the end.
    function ReadBlock(var Buffer: TArray<Char>; Index, Count: Integer): Integer;
    // The characters up to the next line break (CR LF, LF or CR), which is read and left out;
    // '' at the end.
    function ReadLine: string; virtual; abstract;
    // All the characters left.
    function ReadToEnd: string; virtual; abstract;
  end;

  // The base of the writers of text: a descendant overrides Close, Flush and Write of a string;
  // the other Writes and the WriteLines write the text of their value with it.
  TTextWriter = class(TRefObject)
  private
    FNewLine: string;
  public
    // NewLine is sLineBreak.
    constructor Create;
    procedure Close; virtual; abstract;
    procedure Flush; virtual; abstract;
    procedure Write(const Value: string); overload; virtual; abstract;
    // 'True' or 'False'.
    procedure Write(Value: Boolean); overload;
    procedure Write(Value: Char); overload;
    procedure Write(const Value: TArray<Char>); overload;
    // The characters Index..Index + Count - 1 of Value (ERangeError if they are not in it).
    procedure Write(const Value: TArray<Char>; Index, Count: Integer); overload;
    // As IntToStr / FloatToStr.
    procedure Write(Value: Integer); overload;
    procedure Write(Value: Cardinal); overload;
    procedure Write(Value: Int64); overload;
    procedure Write(Value: UInt64); overload;
    procedure Write(Value: Single); overload;
    procedure Write(Value: Double); overload;
    // Value.ToString.
    procedure Write(const Value: TObject); overload;
    // NewLine.
    procedure WriteLine; overload;
    // Write, then NewLine.
    procedure WriteLine(const Value: string); overload;
    procedure WriteLine(Value: Boolean); overload;
    procedure WriteLine(Value: Char); overload;
    procedure WriteLine(const Value: TArray<Char>); overload;
    procedure WriteLine(const Value: TArray<Char>; Index, Count: Integer); overload;
    procedure WriteLine(Value: Integer); overload;
    procedure WriteLine(Value: Cardinal); overload;
    procedure WriteLine(Value: Int64); overload;
    procedure WriteLine(Value: UInt64); overload;
    procedure WriteLine(Value: Single); overload;
    procedure WriteLine(Value: Double); overload;
    procedure WriteLine(const Value: TObject); overload;
    // What WriteLine writes after the value.
    property NewLine: string read FNewLine write FNewLine;
  end;

  // Reads the text of a stream in an encoding, a buffer at a time. The reader holds a reference
  // to the stream (there is no OwnStream: the stream lives while one is held); Close closes the
  // stream too.
  TStreamReader = class(TTextReader)
  private
    FStream: TStream;
    FEncoding: TEncoding;
    FDetectBOM: Boolean;
    // The bytes read: FByteCount of them at the start are not decoded yet (a character cut by
    // the end of a read)
    FBytes: TBytes;
    FByteCount: Integer;
    // The text decoded and not read yet: from FTextPos to its end
    FText: string;
    FTextPos: Integer;
    // Whether the byte order mark was looked for, and whether the stream has no more bytes
    FStarted: Boolean;
    FEnded: Boolean;
    procedure Init(const Stream: TStream; Encoding: TEncoding; DetectBOM: Boolean;
      BufferSize: Integer);
    procedure CheckOpen;
    function ReadMore: Boolean;
    function Fill: Boolean;
    function GetEndOfStream: Boolean;
  public
    // Encoding nil is TEncoding.Default (UTF-8). With DetectBOM, a byte order mark of UTF-8,
    // UTF-16LE or UTF-16BE at the start chooses the encoding instead (and is skipped); without
    // it, a preamble of Encoding at the start is skipped. The first two forms detect the BOM.
    constructor Create(const Stream: TStream); overload;
    constructor Create(const Stream: TStream; DetectBOM: Boolean); overload;
    constructor Create(const Stream: TStream; Encoding: TEncoding; DetectBOM: Boolean = False;
      BufferSize: Integer = 4096); overload;
    // On a TFileStream opened to read.
    constructor Create(const FileName: string); overload;
    constructor Create(const FileName: string; DetectBOM: Boolean); overload;
    constructor Create(const FileName: string; Encoding: TEncoding; DetectBOM: Boolean = False;
      BufferSize: Integer = 4096); overload;
    // Closes the stream; reading afterwards raises EStreamError.
    procedure Close; override;
    // Forgets the text and bytes read ahead (after the stream's position was moved).
    procedure DiscardBufferedData;
    function Peek: Integer; override;
    function Read: Integer; overload; override;
    function Read(var Buffer: TArray<Char>; Index, Count: Integer): Integer; overload; override;
    function ReadLine: string; override;
    function ReadToEnd: string; override;
    property BaseStream: TStream read FStream;
    // The encoding the text is read in (the one the BOM chose, if any).
    property CurrentEncoding: TEncoding read FEncoding;
    // Whether all the text was read.
    property EndOfStream: Boolean read GetEndOfStream;
  end;

  // Writes text to a stream in an encoding. The preamble of the encoding (the BOM of
  // TEncoding.UTF8, for one) is written first when the stream is at its start. With AutoFlush,
  // each Write reaches the stream at once; otherwise the text waits in a buffer until Flush,
  // Close, the last reference going, or BufferSize characters.
  TStreamWriter = class(TTextWriter)
  private
    FStream: TStream;
    FEncoding: TEncoding;
    FAutoFlush: Boolean;
    FBufferSize: Integer;
    // The text written and not yet in the stream
    FBuffer: string;
    procedure Init(const Stream: TStream; Encoding: TEncoding; BufferSize: Integer;
      AutoFlush: Boolean);
    procedure CheckOpen;
    procedure WriteBuffer(All: Boolean);
  public
    // Encoding nil is TEncoding.Default (UTF-8 without a BOM). AutoFlush is True.
    constructor Create(const Stream: TStream); overload;
    constructor Create(const Stream: TStream; Encoding: TEncoding; BufferSize: Integer = 4096);
      overload;
    // On a TFileStream: a new file (or one cut to 0 bytes), or with Append, the end of the file
    // (created if missing). AutoFlush is False: no one else holds the stream.
    constructor Create(const FileName: string; Append: Boolean = False); overload;
    constructor Create(const FileName: string; Append: Boolean; Encoding: TEncoding;
      BufferSize: Integer = 4096); overload;
    // Flushes.
    destructor Destroy; override;
    // Flushes and closes the stream; writing afterwards raises EStreamError.
    procedure Close; override;
    // Writes the text waiting in the buffer to the stream.
    procedure Flush; override;
    procedure Write(const Value: string); overload; override;
    property AutoFlush: Boolean read FAutoFlush write FAutoFlush;
    property BaseStream: TStream read FStream;
    property Encoding: TEncoding read FEncoding;
  end;

  // Reads values from a stream: numbers in their bytes, little-endian on every target; strings
  // with their length before them; characters in the encoding.
  TBinaryReader = class(TRefObject)
  private
    FStream: TStream;
    FEncoding: TEncoding;
    // Characters decoded and not read yet (the low surrogate of a pair, the rest of an invalid
    // sequence)
    FPending: string;
    procedure Init(const Stream: TStream; AEncoding: TEncoding);
    procedure CheckOpen;
    function ReadCharCode(Peeking: Boolean): Integer;
    function Read7BitEncodedInt: Integer;
  public
    // Encoding nil is TEncoding.Default (UTF-8).
    constructor Create(const Stream: TStream; AEncoding: TEncoding = nil); overload;
    // On a TFileStream opened to read.
    constructor Create(const FileName: string; AEncoding: TEncoding = nil); overload;
    // Closes the stream; reading afterwards raises EStreamError.
    procedure Close; virtual;
    // The next character without reading it; -1 at the end or when the stream cannot seek.
    function PeekChar: Integer; virtual;
    // Reads the next character as its code unit; -1 at the end.
    function Read: Integer; overload; virtual;
    // Reads up to Count characters / bytes into Buffer from Index; returns how many were read.
    function Read(var Buffer: TArray<Char>; Index, Count: Integer): Integer; overload; virtual;
    function Read(var Buffer: TBytes; Index, Count: Integer): Integer; overload; virtual;
    // The values: EReadError when the stream ends first.
    function ReadBoolean: Boolean; virtual;
    function ReadByte: Byte; virtual;
    function ReadSByte: ShortInt; virtual;
    function ReadShortInt: ShortInt; virtual;
    function ReadInt16: SmallInt; virtual;
    function ReadSmallInt: SmallInt; virtual;
    function ReadUInt16: Word; virtual;
    function ReadWord: Word; virtual;
    function ReadInt32: Integer; virtual;
    function ReadInteger: Integer; virtual;
    function ReadUInt32: Cardinal; virtual;
    function ReadCardinal: Cardinal; virtual;
    function ReadInt64: Int64; virtual;
    function ReadUInt64: UInt64; virtual;
    function ReadSingle: Single; virtual;
    function ReadDouble: Double; virtual;
    function ReadChar: Char; virtual;
    // A string written by TBinaryWriter.Write(string): its length in bytes as a 7-bit encoded
    // integer, then its bytes in the encoding.
    function ReadString: string; virtual;
    // Up to Count bytes / characters (fewer at the end).
    function ReadBytes(Count: Integer): TBytes; virtual;
    function ReadChars(Count: Integer): TArray<Char>; virtual;
    property BaseStream: TStream read FStream;
  end;

  // Writes values to a stream in the form TBinaryReader reads.
  TBinaryWriter = class(TRefObject)
  private
    FStream: TStream;
    FEncoding: TEncoding;
    procedure Init(const Stream: TStream; AEncoding: TEncoding);
    procedure CheckOpen;
    procedure WriteBytes(const Bytes: TBytes);
    procedure Write7BitEncodedInt(Value: Integer);
  public
    // Encoding nil is TEncoding.Default (UTF-8).
    constructor Create(const Stream: TStream; AEncoding: TEncoding = nil); overload;
    // On a TFileStream: a new file (or one cut to 0 bytes), or with Append, the end of the file
    // (created if missing).
    constructor Create(const FileName: string; Append: Boolean = False;
      AEncoding: TEncoding = nil); overload;
    // Closes the stream; writing afterwards raises EStreamError.
    procedure Close; virtual;
    // Does nothing: the writer has no buffer (the stream's own is the stream's).
    procedure Flush; virtual;
    // Moves the position of the stream; returns the new one.
    function Seek(Offset: Integer; Origin: TSeekOrigin): Int64; virtual;
    procedure Write(Value: Boolean); overload; virtual;
    procedure Write(Value: Byte); overload; virtual;
    procedure Write(Value: ShortInt); overload; virtual;
    procedure Write(Value: SmallInt); overload; virtual;
    procedure Write(Value: Word); overload; virtual;
    procedure Write(Value: Integer); overload; virtual;
    procedure Write(Value: Cardinal); overload; virtual;
    procedure Write(Value: Int64); overload; virtual;
    procedure Write(Value: UInt64); overload; virtual;
    procedure Write(Value: Single); overload; virtual;
    procedure Write(Value: Double); overload; virtual;
    // The character in the encoding.
    procedure Write(Value: Char); overload; virtual;
    // The length in bytes as a 7-bit encoded integer, then the bytes in the encoding.
    procedure Write(const Value: string); overload; virtual;
    procedure Write(const Value: TBytes); overload; virtual;
    procedure Write(const Value: TBytes; Index, Count: Integer); overload; virtual;
    // The characters in the encoding, with no length.
    procedure Write(const Value: TArray<Char>); overload; virtual;
    procedure Write(const Value: TArray<Char>; Index, Count: Integer); overload; virtual;
    property BaseStream: TStream read FStream;
  end;
