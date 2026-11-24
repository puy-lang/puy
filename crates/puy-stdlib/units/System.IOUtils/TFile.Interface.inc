type
  // How TFile.Open opens a file. The modes that may create one open it to read and write.
  TFileMode = (fmCreateNew, fmCreate, fmOpen, fmOpenOrCreate, fmTruncate, fmAppend);
  // What TFile.Open opens an existing file for.
  TFileAccess = (faRead, faWrite, faReadWrite);
  // The attributes of a file Puy keeps on every target (spec/stdlib/System.IOUtils/TFile.md).
  TFileAttribute = (faReadOnly, faDirectory, faSymLink, faNormal);
  TFileAttributes = set of TFileAttribute;

  // The lines of a file, read as they are enumerated (TFile.ReadLines). Each enumeration reads
  // the file from its start.
  TFileLineEnumerator = class(TRefObject)
  private
    FStream: TFileStream;
    FEncoding: TEncoding;
    FWide: Boolean;
    FBigEndian: Boolean;
    FBuffer: TBytes;
    FPos: Integer;
    FLen: Integer;
    FSkipLF: Boolean;
    FStarted: Boolean;
    FCurrent: string;
    function Fill: Boolean;
    function UnitAt(P: Integer): Integer;
  public
    constructor Create(const AStream: TFileStream; const AEncoding: TEncoding);
    function MoveNext: Boolean;
    property Current: string read FCurrent;
  end;

  TFileLines = class(TRefObject)
  private
    FPath: string;
    FEncoding: TEncoding;
    FFirst: TFileStream;
  public
    constructor Create(const APath: string; const AEncoding: TEncoding; const AFirst: TFileStream);
    function GetEnumerator: TFileLineEnumerator;
  end;

  // Whole files (docs/spec/stdlib/System.IOUtils/TFile.md). Text is UTF-8 without a BOM unless
  // an encoding is given; reading detects a BOM. A missing file is EFileNotFoundException, a
  // missing directory to write in EDirectoryNotFoundException, an empty path
  // EArgumentException; the other failures of the host are EInOutError (or the stream
  // exceptions of TFileStream) with their kind and code.
  TFile = record
  public
    class function Exists(const Path: string; FollowLink: Boolean = True): Boolean; static;
    class function ReadAllBytes(const Path: string): TBytes; static;
    class procedure WriteAllBytes(const Path: string; const Bytes: TBytes); static;
    class function ReadAllText(const Path: string): string; overload; static;
    class function ReadAllText(const Path: string; const Encoding: TEncoding): string; overload;
      static;
    class procedure WriteAllText(const Path, Contents: string); overload; static;
    class procedure WriteAllText(const Path, Contents: string; const Encoding: TEncoding);
      overload; static;
    class procedure AppendAllText(const Path, Contents: string); overload; static;
    class procedure AppendAllText(const Path, Contents: string; const Encoding: TEncoding);
      overload; static;
    class function ReadAllLines(const Path: string): TArray<string>; overload; static;
    class function ReadAllLines(const Path: string; const Encoding: TEncoding): TArray<string>;
      overload; static;
    class procedure WriteAllLines(const Path: string; const Contents: TArray<string>); overload;
      static;
    class procedure WriteAllLines(const Path: string; const Contents: TArray<string>;
      const Encoding: TEncoding); overload; static;
    class function ReadLines(const Path: string): TFileLines; overload; static;
    class function ReadLines(const Path: string; const Encoding: TEncoding): TFileLines;
      overload; static;
    class procedure Delete(const Path: string); static;
    class procedure Copy(const SourceFileName, DestFileName: string;
      Overwrite: Boolean = False); static;
    class procedure Move(const SourceFileName, DestFileName: string); static;
    class function Create(const Path: string): TFileStream; static;
    class function Open(const Path: string; Mode: TFileMode): TFileStream; overload; static;
    class function Open(const Path: string; Mode: TFileMode; Access: TFileAccess): TFileStream;
      overload; static;
    class function OpenRead(const Path: string): TFileStream; static;
    class function OpenWrite(const Path: string): TFileStream; static;
    class function GetSize(const Path: string): Int64; static;
    class function GetAttributes(const Path: string; FollowLink: Boolean = True): TFileAttributes;
      static;
    class procedure SetAttributes(const Path: string; const Attributes: TFileAttributes); static;
    // The times in the local time as TDateTime (the time zone of System.SysUtils's Now)
    class function GetCreationTime(const Path: string): TDateTime; static;
    class function GetLastAccessTime(const Path: string): TDateTime; static;
    class function GetLastWriteTime(const Path: string): TDateTime; static;
    class procedure SetCreationTime(const Path: string; const CreationTime: TDateTime); static;
    class procedure SetLastAccessTime(const Path: string; const LastAccessTime: TDateTime);
      static;
    class procedure SetLastWriteTime(const Path: string; const LastWriteTime: TDateTime); static;
    // The times in UTC as TDateTime
    class function GetCreationTimeUtc(const Path: string): TDateTime; static;
    class function GetLastAccessTimeUtc(const Path: string): TDateTime; static;
    class function GetLastWriteTimeUtc(const Path: string): TDateTime; static;
    class procedure SetCreationTimeUtc(const Path: string; const CreationTime: TDateTime); static;
    class procedure SetLastAccessTimeUtc(const Path: string; const LastAccessTime: TDateTime);
      static;
    class procedure SetLastWriteTimeUtc(const Path: string; const LastWriteTime: TDateTime);
      static;
    // The times as TInstant, to the nanosecond
    class function GetCreationInstant(const Path: string): TInstant; static;
    class function GetLastAccessInstant(const Path: string): TInstant; static;
    class function GetLastWriteInstant(const Path: string): TInstant; static;
    class procedure SetCreationInstant(const Path: string; const CreationInstant: TInstant);
      static;
    class procedure SetLastAccessInstant(const Path: string; const LastAccessInstant: TInstant);
      static;
    class procedure SetLastWriteInstant(const Path: string; const LastWriteInstant: TInstant);
      static;
  end;
