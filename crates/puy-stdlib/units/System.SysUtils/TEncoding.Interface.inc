type
  // Bytes, one byte per element (requirements §5.5).
  TBytes = TArray<Byte>;

  // The conversions between strings and bytes (docs/spec/stdlib/System.SysUtils/TEncoding.md).
  // The standard encodings (the class properties) belong to the unit: do not free them. An
  // encoding made by GetEncoding or Clone belongs to the caller.
  TEncoding = class
  private
    class var FUTF8: TEncoding;
    class var FUTF8Strict: TEncoding;
    class var FDefault: TEncoding;
    class var FUnicode: TEncoding;
    class var FBigEndianUnicode: TEncoding;
    class var FASCII: TEncoding;
    class function GetUTF8: TEncoding; static;
    class function GetUTF8Strict: TEncoding; static;
    class function GetDefault: TEncoding; static;
    class function GetUnicode: TEncoding; static;
    class function GetBigEndianUnicode: TEncoding; static;
    class function GetASCII: TEncoding; static;
    class procedure FreeStandard; static;
    // The conversions of each encoding
    function Encode(const S: string): TBytes; virtual; abstract;
    function Decode(const Bytes: TBytes; Index, Count: Integer): string; virtual; abstract;
    function GetCodePage: Cardinal; virtual; abstract;
    function GetEncodingName: string; virtual; abstract;
    function GetIsSingleByte: Boolean; virtual;
  public
    function Clone: TEncoding; virtual; abstract;
    function GetPreamble: TBytes; virtual; abstract;
    function GetBytes(const S: string): TBytes;
    function GetByteCount(const S: string): Integer;
    function GetString(const Bytes: TBytes): string; overload;
    function GetString(const Bytes: TBytes; ByteIndex, ByteCount: Integer): string; overload;
    function GetCharCount(const Bytes: TBytes): Integer;
    class function Convert(Source, Destination: TEncoding; const Bytes: TBytes): TBytes;
      overload;
    class function Convert(Source, Destination: TEncoding; const Bytes: TBytes;
      StartIndex, Count: Integer): TBytes; overload;
    class function GetBufferEncoding(const Buffer: TBytes; var AEncoding: TEncoding): Integer;
      overload;
    class function GetBufferEncoding(const Buffer: TBytes; var AEncoding: TEncoding;
      ADefaultEncoding: TEncoding): Integer; overload;
    class function GetEncoding(CodePage: Integer): TEncoding;
    class function IsStandardEncoding(AEncoding: TEncoding): Boolean;
    property CodePage: Cardinal read GetCodePage;
    property EncodingName: string read GetEncodingName;
    property IsSingleByte: Boolean read GetIsSingleByte;
    class property ASCII: TEncoding read GetASCII;
    class property BigEndianUnicode: TEncoding read GetBigEndianUnicode;
    class property Default: TEncoding read GetDefault;
    class property Unicode: TEncoding read GetUnicode;
    class property UTF8: TEncoding read GetUTF8;
    // Raises EEncodingError where UTF8 puts U+FFFD (Puy only; requirements §5.5).
    class property UTF8Strict: TEncoding read GetUTF8Strict;
  end;

  // UTF-8. UTF8 and UTF8Strict write the byte order mark as their preamble; Default does not.
  TUTF8Encoding = class(TEncoding)
  private
    FStrict: Boolean;
    FNoPreamble: Boolean;
    function Encode(const S: string): TBytes; override;
    function Decode(const Bytes: TBytes; Index, Count: Integer): string; override;
    function GetCodePage: Cardinal; override;
    function GetEncodingName: string; override;
  public
    function Clone: TEncoding; override;
    function GetPreamble: TBytes; override;
  end;

  // UTF-16, little-endian.
  TUnicodeEncoding = class(TEncoding)
  private
    function Encode(const S: string): TBytes; override;
    function Decode(const Bytes: TBytes; Index, Count: Integer): string; override;
    function GetCodePage: Cardinal; override;
    function GetEncodingName: string; override;
  public
    function Clone: TEncoding; override;
    function GetPreamble: TBytes; override;
  end;

  // UTF-16, big-endian.
  TBigEndianUnicodeEncoding = class(TUnicodeEncoding)
  private
    function Encode(const S: string): TBytes; override;
    function Decode(const Bytes: TBytes; Index, Count: Integer): string; override;
    function GetCodePage: Cardinal; override;
    function GetEncodingName: string; override;
  public
    function Clone: TEncoding; override;
    function GetPreamble: TBytes; override;
  end;
