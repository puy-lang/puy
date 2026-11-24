// ===== TTimeSpan (docs/spec/stdlib/System/TTimeSpan.md) =====

procedure TimeSpanRangeError;
begin
  raise ERangeError.Create('TTimeSpan value out of range');
end;

// N in decimal, with zeros before it up to Width digits (N >= 0). TInstant uses it too (its part
// is always read with this one).
function TimeSpanPad(const N: Int64; Width: Integer): string;
begin
  Result := IntToStr(N);
  while Length(Result) < Width do
    Result := '0' + Result;
end;

// A + B, ERangeError when it does not fit in Int64.
function TimeSpanAdd(const A, B: Int64): Int64;
begin
  if ((B > 0) and (A > High(Int64) - B)) or ((B < 0) and (A < Low(Int64) - B)) then
    TimeSpanRangeError;
  Result := A + B;
end;

// A - B, ERangeError when it does not fit in Int64.
function TimeSpanSubtract(const A, B: Int64): Int64;
begin
  if ((B < 0) and (A > High(Int64) + B)) or ((B > 0) and (A < Low(Int64) + B)) then
    TimeSpanRangeError;
  Result := A - B;
end;

// The digits of S from Index on (at least one), or False when there is none or the number does
// not fit in Int64. Index moves past them; Count is how many there were.
function TimeSpanNumber(const S: string; var Index: Integer; out Value: Int64;
  out Count: Integer): Boolean;
begin
  Value := 0;
  Count := 0;
  while (Index <= Length(S)) and (S[Index] >= '0') and (S[Index] <= '9') do
  begin
    var Digit := Ord(S[Index]) - Ord('0');
    if Value > (High(Int64) - Digit) div 10 then
      Exit(False);
    Value := Value * 10 + Digit;
    Inc(Index);
    Inc(Count);
  end;
  Result := Count > 0;
end;

class function TTimeSpan.Make(const Ticks: Int64): TTimeSpan;
begin
  Result := Default(TTimeSpan);
  Result.FTicks := Ticks;
end;

// Value * Factor ticks rounded to the nearest tick (ties to even); ERangeError for NaN and
// outside Int64.
class function TTimeSpan.Scaled(const Value: Double; const Factor: Int64): TTimeSpan;
begin
  var Ticks: Double := Value * Factor;
  // -2^63 fits, 2^63 does not
  if not ((Ticks >= -9223372036854775808.0) and (Ticks < 9223372036854775808.0)) then
    TimeSpanRangeError;
  Result := Make(Round(Ticks));
end;

constructor TTimeSpan.Create(const ATicks: Int64);
begin
  FTicks := ATicks;
end;

constructor TTimeSpan.Create(Hours, Minutes, Seconds: Integer);
begin
  Self := TTimeSpan.Create(0, Hours, Minutes, Seconds, 0);
end;

constructor TTimeSpan.Create(Days, Hours, Minutes, Seconds: Integer);
begin
  Self := TTimeSpan.Create(Days, Hours, Minutes, Seconds, 0);
end;

constructor TTimeSpan.Create(Days, Hours, Minutes, Seconds, Milliseconds: Integer);
begin
  // The milliseconds fit in Int64 whatever the Integer parts (|Days| * 86400000 < 2^58)
  var Millis: Int64 := (((Int64(Days) * 24 + Hours) * 60 + Minutes) * 60 + Seconds) * 1000 +
    Milliseconds;
  if (Millis > High(Int64) div TicksPerMillisecond) or
    (Millis < Low(Int64) div TicksPerMillisecond) then
    TimeSpanRangeError;
  FTicks := Millis * TicksPerMillisecond;
end;

class function TTimeSpan.FromDays(const Value: Double): TTimeSpan;
begin
  Result := Scaled(Value, TicksPerDay);
end;

class function TTimeSpan.FromHours(const Value: Double): TTimeSpan;
begin
  Result := Scaled(Value, TicksPerHour);
end;

class function TTimeSpan.FromMinutes(const Value: Double): TTimeSpan;
begin
  Result := Scaled(Value, TicksPerMinute);
end;

class function TTimeSpan.FromSeconds(const Value: Double): TTimeSpan;
begin
  Result := Scaled(Value, TicksPerSecond);
end;

class function TTimeSpan.FromMilliseconds(const Value: Double): TTimeSpan;
begin
  Result := Scaled(Value, TicksPerMillisecond);
end;

class function TTimeSpan.FromTicks(const Value: Int64): TTimeSpan;
begin
  Result := Make(Value);
end;

class function TTimeSpan.GetZero: TTimeSpan;
begin
  Result := Make(0);
end;

class function TTimeSpan.GetMinValue: TTimeSpan;
begin
  Result := Make(Low(Int64));
end;

class function TTimeSpan.GetMaxValue: TTimeSpan;
begin
  Result := Make(High(Int64));
end;

// [-][d.]h:mm[:ss[.fffffff]] or [-]d, with spaces around it.
class function TTimeSpan.TryParseText(const S: string; out Value: TTimeSpan): Boolean;
var
  First, Hours, Minutes, Seconds, Fraction: Int64;
  Count: Integer;
begin
  Value := Make(0);
  var I := 1;
  var Last := Length(S);
  while (I <= Last) and (S[I] = ' ') do
    Inc(I);
  while (Last >= I) and (S[Last] = ' ') do
    Dec(Last);
  var Text := Copy(S, I, Last - I + 1);
  I := 1;
  var Negative := (I <= Length(Text)) and (Text[I] = '-');
  if Negative then
    Inc(I);
  if not TimeSpanNumber(Text, I, First, Count) then
    Exit(False);
  var Days: Int64 := 0;
  Seconds := 0;
  Fraction := 0;
  if I > Length(Text) then
  begin
    // Days alone
    Days := First;
    Hours := 0;
    Minutes := 0;
  end
  else
  begin
    if Text[I] = '.' then
    begin
      Days := First;
      Inc(I);
      if not TimeSpanNumber(Text, I, Hours, Count) or (Count > 2) then
        Exit(False);
    end
    else
    begin
      if Count > 2 then
        Exit(False);
      Hours := First;
    end;
    if (I > Length(Text)) or (Text[I] <> ':') then
      Exit(False);
    Inc(I);
    if not TimeSpanNumber(Text, I, Minutes, Count) or (Count <> 2) then
      Exit(False);
    if (I <= Length(Text)) and (Text[I] = ':') then
    begin
      Inc(I);
      if not TimeSpanNumber(Text, I, Seconds, Count) or (Count <> 2) then
        Exit(False);
      if (I <= Length(Text)) and (Text[I] = '.') then
      begin
        Inc(I);
        if not TimeSpanNumber(Text, I, Fraction, Count) or (Count > 7) then
          Exit(False);
        while Count < 7 do
        begin
          Fraction := Fraction * 10;
          Inc(Count);
        end;
      end;
    end;
    if I <= Length(Text) then
      Exit(False);
    if (Hours > 23) or (Minutes > 59) or (Seconds > 59) then
      Exit(False);
  end;
  // Counted below zero, so that MinValue reads back
  if Days > High(Int64) div TicksPerDay then
    Exit(False);
  var InDay := ((Hours * 60 + Minutes) * 60 + Seconds) * TicksPerSecond + Fraction;
  var Neg := -(Days * TicksPerDay);
  if Neg < Low(Int64) + InDay then
    Exit(False);
  Neg := Neg - InDay;
  if not Negative then
  begin
    if Neg = Low(Int64) then
      Exit(False);
    Neg := -Neg;
  end;
  Value := Make(Neg);
  Result := True;
end;

class function TTimeSpan.Parse(const S: string): TTimeSpan;
begin
  if not TryParseText(S, Result) then
    RaiseConvertError(S, 'time span');
end;

class function TTimeSpan.TryParse(const S: string; out Value: TTimeSpan): Boolean;
begin
  Result := TryParseText(S, Value);
end;

function TTimeSpan.GetDays: Integer;
begin
  Result := FTicks div TicksPerDay;
end;

function TTimeSpan.GetHours: Integer;
begin
  Result := FTicks div TicksPerHour mod 24;
end;

function TTimeSpan.GetMinutes: Integer;
begin
  Result := FTicks div TicksPerMinute mod 60;
end;

function TTimeSpan.GetSeconds: Integer;
begin
  Result := FTicks div TicksPerSecond mod 60;
end;

function TTimeSpan.GetMilliseconds: Integer;
begin
  Result := FTicks div TicksPerMillisecond mod 1000;
end;

function TTimeSpan.GetTotalDays: Double;
begin
  Result := FTicks / TicksPerDay;
end;

function TTimeSpan.GetTotalHours: Double;
begin
  Result := FTicks / TicksPerHour;
end;

function TTimeSpan.GetTotalMinutes: Double;
begin
  Result := FTicks / TicksPerMinute;
end;

function TTimeSpan.GetTotalSeconds: Double;
begin
  Result := FTicks / TicksPerSecond;
end;

function TTimeSpan.GetTotalMilliseconds: Double;
begin
  Result := FTicks / TicksPerMillisecond;
end;

function TTimeSpan.Add(const TS: TTimeSpan): TTimeSpan;
begin
  Result := Make(TimeSpanAdd(FTicks, TS.FTicks));
end;

function TTimeSpan.Subtract(const TS: TTimeSpan): TTimeSpan;
begin
  Result := Make(TimeSpanSubtract(FTicks, TS.FTicks));
end;

function TTimeSpan.Duration: TTimeSpan;
begin
  if FTicks < 0 then
    Result := Negate
  else
    Result := Self;
end;

function TTimeSpan.Negate: TTimeSpan;
begin
  if FTicks = Low(Int64) then
    TimeSpanRangeError;
  Result := Make(-FTicks);
end;

function TTimeSpan.CompareTo(const Other: TTimeSpan): Integer;
begin
  if FTicks < Other.FTicks then
    Result := -1
  else if FTicks > Other.FTicks then
    Result := 1
  else
    Result := 0;
end;

// [-][d.]hh:mm:ss[.fffffff]: the days only when there are some, the fraction only when it is
// not zero.
function TTimeSpan.ToString: string;
begin
  // Each part is taken with its sign and made positive, so that MinValue is written too
  var Days: Int64 := Abs(FTicks div TicksPerDay);
  var Hours: Int64 := Abs(FTicks div TicksPerHour mod 24);
  var Minutes: Int64 := Abs(FTicks div TicksPerMinute mod 60);
  var Secs: Int64 := Abs(FTicks div TicksPerSecond mod 60);
  var Fraction: Int64 := Abs(FTicks mod TicksPerSecond);
  Result := '';
  if FTicks < 0 then
    Result := '-';
  if Days <> 0 then
    Result := Result + IntToStr(Days) + '.';
  Result := Result + TimeSpanPad(Hours, 2) + ':' + TimeSpanPad(Minutes, 2) + ':' +
    TimeSpanPad(Secs, 2);
  if Fraction <> 0 then
    Result := Result + '.' + TimeSpanPad(Fraction, 7);
end;

function TTimeSpan.GetHashCode: Int64;
begin
  Result := KeyHash(FTicks);
end;

class operator TTimeSpan.Add(const A, B: TTimeSpan): TTimeSpan;
begin
  Result := A.Add(B);
end;

class operator TTimeSpan.Subtract(const A, B: TTimeSpan): TTimeSpan;
begin
  Result := A.Subtract(B);
end;

class operator TTimeSpan.Negative(const A: TTimeSpan): TTimeSpan;
begin
  Result := A.Negate;
end;

class operator TTimeSpan.Positive(const A: TTimeSpan): TTimeSpan;
begin
  Result := A;
end;

class operator TTimeSpan.Equal(const A, B: TTimeSpan): Boolean;
begin
  Result := A.FTicks = B.FTicks;
end;

class operator TTimeSpan.NotEqual(const A, B: TTimeSpan): Boolean;
begin
  Result := A.FTicks <> B.FTicks;
end;

class operator TTimeSpan.LessThan(const A, B: TTimeSpan): Boolean;
begin
  Result := A.FTicks < B.FTicks;
end;

class operator TTimeSpan.LessThanOrEqual(const A, B: TTimeSpan): Boolean;
begin
  Result := A.FTicks <= B.FTicks;
end;

class operator TTimeSpan.GreaterThan(const A, B: TTimeSpan): Boolean;
begin
  Result := A.FTicks > B.FTicks;
end;

class operator TTimeSpan.GreaterThanOrEqual(const A, B: TTimeSpan): Boolean;
begin
  Result := A.FTicks >= B.FTicks;
end;
