// ===== Floating-point type helpers (docs/spec/stdlib/System/Helpers.md) =====

type
  // The members of `Single` besides the built-in ToString.
  TSingleHelper = record helper for Single
  public
    const Epsilon: Single = 1.4012984643248171e-45;
    const MaxValue: Single = 3.4028234663852886e38;
    const MinValue: Single = -3.4028234663852886e38;
    const NaN: Single = 0.0 / 0.0;
    const NegativeInfinity: Single = -1.0 / 0.0;
    const PositiveInfinity: Single = 1.0 / 0.0;
    class function FromBits(Bits: Cardinal): Single; static;
    class function Parse(const S: string): Single; static;
    class function TryParse(const S: string; out Value: Single): Boolean; static;

    function IsInfinity: Boolean;
    function IsNan: Boolean;
    function IsNegativeInfinity: Boolean;
    function IsPositiveInfinity: Boolean;
    function ToBits: Cardinal;
  end;

  // The members of `Double` besides the built-in ToString.
  TDoubleHelper = record helper for Double
  public
    const Epsilon: Double = 4.9406564584124654e-324;
    const MaxValue: Double = 1.7976931348623157e308;
    const MinValue: Double = -1.7976931348623157e308;
    const NaN: Double = 0.0 / 0.0;
    const NegativeInfinity: Double = -1.0 / 0.0;
    const PositiveInfinity: Double = 1.0 / 0.0;
    class function FromBits(Bits: UInt64): Double; static;
    class function Parse(const S: string): Double; static;
    class function TryParse(const S: string; out Value: Double): Boolean; static;

    function IsInfinity: Boolean;
    function IsNan: Boolean;
    function IsNegativeInfinity: Boolean;
    function IsPositiveInfinity: Boolean;
    function ToBits: UInt64;
  end;
