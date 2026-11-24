// ===== The minimum and the maximum, ranges, signs, rounding, comparisons =====
// (docs/spec/stdlib/System.Math/README.md chapter 2)

// The smaller and the larger of two values. For real numbers, NaN if either is NaN.
function Min(const A, B: Integer): Integer; overload;
function Min(const A, B: Int64): Int64; overload;
function Min(const A, B: UInt64): UInt64; overload;
function Min(const A, B: Single): Single; overload;
function Min(const A, B: Double): Double; overload;
function Max(const A, B: Integer): Integer; overload;
function Max(const A, B: Int64): Int64; overload;
function Max(const A, B: UInt64): UInt64; overload;
function Max(const A, B: Single): Single; overload;
function Max(const A, B: Double): Double; overload;

// Whether AMin <= AValue <= AMax.
function InRange(const AValue, AMin, AMax: Integer): Boolean; overload;
function InRange(const AValue, AMin, AMax: Int64): Boolean; overload;
function InRange(const AValue, AMin, AMax: UInt64): Boolean; overload;
function InRange(const AValue, AMin, AMax: Double): Boolean; overload;

// AValue moved into AMin..AMax. EInvalidArgument if AMin > AMax (or either is NaN).
function EnsureRange(const AValue, AMin, AMax: Integer): Integer; overload;
function EnsureRange(const AValue, AMin, AMax: Int64): Int64; overload;
function EnsureRange(const AValue, AMin, AMax: UInt64): UInt64; overload;
function EnsureRange(const AValue, AMin, AMax: Double): Double; overload;

// -1, 0 or 1 by the sign of the value (0 for -0.0 and NaN).
function Sign(const AValue: Integer): TValueSign; overload;
function Sign(const AValue: Int64): TValueSign; overload;
function Sign(const AValue: Double): TValueSign; overload;

// The largest integer not above X / the smallest integer not below X. EInvalidOp for NaN, an
// infinity or a result outside Integer.
function Floor(const X: Double): Integer;
function Ceil(const X: Double): Integer;

// AValue rounded to a multiple of 10^ADigit, a half to the even multiple.
function RoundTo(const AValue: Double; const ADigit: TRoundToEXRangeExtended): Double;
// AValue rounded to a multiple of 10^ADigit, a half up (toward positive infinity).
function SimpleRoundTo(const AValue: Double; const ADigit: TRoundToRange = -2): Double;

// The kinds of a real number.
function IsNan(const AValue: Single): Boolean; overload;
function IsNan(const AValue: Double): Boolean; overload;
function IsInfinite(const AValue: Double): Boolean;

// Whether A and B differ by at most Epsilon; Epsilon = 0 picks one from their magnitude.
function SameValue(const A, B: Double; Epsilon: Double = 0): Boolean; overload;
function SameValue(const A, B: Single; Epsilon: Single = 0): Boolean; overload;
// Whether A is within Epsilon of 0; Epsilon = 0 picks a small one.
function IsZero(const A: Double; Epsilon: Double = 0): Boolean; overload;
function IsZero(const A: Single; Epsilon: Single = 0): Boolean; overload;

// -1 if A < B, 0 if they are equal (within Epsilon for real numbers), 1 if A > B.
function CompareValue(const A, B: Integer): TValueRelationship; overload;
function CompareValue(const A, B: Int64): TValueRelationship; overload;
function CompareValue(const A, B: UInt64): TValueRelationship; overload;
function CompareValue(const A, B: Double; Epsilon: Double = 0): TValueRelationship; overload;
