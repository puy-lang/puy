type
  // Where TStream.Seek counts from.
  TSeekOrigin = (soBeginning, soCurrent, soEnd);

  // A stream that fails: a closed stream, a position before the start, a stream that cannot
  // change its size. ErrorKind and ErrorCode (Puy only) are those of the host's failure, as in
  // EInOutError; ekNone and 0 when the host did not fail.
  EStreamError = class(Exception)
  private
    FErrorKind: TIOErrorKind;
    FErrorCode: Integer;
  public
    property ErrorKind: TIOErrorKind read FErrorKind write FErrorKind;
    property ErrorCode: Integer read FErrorCode write FErrorCode;
  end;

  // TFileStream could not create the file (fmCreate).
  EFCreateError = class(EStreamError);
  // TFileStream could not open the file.
  EFOpenError = class(EStreamError);
  EFilerError = class(EStreamError);
  // A read that failed or ended early (ReadBuffer, ReadBufferData, CopyFrom).
  EReadError = class(EFilerError);
  // A write that failed or was cut short.
  EWriteError = class(EFilerError);

  // The base of the streams: a sequence of bytes with a position. A descendant overrides the
  // three cores, Read, Write and Seek (and GetSize / SetSize when it has a faster way); the
  // rest is written on them. The values of ReadData / WriteData are little-endian on every
  // target. Streams are reference counted: one closes when its last reference goes, or at once
  // with Close.
  TStream = class(TRefObject)
  private
    // The bytes of a value of ReadData / WriteData
    FScratch: TBytes;
    function GetPosition: Int64;
    procedure SetPosition(const Pos: Int64);
    function ReadBits(var Bits: UInt64; Size, Count: Integer): Integer;
    function WriteBits(Bits: UInt64; Size, Count: Integer): Integer;
  protected
    function GetSize: Int64; virtual;
    procedure SetSize(const NewSize: Int64); virtual;
    // Whether Seek, Size and Position work (Puy only); a stream that cannot seek raises
    // EStreamError from them.
    function GetCanSeek: Boolean; virtual;
    // ERangeError unless the bytes Offset..Offset + Count - 1 are in Buffer (Puy only): the
    // check of the standard streams' Read and Write, for a descendant to make the same.
    class procedure CheckBufferRange(const Buffer: TBytes; Offset, Count: Integer); static;
  public
    // Reads up to Count bytes into Buffer from Offset; returns how many were read (0 at the
    // end). ERangeError if the bytes Offset..Offset + Count - 1 are not in Buffer.
    function Read(var Buffer: TBytes; Offset, Count: Integer): Integer; overload; virtual;
      abstract;
    function Read(var Buffer: TBytes; Count: Integer): Integer; overload;
    // Writes Count bytes of Buffer from Offset; returns how many were written.
    function Write(const Buffer: TBytes; Offset, Count: Integer): Integer; overload; virtual;
      abstract;
    function Write(const Buffer: TBytes; Count: Integer): Integer; overload;
    // Moves the position; returns the new one.
    function Seek(const Offset: Int64; Origin: TSeekOrigin): Int64; virtual; abstract;
    // Read / Write that raise EReadError / EWriteError unless all Count bytes are moved.
    procedure ReadBuffer(var Buffer: TBytes; Count: Integer); overload;
    procedure ReadBuffer(var Buffer: TBytes; Offset, Count: Integer); overload;
    procedure WriteBuffer(const Buffer: TBytes; Count: Integer); overload;
    procedure WriteBuffer(const Buffer: TBytes; Offset, Count: Integer); overload;
    // Copies Count bytes from Source's position (Count = 0: all of Source, from its start);
    // returns the number copied. EReadError if Source ends first.
    function CopyFrom(const Source: TStream; Count: Int64 = 0): Int64;
    // Closes the stream at once (Puy only): a file is closed, a stream in memory lets its
    // bytes go, and reading, writing or seeking afterwards raises EStreamError. Close again does
    // nothing. TStream's own does nothing: a descendant that holds something overrides it.
    procedure Close; virtual;
    // Writes S in AEncoding (TEncoding.Default, UTF-8, when nil or left out), with no byte order
    // mark (Puy only).
    procedure WriteText(const S: string); overload;
    procedure WriteText(const S: string; AEncoding: TEncoding); overload;
    // The value of Buffer from its SizeOf(Buffer) bytes, little-endian; returns how many bytes
    // were read (fewer at the end: the low bytes read replace those of Buffer). With Count, reads
    // Count bytes: the first SizeOf(Buffer) go to Buffer and the others are skipped.
    function ReadData(var Buffer: Boolean): Integer; overload;
    function ReadData(var Buffer: Boolean; Count: Integer): Integer; overload;
    function ReadData(var Buffer: Char): Integer; overload;
    function ReadData(var Buffer: Char; Count: Integer): Integer; overload;
    function ReadData(var Buffer: ShortInt): Integer; overload;
    function ReadData(var Buffer: ShortInt; Count: Integer): Integer; overload;
    function ReadData(var Buffer: Byte): Integer; overload;
    function ReadData(var Buffer: Byte; Count: Integer): Integer; overload;
    function ReadData(var Buffer: SmallInt): Integer; overload;
    function ReadData(var Buffer: SmallInt; Count: Integer): Integer; overload;
    function ReadData(var Buffer: Word): Integer; overload;
    function ReadData(var Buffer: Word; Count: Integer): Integer; overload;
    function ReadData(var Buffer: Integer): Integer; overload;
    function ReadData(var Buffer: Integer; Count: Integer): Integer; overload;
    function ReadData(var Buffer: Cardinal): Integer; overload;
    function ReadData(var Buffer: Cardinal; Count: Integer): Integer; overload;
    function ReadData(var Buffer: Int64): Integer; overload;
    function ReadData(var Buffer: Int64; Count: Integer): Integer; overload;
    function ReadData(var Buffer: UInt64): Integer; overload;
    function ReadData(var Buffer: UInt64; Count: Integer): Integer; overload;
    function ReadData(var Buffer: Single): Integer; overload;
    function ReadData(var Buffer: Single; Count: Integer): Integer; overload;
    function ReadData(var Buffer: Double): Integer; overload;
    function ReadData(var Buffer: Double; Count: Integer): Integer; overload;
    // Writes the SizeOf(Buffer) bytes of Buffer, little-endian; returns how many were written.
    // With Count, writes Count bytes: the first SizeOf(Buffer) of Buffer, then zeros.
    function WriteData(const Buffer: Boolean): Integer; overload;
    function WriteData(const Buffer: Boolean; Count: Integer): Integer; overload;
    function WriteData(const Buffer: Char): Integer; overload;
    function WriteData(const Buffer: Char; Count: Integer): Integer; overload;
    function WriteData(const Buffer: ShortInt): Integer; overload;
    function WriteData(const Buffer: ShortInt; Count: Integer): Integer; overload;
    function WriteData(const Buffer: Byte): Integer; overload;
    function WriteData(const Buffer: Byte; Count: Integer): Integer; overload;
    function WriteData(const Buffer: SmallInt): Integer; overload;
    function WriteData(const Buffer: SmallInt; Count: Integer): Integer; overload;
    function WriteData(const Buffer: Word): Integer; overload;
    function WriteData(const Buffer: Word; Count: Integer): Integer; overload;
    function WriteData(const Buffer: Integer): Integer; overload;
    function WriteData(const Buffer: Integer; Count: Integer): Integer; overload;
    function WriteData(const Buffer: Cardinal): Integer; overload;
    function WriteData(const Buffer: Cardinal; Count: Integer): Integer; overload;
    function WriteData(const Buffer: Int64): Integer; overload;
    function WriteData(const Buffer: Int64; Count: Integer): Integer; overload;
    function WriteData(const Buffer: UInt64): Integer; overload;
    function WriteData(const Buffer: UInt64; Count: Integer): Integer; overload;
    function WriteData(const Buffer: Single): Integer; overload;
    function WriteData(const Buffer: Single; Count: Integer): Integer; overload;
    function WriteData(const Buffer: Double): Integer; overload;
    function WriteData(const Buffer: Double; Count: Integer): Integer; overload;
    // ReadData / WriteData that raise EReadError / EWriteError unless all the bytes are moved.
    procedure ReadBufferData(var Buffer: Boolean); overload;
    procedure ReadBufferData(var Buffer: Boolean; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: Char); overload;
    procedure ReadBufferData(var Buffer: Char; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: ShortInt); overload;
    procedure ReadBufferData(var Buffer: ShortInt; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: Byte); overload;
    procedure ReadBufferData(var Buffer: Byte; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: SmallInt); overload;
    procedure ReadBufferData(var Buffer: SmallInt; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: Word); overload;
    procedure ReadBufferData(var Buffer: Word; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: Integer); overload;
    procedure ReadBufferData(var Buffer: Integer; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: Cardinal); overload;
    procedure ReadBufferData(var Buffer: Cardinal; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: Int64); overload;
    procedure ReadBufferData(var Buffer: Int64; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: UInt64); overload;
    procedure ReadBufferData(var Buffer: UInt64; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: Single); overload;
    procedure ReadBufferData(var Buffer: Single; Count: Integer); overload;
    procedure ReadBufferData(var Buffer: Double); overload;
    procedure ReadBufferData(var Buffer: Double; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Boolean); overload;
    procedure WriteBufferData(const Buffer: Boolean; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Char); overload;
    procedure WriteBufferData(const Buffer: Char; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: ShortInt); overload;
    procedure WriteBufferData(const Buffer: ShortInt; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Byte); overload;
    procedure WriteBufferData(const Buffer: Byte; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: SmallInt); overload;
    procedure WriteBufferData(const Buffer: SmallInt; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Word); overload;
    procedure WriteBufferData(const Buffer: Word; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Integer); overload;
    procedure WriteBufferData(const Buffer: Integer; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Cardinal); overload;
    procedure WriteBufferData(const Buffer: Cardinal; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Int64); overload;
    procedure WriteBufferData(const Buffer: Int64; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: UInt64); overload;
    procedure WriteBufferData(const Buffer: UInt64; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Single); overload;
    procedure WriteBufferData(const Buffer: Single; Count: Integer); overload;
    procedure WriteBufferData(const Buffer: Double); overload;
    procedure WriteBufferData(const Buffer: Double; Count: Integer); overload;
    property Position: Int64 read GetPosition write SetPosition;
    property Size: Int64 read GetSize write SetSize;
    property CanSeek: Boolean read GetCanSeek;
  end;

  // A stream on a file of the host (THandleStream's handle is the file's, not an OS handle), or
  // of the file system a program installed in place of the host's (TFileSystem of
  // System.SysUtils; Handle is nil for those). Reading, writing and seeking wait for the host; a
  // failure of the host raises EReadError, EWriteError or EStreamError with its ErrorKind and
  // ErrorCode, and ENotSupportedException where there is no file system.
  THandleStream = class(TStream)
  private
    // The host's file (nil for a file of an installed file system, and once closed)
    FHandle: TFileHandle;
    // The file every operation goes through: FHandle's, or the installed file system's
    FFile: IFileHandle;
    FFileName: string;
    procedure CheckOpen;
  protected
    function GetSize: Int64; override;
    procedure SetSize(const NewSize: Int64); override;
  public
    constructor Create(const AHandle: TFileHandle);
    destructor Destroy; override;
    function Read(var Buffer: TBytes; Offset, Count: Integer): Integer; overload; override;
    function Write(const Buffer: TBytes; Offset, Count: Integer): Integer; overload; override;
    function Seek(const Offset: Int64; Origin: TSeekOrigin): Int64; override;
    procedure Close; override;
    property Handle: TFileHandle read FHandle;
  end;

  // A stream on a file opened by name. Mode is fmCreate (create the file, or cut it to 0
  // bytes, to read and write) or one of fmOpenRead, fmOpenWrite and fmOpenReadWrite (the file
  // must exist); a share mode `or`ed in has no effect. Rights is accepted and has no effect.
  // EFCreateError / EFOpenError when the host refuses.
  TFileStream = class(THandleStream)
  public
    constructor Create(const AFileName: string; Mode: Word); overload;
    constructor Create(const AFileName: string; Mode: Word; Rights: Cardinal); overload;
    property FileName: string read FFileName;
  end;

  // A stream in memory. The bytes are kept in blocks that grow as the stream does, so that it
  // can hold more than 2GB and never copies its bytes to grow; there is no pointer to them
  // (Memory: E3022), ToBytes copies them out. Seeking past the end is allowed: writing there
  // fills the gap with zeros.
  TMemoryStream = class(TStream)
  private
    // The blocks and where each starts in the stream
    FChunks: TArray<TBytes>;
    FStarts: TArray<Int64>;
    FCapacity: Int64;
    FSize: Int64;
    FPosition: Int64;
    FClosed: Boolean;
    // One block that grows by copying, at most MaxInt bytes (TBytesStream)
    FSingle: Boolean;
    procedure CheckOpen;
    procedure Reserve(const NewCapacity: Int64);
    function ChunkAt(const Pos: Int64): Integer;
    procedure CopyOut(const Pos: Int64; var Buffer: TBytes; Offset, Count: Integer);
    procedure CopyIn(const Pos: Int64; const Buffer: TBytes; Offset, Count: Integer);
    procedure CopyTo(const Stream: TStream; const From, Count: Int64);
  protected
    function GetSize: Int64; override;
    procedure SetSize(const NewSize: Int64); override;
  public
    function Read(var Buffer: TBytes; Offset, Count: Integer): Integer; overload; override;
    function Write(const Buffer: TBytes; Offset, Count: Integer): Integer; overload; override;
    function Seek(const Offset: Int64; Origin: TSeekOrigin): Int64; override;
    procedure Close; override;
    // Empties the stream (Size and Position 0) and lets its bytes go.
    procedure Clear;
    // Replaces the bytes with all of Stream's (from its start) / those of the file.
    procedure LoadFromStream(const Stream: TStream);
    procedure LoadFromFile(const FileName: string);
    // Writes all the bytes to Stream (at its position) / to a new file; the position of this
    // stream does not change.
    procedure SaveToStream(const Stream: TStream);
    procedure SaveToFile(const FileName: string);
    // A copy of the bytes (Puy only). EOutOfMemory when there are more than MaxInt.
    function ToBytes: TBytes;
  end;

  // A stream in memory held in one TBytes, at most MaxInt bytes (EOutOfMemory beyond).
  TBytesStream = class(TMemoryStream)
  private
    function GetBytes: TBytes;
  public
    // An empty stream, or one whose bytes are ABytes (shared with the caller's array until the
    // stream grows).
    constructor Create; overload;
    constructor Create(const ABytes: TBytes); overload;
    // The bytes themselves (not a copy), Size of them: writing to the array writes the stream
    // until the stream grows or shrinks.
    property Bytes: TBytes read GetBytes;
  end;

  // A stream in memory read and written as text in an encoding (TEncoding.Default, UTF-8
  // without a byte order mark, unless one is given).
  TStringStream = class(TMemoryStream)
  private
    FEncoding: TEncoding;
    FOwnsEncoding: Boolean;
    procedure Init(const AString: string; AEncoding: TEncoding; AOwnsEncoding: Boolean);
    function GetDataString: string;
    function GetBytes: TBytes;
  public
    constructor Create; overload;
    constructor Create(const AString: string); overload;
    // An encoding that is not standard is freed with the stream when AOwnsEncoding.
    constructor Create(const AString: string; AEncoding: TEncoding;
      AOwnsEncoding: Boolean = True); overload;
    constructor Create(const ABytes: TBytes); overload;
    destructor Destroy; override;
    // Count bytes from the position, as text.
    function ReadString(Count: Integer): string;
    // The text in the encoding, at the position (no byte order mark).
    procedure WriteString(const AString: string);
    // All the bytes as text (EOutOfMemory when there are more than MaxInt).
    property DataString: string read GetDataString;
    // A copy of the bytes (TStringStream is not a TBytesStream in Puy).
    property Bytes: TBytes read GetBytes;
    property Encoding: TEncoding read FEncoding;
  end;

const
  // TFileStream: create the file (or cut it to 0 bytes) to read and write.
  fmCreate = $FF00;
