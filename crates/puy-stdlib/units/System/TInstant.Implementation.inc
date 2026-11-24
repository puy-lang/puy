// ===== TInstant (docs/spec/stdlib/System/TInstant.md) =====

const
  InstantNanosPerMilli = 1000000;
  // A tick of TTimeSpan
  InstantNanosPerTick = 100;
  InstantNanosPerSecond = 1000000000;
  InstantNanosPerDay = 86400000000000;
  InstantMillisPerDay = 86400000;
  // The TDateTime of 1970-01-01 (the days from 1899-12-30)
  InstantUnixEpochDays = 25569;
  // The TDateTime range of the years 1 to 9999 (0001-01-01 and 10000-01-01)
  InstantMinDateTime = -693593;
  InstantMaxDateTime = 2958466;

procedure InstantRangeError;
begin
  raise ERangeError.Create('TInstant value out of range (1677-09-21 to 2262-04-11)');
end;

// A div B rounded toward minus infinity (B > 0).
function InstantFloorDiv(const A, B: Int64): Int64;
begin
  Result := A div B;
  if (A mod B) < 0 then
    Dec(Result);
end;

// The days from 1970-01-01 of the proleptic Gregorian date Y-M-D (a valid date).
function InstantDaysFromCivil(Y, M, D: Int64): Int64;
begin
  if M <= 2 then
    Dec(Y);
  var Era := InstantFloorDiv(Y, 400);
  var YearOfEra := Y - Era * 400;
  var MonthFromMarch := M - 3;
  if M <= 2 then
    MonthFromMarch := M + 9;
  var DayOfYear := (153 * MonthFromMarch + 2) div 5 + D - 1;
  var DayOfEra := YearOfEra * 365 + YearOfEra div 4 - YearOfEra div 100 + DayOfYear;
  Result := Era * 146097 + DayOfEra - 719468;
end;

// The nanoseconds of the day of Nanos (0 to InstantNanosPerDay - 1), counted without multiplying
// the days back, which would overflow at the ends of the range.
function InstantInDay(const Nanos: Int64): Int64;
begin
  Result := Nanos mod InstantNanosPerDay;
  if Result < 0 then
    Result := Result + InstantNanosPerDay;
end;

// The proleptic Gregorian date of Days from 1970-01-01.
procedure InstantCivilFromDays(const Days: Int64; out Y, M, D: Int64);
begin
  var Z := Days + 719468;
  var Era := InstantFloorDiv(Z, 146097);
  var DayOfEra := Z - Era * 146097;
  var YearOfEra := (DayOfEra - DayOfEra div 1460 + DayOfEra div 36524 - DayOfEra div 146096) div 365;
  var DayOfYear := DayOfEra - (365 * YearOfEra + YearOfEra div 4 - YearOfEra div 100);
  var MonthFromMarch := (5 * DayOfYear + 2) div 153;
  D := DayOfYear - (153 * MonthFromMarch + 2) div 5 + 1;
  if MonthFromMarch < 10 then
    M := MonthFromMarch + 3
  else
    M := MonthFromMarch - 9;
  Y := YearOfEra + Era * 400;
  if M <= 2 then
    Inc(Y);
end;

// The number written by the Count digits of S from Index, or False when one is not a digit.
function InstantDigits(const S: string; Index, Count: Integer; out Value: Int64): Boolean;
begin
  Value := 0;
  if Index + Count - 1 > Length(S) then
    Exit(False);
  for var I := Index to Index + Count - 1 do
  begin
    if (S[I] < '0') or (S[I] > '9') then
      Exit(False);
    Value := Value * 10 + (Ord(S[I]) - Ord('0'));
  end;
  Result := True;
end;

class function TInstant.Make(const Nanos: Int64): TInstant;
begin
  Result := Default(TInstant);
  Result.FNanos := Nanos;
end;

// N * Factor nanoseconds (Factor > 0), ERangeError when it does not fit.
class function TInstant.Scaled(const N, Factor: Int64): TInstant;
begin
  if (N > High(Int64) div Factor) or (N < Low(Int64) div Factor) then
    InstantRangeError;
  Result := Make(N * Factor);
end;

// Seconds since the UNIX epoch and the nanoseconds after them (0 to 999999999), or False when
// the instant does not fit.
class function TInstant.TryMake(const Seconds, Fraction: Int64; out Value: TInstant): Boolean;
begin
  Value := Make(0);
  if Seconds >= 0 then
  begin
    if Seconds > High(Int64) div InstantNanosPerSecond then
      Exit(False);
    var Whole := Seconds * InstantNanosPerSecond;
    if Fraction > High(Int64) - Whole then
      Exit(False);
    Value := Make(Whole + Fraction);
  end
  else
  begin
    // From the next second back, so that the first second of the range fits
    if Seconds + 1 < Low(Int64) div InstantNanosPerSecond then
      Exit(False);
    var Whole := (Seconds + 1) * InstantNanosPerSecond;
    var Back := InstantNanosPerSecond - Fraction;
    if Whole < Low(Int64) + Back then
      Exit(False);
    Value := Make(Whole - Back);
  end;
  Result := True;
end;

class function TInstant.Now: TInstant;
begin
  Result := Make(ClockNow);
end;

class function TInstant.FromUnixNanos(const Nanos: Int64): TInstant;
begin
  Result := Make(Nanos);
end;

class function TInstant.FromUnixMillis(const Millis: Int64): TInstant;
begin
  Result := Scaled(Millis, InstantNanosPerMilli);
end;

class function TInstant.FromUnixSeconds(const Seconds: Int64): TInstant;
begin
  Result := Scaled(Seconds, InstantNanosPerSecond);
end;

// A TDateTime is the days from 1899-12-30 with the time of day as the fraction; before that day
// the fraction counts away from zero (-1.25 is 1899-12-29 06:00). Rounded to the millisecond,
// the precision of a TDateTime.
class function TInstant.FromDateTime(const Value: TDateTime): TInstant;
begin
  if not ((Value > InstantMinDateTime - 1) and (Value < InstantMaxDateTime)) then
    InstantRangeError;
  var Days: Int64 := Trunc(Value);
  var Millis: Int64 := Round(Abs(Value - Days) * InstantMillisPerDay);
  Result := Scaled(Days - InstantUnixEpochDays, InstantNanosPerDay)
    .AddNanos(Millis * InstantNanosPerMilli);
end;

class function TInstant.Parse(const S: string): TInstant;
begin
  if not TryParse(S, Result) then
    RaiseConvertError(S, 'RFC 3339 date and time');
end;

// RFC 3339: YYYY-MM-DDThh:mm:ss, an optional fraction of a second, and Z or an offset +hh:mm /
// -hh:mm. The T and the Z may be lowercase, and the T a space.
class function TInstant.TryParse(const S: string; out Value: TInstant): Boolean;
var
  Y, Mo, D, H, Mi, Sec, OffH, OffM, Fraction: Int64;
begin
  Value := Make(0);
  if not (InstantDigits(S, 1, 4, Y) and (Length(S) >= 20) and (S[5] = '-') and
    InstantDigits(S, 6, 2, Mo) and (S[8] = '-') and InstantDigits(S, 9, 2, D) and
    ((S[11] = 'T') or (S[11] = 't') or (S[11] = ' ')) and InstantDigits(S, 12, 2, H) and
    (S[14] = ':') and InstantDigits(S, 15, 2, Mi) and (S[17] = ':') and
    InstantDigits(S, 18, 2, Sec)) then
    Exit(False);
  var DaysInMonth: Int64 := 31;
  case Mo of
    4, 6, 9, 11:
      DaysInMonth := 30;
    2:
      if ((Y mod 4 = 0) and (Y mod 100 <> 0)) or (Y mod 400 = 0) then
        DaysInMonth := 29
      else
        DaysInMonth := 28;
  end;
  if (Mo < 1) or (Mo > 12) or (D < 1) or (D > DaysInMonth) or (H > 23) or (Mi > 59) or
    (Sec > 59) then
    Exit(False);
  // The fraction: any number of digits, the ones after the ninth dropped
  var I := 20;
  Fraction := 0;
  if S[I] = '.' then
  begin
    Inc(I);
    var Start := I;
    while (I <= Length(S)) and (S[I] >= '0') and (S[I] <= '9') do
    begin
      if I - Start < 9 then
        Fraction := Fraction * 10 + (Ord(S[I]) - Ord('0'));
      Inc(I);
    end;
    if I = Start then
      Exit(False);
    for var K := I - Start + 1 to 9 do
      Fraction := Fraction * 10;
  end;
  if I > Length(S) then
    Exit(False);
  var Offset: Int64 := 0;
  if (S[I] = 'Z') or (S[I] = 'z') then
    Inc(I)
  else if (S[I] = '+') or (S[I] = '-') then
  begin
    if not (InstantDigits(S, I + 1, 2, OffH) and (I + 3 <= Length(S)) and (S[I + 3] = ':') and
      InstantDigits(S, I + 4, 2, OffM)) or (OffH > 23) or (OffM > 59) then
      Exit(False);
    Offset := OffH * 3600 + OffM * 60;
    if S[I] = '-' then
      Offset := -Offset;
    I := I + 6;
  end
  else
    Exit(False);
  if I <= Length(S) then
    Exit(False);
  var Seconds := InstantDaysFromCivil(Y, Mo, D) * 86400 + H * 3600 + Mi * 60 + Sec - Offset;
  Result := TryMake(Seconds, Fraction, Value);
end;

function TInstant.ToUnixNanos: Int64;
begin
  Result := FNanos;
end;

function TInstant.ToUnixMillis: Int64;
begin
  Result := InstantFloorDiv(FNanos, InstantNanosPerMilli);
end;

function TInstant.ToUnixSeconds: Int64;
begin
  Result := InstantFloorDiv(FNanos, InstantNanosPerSecond);
end;

function TInstant.ToDateTime: TDateTime;
begin
  var Days := InstantFloorDiv(FNanos, InstantNanosPerDay);
  var Time: Double := InstantInDay(FNanos) / InstantNanosPerDay;
  Days := Days + InstantUnixEpochDays;
  if Days >= 0 then
    Result := Days + Time
  else
    Result := Days - Time;
end;

// RFC 3339 in UTC, with the fewest of 0, 3, 6 or 9 digits of the fraction that write it exactly.
function TInstant.ToString: string;
var
  Y, M, D: Int64;
begin
  var Days := InstantFloorDiv(FNanos, InstantNanosPerDay);
  var InDay := InstantInDay(FNanos);
  InstantCivilFromDays(Days, Y, M, D);
  var Secs := InDay div InstantNanosPerSecond;
  var Fraction := InDay mod InstantNanosPerSecond;
  Result := TimeSpanPad(Y, 4) + '-' + TimeSpanPad(M, 2) + '-' + TimeSpanPad(D, 2) + 'T' +
    TimeSpanPad(Secs div 3600, 2) + ':' + TimeSpanPad(Secs div 60 mod 60, 2) + ':' +
    TimeSpanPad(Secs mod 60, 2);
  if Fraction <> 0 then
    if Fraction mod 1000000 = 0 then
      Result := Result + '.' + TimeSpanPad(Fraction div 1000000, 3)
    else if Fraction mod 1000 = 0 then
      Result := Result + '.' + TimeSpanPad(Fraction div 1000, 6)
    else
      Result := Result + '.' + TimeSpanPad(Fraction, 9);
  Result := Result + 'Z';
end;

function TInstant.AddNanos(const Nanos: Int64): TInstant;
begin
  if ((Nanos > 0) and (FNanos > High(Int64) - Nanos)) or
    ((Nanos < 0) and (FNanos < Low(Int64) - Nanos)) then
    InstantRangeError;
  Result := Make(FNanos + Nanos);
end;

function TInstant.AddMillis(const Millis: Int64): TInstant;
begin
  Result := AddNanos(Scaled(Millis, InstantNanosPerMilli).FNanos);
end;

function TInstant.AddSeconds(const Seconds: Int64): TInstant;
begin
  Result := AddNanos(Scaled(Seconds, InstantNanosPerSecond).FNanos);
end;

function TInstant.NanosSince(const Other: TInstant): Int64;
begin
  if ((Other.FNanos < 0) and (FNanos > High(Int64) + Other.FNanos)) or
    ((Other.FNanos > 0) and (FNanos < Low(Int64) + Other.FNanos)) then
    raise ERangeError.Create('the nanoseconds between the instants are out of range of Int64');
  Result := FNanos - Other.FNanos;
end;

function TInstant.CompareTo(const Other: TInstant): Integer;
begin
  if FNanos < Other.FNanos then
    Result := -1
  else if FNanos > Other.FNanos then
    Result := 1
  else
    Result := 0;
end;

function TInstant.GetHashCode: Int64;
begin
  Result := KeyHash(FNanos);
end;

class operator TInstant.Equal(const A, B: TInstant): Boolean;
begin
  Result := A.FNanos = B.FNanos;
end;

class operator TInstant.NotEqual(const A, B: TInstant): Boolean;
begin
  Result := A.FNanos <> B.FNanos;
end;

class operator TInstant.LessThan(const A, B: TInstant): Boolean;
begin
  Result := A.FNanos < B.FNanos;
end;

class operator TInstant.LessThanOrEqual(const A, B: TInstant): Boolean;
begin
  Result := A.FNanos <= B.FNanos;
end;

class operator TInstant.GreaterThan(const A, B: TInstant): Boolean;
begin
  Result := A.FNanos > B.FNanos;
end;

class operator TInstant.GreaterThanOrEqual(const A, B: TInstant): Boolean;
begin
  Result := A.FNanos >= B.FNanos;
end;

class operator TInstant.Add(const A: TInstant; const B: TTimeSpan): TInstant;
begin
  Result := A.AddNanos(Scaled(B.Ticks, InstantNanosPerTick).FNanos);
end;

class operator TInstant.Add(const A: TTimeSpan; const B: TInstant): TInstant;
begin
  Result := B.AddNanos(Scaled(A.Ticks, InstantNanosPerTick).FNanos);
end;

class operator TInstant.Subtract(const A: TInstant; const B: TTimeSpan): TInstant;
begin
  // Ticks * 100 is never Low(Int64), so it can be negated
  Result := A.AddNanos(-Scaled(B.Ticks, InstantNanosPerTick).FNanos);
end;

// The ticks between the instants, the nanoseconds below a tick dropped (toward zero). They
// always fit: the nanoseconds of the whole range of TInstant are 100 times the ticks.
class operator TInstant.Subtract(const A, B: TInstant): TTimeSpan;
begin
  // A - B = 100 * Q + R with |R| < 200, counted without overflowing Int64
  var Q := A.FNanos div InstantNanosPerTick - B.FNanos div InstantNanosPerTick;
  var R := A.FNanos mod InstantNanosPerTick - B.FNanos mod InstantNanosPerTick;
  var Ticks := Q + R div InstantNanosPerTick;
  R := R mod InstantNanosPerTick;
  if (Ticks > 0) and (R < 0) then
    Dec(Ticks)
  else if (Ticks < 0) and (R > 0) then
    Inc(Ticks);
  Result := TTimeSpan.FromTicks(Ticks);
end;
