// ===== TCharHelper (docs/spec/stdlib/System/Helpers.md) =====

type
  // The members of `Char` besides the built-in ToString.
  TCharHelper = record helper for Char
  public
    class function IsControl(C: Char): Boolean; overload; static;
    class function IsDigit(C: Char): Boolean; overload; static;
    class function IsHighSurrogate(C: Char): Boolean; overload; static;
    class function IsLetter(C: Char): Boolean; overload; static;
    class function IsLetterOrDigit(C: Char): Boolean; overload; static;
    class function IsLower(C: Char): Boolean; overload; static;
    class function IsLowSurrogate(C: Char): Boolean; overload; static;
    class function IsNumber(C: Char): Boolean; overload; static;
    class function IsSurrogate(C: Char): Boolean; overload; static;
    class function IsUpper(C: Char): Boolean; overload; static;
    class function IsWhiteSpace(C: Char): Boolean; overload; static;
    class function ToLower(C: Char): Char; overload; static;
    class function ToUpper(C: Char): Char; overload; static;

    function IsControl: Boolean; overload;
    function IsDigit: Boolean; overload;
    function IsHighSurrogate: Boolean; overload;
    function IsLetter: Boolean; overload;
    function IsLetterOrDigit: Boolean; overload;
    function IsLower: Boolean; overload;
    function IsLowSurrogate: Boolean; overload;
    function IsNumber: Boolean; overload;
    function IsSurrogate: Boolean; overload;
    function IsUpper: Boolean; overload;
    function IsWhiteSpace: Boolean; overload;
    function ToLower: Char; overload;
    function ToUpper: Char; overload;
  end;
