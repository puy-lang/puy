// ===== The minimum and the maximum, ranges, signs, rounding, comparisons =====

const
  // The default Epsilon of SameValue, IsZero and CompareValue: relative to the smaller
  // magnitude, and absolute near 0
  DoubleResolution = 1E-12;
  SingleResolution = 1E-4;

function Min(const A, B: Integer): Integer;
begin
  if A < B then Result := A else Result := B;
end;

function Min(const A, B: Int64): Int64;
begin
  if A < B then Result := A else Result := B;
end;

function Min(const A, B: UInt64): UInt64;
begin
  if A < B then Result := A else Result := B;
end;

function Min(const A, B: Single): Single;
begin
  if A <> A then
    Result := A
  else if (B <> B) or (B < A) then
    Result := B
  else
    Result := A;
end;

function Min(const A, B: Double): Double;
begin
  if A <> A then
    Result := A
  else if (B <> B) or (B < A) then
    Result := B
  else
    Result := A;
end;

function Max(const A, B: Integer): Integer;
begin
  if A > B then Result := A else Result := B;
end;

function Max(const A, B: Int64): Int64;
begin
  if A > B then Result := A else Result := B;
end;

function Max(const A, B: UInt64): UInt64;
begin
  if A > B then Result := A else Result := B;
end;

function Max(const A, B: Single): Single;
begin
  if A <> A then
    Result := A
  else if (B <> B) or (B > A) then
    Result := B
  else
    Result := A;
end;

function Max(const A, B: Double): Double;
begin
  if A <> A then
    Result := A
  else if (B <> B) or (B > A) then
    Result := B
  else
    Result := A;
end;

function InRange(const AValue, AMin, AMax: Integer): Boolean;
begin
  Result := (AValue >= AMin) and (AValue <= AMax);
end;

function InRange(const AValue, AMin, AMax: Int64): Boolean;
begin
  Result := (AValue >= AMin) and (AValue <= AMax);
end;

function InRange(const AValue, AMin, AMax: UInt64): Boolean;
begin
  Result := (AValue >= AMin) and (AValue <= AMax);
end;

function InRange(const AValue, AMin, AMax: Double): Boolean;
begin
  Result := (AValue >= AMin) and (AValue <= AMax);
end;

// The exception of EnsureRange for AMin > AMax.
procedure RaiseRangeOrder(const AMin, AMax: string);
begin
  raise EInvalidArgument.Create('EnsureRange: the range ' + AMin + '..' + AMax + ' is empty');
end;

function EnsureRange(const AValue, AMin, AMax: Integer): Integer;
begin
  if AMin > AMax then
    RaiseRangeOrder(IntToStr(AMin), IntToStr(AMax));
  if AValue < AMin then
    Result := AMin
  else if AValue > AMax then
    Result := AMax
  else
    Result := AValue;
end;

function EnsureRange(const AValue, AMin, AMax: Int64): Int64;
begin
  if AMin > AMax then
    RaiseRangeOrder(IntToStr(AMin), IntToStr(AMax));
  if AValue < AMin then
    Result := AMin
  else if AValue > AMax then
    Result := AMax
  else
    Result := AValue;
end;

function EnsureRange(const AValue, AMin, AMax: UInt64): UInt64;
begin
  if AMin > AMax then
    RaiseRangeOrder(IntToStr(AMin), IntToStr(AMax));
  if AValue < AMin then
    Result := AMin
  else if AValue > AMax then
    Result := AMax
  else
    Result := AValue;
end;

function EnsureRange(const AValue, AMin, AMax: Double): Double;
begin
  if not (AMin <= AMax) then
    RaiseRangeOrder(FloatToStr(AMin), FloatToStr(AMax));
  if AValue < AMin then
    Result := AMin
  else if AValue > AMax then
    Result := AMax
  else
    Result := AValue;
end;

function Sign(const AValue: Integer): TValueSign;
begin
  if AValue < 0 then
    Result := NegativeValue
  else if AValue > 0 then
    Result := PositiveValue
  else
    Result := ZeroValue;
end;

function Sign(const AValue: Int64): TValueSign;
begin
  if AValue < 0 then
    Result := NegativeValue
  else if AValue > 0 then
    Result := PositiveValue
  else
    Result := ZeroValue;
end;

function Sign(const AValue: Double): TValueSign;
begin
  if AValue < 0 then
    Result := NegativeValue
  else if AValue > 0 then
    Result := PositiveValue
  else
    Result := ZeroValue;
end;

// F (an integral Double) as an Integer; EInvalidOp naming the routine and X when it is NaN, an
// infinity or outside Integer.
function IntegralToInteger(const F, X: Double; const Name: string): Integer;
begin
  if not ((F >= -2147483648.0) and (F <= 2147483647.0)) then
    raise EInvalidOp.Create('invalid floating-point operation: ' + Name + '(' +
      FloatToStr(X) + ')');
  Result := Trunc(F);
end;

function Floor(const X: Double): Integer;
var
  F: Double;
begin
  F := Int(X);
  if F > X then
    F := F - 1;
  Result := IntegralToInteger(F, X, 'Floor');
end;

function Ceil(const X: Double): Integer;
var
  F: Double;
begin
  F := Int(X);
  if F < X then
    F := F + 1;
  Result := IntegralToInteger(F, X, 'Ceil');
end;

// 10^N for 0 <= N <= 37 (exact up to 10^22).
function PowerOfTen(N: Integer): Double;
var
  I: Integer;
begin
  Result := 1;
  for I := 1 to N do
    Result := Result * 10;
end;

// X rounded to an integral Double: a half to the even one if HalfEven, else up. A value without
// a fraction (an integral value, an infinity, NaN) is itself.
function RoundIntegral(const X: Double; HalfEven: Boolean): Double;
var
  F: Double;
begin
  Result := Int(X);
  F := X - Result;
  if F > 0.5 then
    Result := Result + 1
  else if F < -0.5 then
    Result := Result - 1
  else if F = 0.5 then
  begin
    if not HalfEven or (Frac(Result / 2) <> 0) then
      Result := Result + 1;
  end
  else if F = -0.5 then
  begin
    if HalfEven and (Frac(Result / 2) <> 0) then
      Result := Result - 1;
  end;
end;

// AValue rounded to a multiple of 10^ADigit: a negative digit scales up by 10^-ADigit (exact
// for up to 22 digits), so that RoundTo(1.25, -1) rounds 12.5 rather than 1.25 / 0.1.
function RoundToDigit(const AValue: Double; ADigit: Integer; HalfEven: Boolean): Double;
var
  Scale: Double;
begin
  if ADigit < 0 then
  begin
    Scale := PowerOfTen(-ADigit);
    Result := RoundIntegral(AValue * Scale, HalfEven) / Scale;
  end
  else
  begin
    Scale := PowerOfTen(ADigit);
    Result := RoundIntegral(AValue / Scale, HalfEven) * Scale;
  end;
  // A value too large to scale is already a multiple
  if IsInfinite(Result) and not IsInfinite(AValue) then
    Result := AValue;
end;

function RoundTo(const AValue: Double; const ADigit: TRoundToEXRangeExtended): Double;
begin
  Result := RoundToDigit(AValue, ADigit, True);
end;

function SimpleRoundTo(const AValue: Double; const ADigit: TRoundToRange): Double;
begin
  Result := RoundToDigit(AValue, ADigit, False);
end;

function IsNan(const AValue: Single): Boolean;
begin
  Result := AValue <> AValue;
end;

function IsNan(const AValue: Double): Boolean;
begin
  Result := AValue <> AValue;
end;

function IsInfinite(const AValue: Double): Boolean;
begin
  Result := (AValue = Infinity) or (AValue = NegInfinity);
end;

// The Epsilon of SameValue for 0: DoubleResolution times the smaller magnitude, at least
// DoubleResolution.
function DefaultEpsilon(const A, B, Resolution: Double): Double;
begin
  Result := Min(Abs(A), Abs(B)) * Resolution;
  if not (Result >= Resolution) then
    Result := Resolution;
end;

function SameValue(const A, B: Double; Epsilon: Double): Boolean;
begin
  if Epsilon = 0 then
    Epsilon := DefaultEpsilon(A, B, DoubleResolution);
  // Equal infinities are the same; NaN is not the same as anything
  Result := (A = B) or (Abs(A - B) <= Epsilon);
end;

function SameValue(const A, B: Single; Epsilon: Single): Boolean;
var
  E: Double;
begin
  E := Epsilon;
  if E = 0 then
    E := DefaultEpsilon(A, B, SingleResolution);
  Result := (A = B) or (Abs(A - B) <= E);
end;

function IsZero(const A: Double; Epsilon: Double): Boolean;
begin
  if Epsilon = 0 then
    Epsilon := DoubleResolution;
  Result := Abs(A) <= Epsilon;
end;

function IsZero(const A: Single; Epsilon: Single): Boolean;
begin
  if Epsilon = 0 then
    Epsilon := SingleResolution;
  Result := Abs(A) <= Epsilon;
end;

function CompareValue(const A, B: Integer): TValueRelationship;
begin
  if A < B then
    Result := LessThanValue
  else if A > B then
    Result := GreaterThanValue
  else
    Result := EqualsValue;
end;

function CompareValue(const A, B: Int64): TValueRelationship;
begin
  if A < B then
    Result := LessThanValue
  else if A > B then
    Result := GreaterThanValue
  else
    Result := EqualsValue;
end;

function CompareValue(const A, B: UInt64): TValueRelationship;
begin
  if A < B then
    Result := LessThanValue
  else if A > B then
    Result := GreaterThanValue
  else
    Result := EqualsValue;
end;

function CompareValue(const A, B: Double; Epsilon: Double): TValueRelationship;
begin
  if SameValue(A, B, Epsilon) then
    Result := EqualsValue
  else if A < B then
    Result := LessThanValue
  else if A > B then
    Result := GreaterThanValue
  else
    // NaN is neither less nor greater, nor the same as anything
    raise EInvalidOp.Create('invalid floating-point operation: CompareValue(' +
      FloatToStr(A) + ', ' + FloatToStr(B) + ')');
end;
