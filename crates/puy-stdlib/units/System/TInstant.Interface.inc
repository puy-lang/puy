// ===== TInstant (docs/spec/stdlib/System/TInstant.md) =====

type
  // An instant in UTC: nanoseconds since 1970-01-01T00:00:00Z (requirements §8.8), about the
  // years 1677 to 2262.
  TInstant = record
  private
    FNanos: Int64;
    class function Make(const Nanos: Int64): TInstant; static;
    class function Scaled(const N, Factor: Int64): TInstant; static;
    class function TryMake(const Seconds, Fraction: Int64; out Value: TInstant): Boolean; static;
  public
    class function Now: TInstant; static;
    class function FromUnixNanos(const Nanos: Int64): TInstant; static;
    class function FromUnixMillis(const Millis: Int64): TInstant; static;
    class function FromUnixSeconds(const Seconds: Int64): TInstant; static;
    class function FromDateTime(const Value: TDateTime): TInstant; static;
    class function Parse(const S: string): TInstant; static;
    class function TryParse(const S: string; out Value: TInstant): Boolean; static;

    function ToUnixNanos: Int64;
    function ToUnixMillis: Int64;
    function ToUnixSeconds: Int64;
    function ToDateTime: TDateTime;
    function ToString: string;

    function AddNanos(const Nanos: Int64): TInstant;
    function AddMillis(const Millis: Int64): TInstant;
    function AddSeconds(const Seconds: Int64): TInstant;
    function NanosSince(const Other: TInstant): Int64;
    function CompareTo(const Other: TInstant): Integer;
    function GetHashCode: Int64;

    class operator Equal(const A, B: TInstant): Boolean;
    class operator NotEqual(const A, B: TInstant): Boolean;
    class operator LessThan(const A, B: TInstant): Boolean;
    class operator LessThanOrEqual(const A, B: TInstant): Boolean;
    class operator GreaterThan(const A, B: TInstant): Boolean;
    class operator GreaterThanOrEqual(const A, B: TInstant): Boolean;
    class operator Add(const A: TInstant; const B: TTimeSpan): TInstant;
    class operator Add(const A: TTimeSpan; const B: TInstant): TInstant;
    class operator Subtract(const A: TInstant; const B: TTimeSpan): TInstant;
    class operator Subtract(const A, B: TInstant): TTimeSpan;
  end;
