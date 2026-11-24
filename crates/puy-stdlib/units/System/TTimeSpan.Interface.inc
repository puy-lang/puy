// ===== TTimeSpan (docs/spec/stdlib/System/TTimeSpan.md) =====

type
  // A length of time: a signed count of ticks of 100 nanoseconds (about 29,227 years either
  // way). System.TimeSpan names the same type.
  TTimeSpan = record
  private
    FTicks: Int64;
    class function Make(const Ticks: Int64): TTimeSpan; static;
    class function Scaled(const Value: Double; const Factor: Int64): TTimeSpan; static;
    class function TryParseText(const S: string; out Value: TTimeSpan): Boolean; static;
    class function GetZero: TTimeSpan; static;
    class function GetMinValue: TTimeSpan; static;
    class function GetMaxValue: TTimeSpan; static;
    function GetDays: Integer;
    function GetHours: Integer;
    function GetMinutes: Integer;
    function GetSeconds: Integer;
    function GetMilliseconds: Integer;
    function GetTotalDays: Double;
    function GetTotalHours: Double;
    function GetTotalMinutes: Double;
    function GetTotalSeconds: Double;
    function GetTotalMilliseconds: Double;
  public
    const
      TicksPerMillisecond = 10000;
      TicksPerSecond = 10000000;
      TicksPerMinute = 600000000;
      TicksPerHour = 36000000000;
      TicksPerDay = 864000000000;

    constructor Create(const ATicks: Int64); overload;
    constructor Create(Hours, Minutes, Seconds: Integer); overload;
    constructor Create(Days, Hours, Minutes, Seconds: Integer); overload;
    constructor Create(Days, Hours, Minutes, Seconds, Milliseconds: Integer); overload;

    class function FromDays(const Value: Double): TTimeSpan; static;
    class function FromHours(const Value: Double): TTimeSpan; static;
    class function FromMinutes(const Value: Double): TTimeSpan; static;
    class function FromSeconds(const Value: Double): TTimeSpan; static;
    class function FromMilliseconds(const Value: Double): TTimeSpan; static;
    class function FromTicks(const Value: Int64): TTimeSpan; static;
    class function Parse(const S: string): TTimeSpan; static;
    class function TryParse(const S: string; out Value: TTimeSpan): Boolean; static;

    function Add(const TS: TTimeSpan): TTimeSpan;
    function Subtract(const TS: TTimeSpan): TTimeSpan;
    function Duration: TTimeSpan;
    function Negate: TTimeSpan;
    function CompareTo(const Other: TTimeSpan): Integer;
    function ToString: string;
    function GetHashCode: Int64;

    class operator Add(const A, B: TTimeSpan): TTimeSpan;
    class operator Subtract(const A, B: TTimeSpan): TTimeSpan;
    class operator Negative(const A: TTimeSpan): TTimeSpan;
    class operator Positive(const A: TTimeSpan): TTimeSpan;
    class operator Equal(const A, B: TTimeSpan): Boolean;
    class operator NotEqual(const A, B: TTimeSpan): Boolean;
    class operator LessThan(const A, B: TTimeSpan): Boolean;
    class operator LessThanOrEqual(const A, B: TTimeSpan): Boolean;
    class operator GreaterThan(const A, B: TTimeSpan): Boolean;
    class operator GreaterThanOrEqual(const A, B: TTimeSpan): Boolean;

    property Ticks: Int64 read FTicks;
    property Days: Integer read GetDays;
    property Hours: Integer read GetHours;
    property Minutes: Integer read GetMinutes;
    property Seconds: Integer read GetSeconds;
    property Milliseconds: Integer read GetMilliseconds;
    property TotalDays: Double read GetTotalDays;
    property TotalHours: Double read GetTotalHours;
    property TotalMinutes: Double read GetTotalMinutes;
    property TotalSeconds: Double read GetTotalSeconds;
    property TotalMilliseconds: Double read GetTotalMilliseconds;

    class property Zero: TTimeSpan read GetZero;
    class property MinValue: TTimeSpan read GetMinValue;
    class property MaxValue: TTimeSpan read GetMaxValue;
  end;
