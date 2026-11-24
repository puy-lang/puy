// ===== TStringHelper (docs/spec/stdlib/System/TStringHelper.md) =====

type
  // The members of `string` besides the built-in Length. The indexes are 0-based.
  TStringHelper = record helper for string
  private
    function GetChars(Index: Integer): Char;
  public
    class function Compare(const StrA, StrB: string): Integer; overload; static;
    class function Compare(const StrA, StrB: string; IgnoreCase: Boolean): Integer; overload; static;
    class function IsNullOrEmpty(const Value: string): Boolean; static;
    class function Join(const Separator: string; const Values: array of string): string; static;

    function CompareTo(const StrB: string): Integer;
    function Contains(const Value: string): Boolean;
    function CountChar(C: Char): Integer;
    function EndsWith(const Value: string): Boolean; overload;
    function EndsWith(const Value: string; IgnoreCase: Boolean): Boolean; overload;
    function Equals(const Value: string): Boolean;
    function IndexOf(const Value: string): Integer; overload;
    function IndexOf(const Value: string; StartIndex: Integer): Integer; overload;
    function IndexOf(const Value: string; StartIndex, Count: Integer): Integer; overload;
    function IndexOfAny(const AnyOf: array of Char): Integer; overload;
    function IndexOfAny(const AnyOf: array of Char; StartIndex: Integer): Integer; overload;
    function Insert(StartIndex: Integer; const Value: string): string;
    function IsEmpty: Boolean;
    function LastIndexOf(const Value: string): Integer; overload;
    function LastIndexOf(const Value: string; StartIndex: Integer): Integer; overload;
    function LastIndexOfAny(const AnyOf: array of Char): Integer; overload;
    function LastIndexOfAny(const AnyOf: array of Char; StartIndex: Integer): Integer; overload;
    function PadLeft(TotalWidth: Integer): string; overload;
    function PadLeft(TotalWidth: Integer; PaddingChar: Char): string; overload;
    function PadRight(TotalWidth: Integer): string; overload;
    function PadRight(TotalWidth: Integer; PaddingChar: Char): string; overload;
    function QuotedString: string; overload;
    function QuotedString(QuoteChar: Char): string; overload;
    function Remove(StartIndex: Integer): string; overload;
    function Remove(StartIndex, Count: Integer): string; overload;
    function Replace(const OldValue, NewValue: string): string;
    function Split(const Separators: array of string): TArray<string>;
    function StartsWith(const Value: string): Boolean; overload;
    function StartsWith(const Value: string; IgnoreCase: Boolean): Boolean; overload;
    function Substring(StartIndex: Integer): string; overload;
    function Substring(StartIndex, Length: Integer): string; overload;
    function ToCharArray: TArray<Char>;
    function ToDouble: Double;
    function ToInt64: Int64;
    function ToInteger: Integer;
    function ToLower: string;
    function ToUpper: string;
    function Trim: string; overload;
    function Trim(const TrimChars: array of Char): string; overload;
    function TrimLeft: string; overload;
    function TrimLeft(const TrimChars: array of Char): string; overload;
    function TrimRight: string; overload;
    function TrimRight(const TrimChars: array of Char): string; overload;

    property Chars[Index: Integer]: Char read GetChars;
  end;
