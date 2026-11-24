// The parts of a date and time, validity, today, and comparisons.

function YearOf(const AValue: TDateTime): Word;
begin
  Result := DuParts(AValue).Year;
end;

function MonthOf(const AValue: TDateTime): Word;
begin
  Result := DuParts(AValue).Month;
end;

function DayOf(const AValue: TDateTime): Word;
begin
  Result := DuParts(AValue).Day;
end;

function HourOf(const AValue: TDateTime): Word;
begin
  Result := DuParts(AValue).Hour;
end;

function MinuteOf(const AValue: TDateTime): Word;
begin
  Result := DuParts(AValue).Minute;
end;

function SecondOf(const AValue: TDateTime): Word;
begin
  Result := DuParts(AValue).Second;
end;

function MilliSecondOf(const AValue: TDateTime): Word;
begin
  Result := DuParts(AValue).MilliSecond;
end;

procedure DecodeDateTime(const AValue: TDateTime; out AYear, AMonth, ADay, AHour, AMinute,
  ASecond, AMilliSecond: Word);
begin
  DecodeDate(AValue, AYear, AMonth, ADay);
  DecodeTime(AValue, AHour, AMinute, ASecond, AMilliSecond);
end;

function EncodeDateTime(const AYear, AMonth, ADay, AHour, AMinute, ASecond,
  AMilliSecond: Word): TDateTime;
begin
  Result := EncodeDate(AYear, AMonth, ADay);
  ReplaceTime(Result, EncodeTime(AHour, AMinute, ASecond, AMilliSecond));
end;

function TryEncodeDateTime(const AYear, AMonth, ADay, AHour, AMinute, ASecond,
  AMilliSecond: Word; out AValue: TDateTime): Boolean;
var
  Time: TDateTime;
begin
  Result := TryEncodeDate(AYear, AMonth, ADay, AValue) and
    TryEncodeTime(AHour, AMinute, ASecond, AMilliSecond, Time);
  if Result then
    ReplaceTime(AValue, Time)
  else
    AValue := 0;
end;

function DayOfTheYear(const AValue: TDateTime): Word;
begin
  var Days := DuDays(DuToMillis(AValue));
  Result := Days - Trunc(EncodeDate(YearOf(AValue), 1, 1)) + 1;
end;

function DayOfTheWeek(const AValue: TDateTime): Word;
begin
  Result := DuIsoDayOfWeek(DuDays(DuToMillis(AValue)));
end;

// The Thursday of the ISO 8601 week of a value, which decides the year of the week (it is within
// the years 1 to 9999: 0001-01-01 was a Monday and 9999-12-31 a Friday).
function DuThursday(const AValue: TDateTime): Int64;
begin
  var Days := DuDays(DuToMillis(AValue));
  Result := Days - DuIsoDayOfWeek(Days) + DayThursday;
end;

// The number of the week of Thursday, a day of the year Year.
function DuWeekNumber(const Thursday: Int64; Year: Word): Word;
begin
  Result := (Thursday - Trunc(EncodeDate(Year, 1, 1))) div 7 + 1;
end;

function WeekOfTheYear(const AValue: TDateTime; out AYear: Word): Word;
begin
  var Thursday := DuThursday(AValue);
  AYear := YearOf(Thursday);
  Result := DuWeekNumber(Thursday, AYear);
end;

function WeekOfTheYear(const AValue: TDateTime): Word;
begin
  var Thursday := DuThursday(AValue);
  Result := DuWeekNumber(Thursday, YearOf(Thursday));
end;

function WeekOf(const AValue: TDateTime): Word;
begin
  Result := WeekOfTheYear(AValue);
end;

function DaysInAMonth(const AYear, AMonth: Word): Word;
begin
  case AMonth of
    4, 6, 9, 11:
      Result := 30;
    2:
      if IsLeapYear(AYear) then
        Result := 29
      else
        Result := 28;
  else
    Result := 31;
  end;
end;

function DaysInAYear(const AYear: Word): Word;
begin
  if IsLeapYear(AYear) then
    Result := 366
  else
    Result := 365;
end;

function DaysInMonth(const AValue: TDateTime): Word;
begin
  var P := DuParts(AValue);
  Result := DaysInAMonth(P.Year, P.Month);
end;

function DaysInYear(const AValue: TDateTime): Word;
begin
  Result := DaysInAYear(YearOf(AValue));
end;

function WeeksInAYear(const AYear: Word): Word;
begin
  // 53 when the year starts on a Thursday, or on a Wednesday in a leap year
  var First := DuIsoDayOfWeek(Trunc(EncodeDate(AYear, 1, 1)));
  if (First = DayThursday) or ((First = DayWednesday) and IsLeapYear(AYear)) then
    Result := 53
  else
    Result := 52;
end;

function WeeksInYear(const AValue: TDateTime): Word;
begin
  Result := WeeksInAYear(YearOf(AValue));
end;

function IsInLeapYear(const AValue: TDateTime): Boolean;
begin
  Result := IsLeapYear(YearOf(AValue));
end;

function IsPM(const AValue: TDateTime): Boolean;
begin
  Result := HourOf(AValue) >= 12;
end;

// The same ranges as EncodeDate and EncodeTime.
function IsValidDate(const AYear, AMonth, ADay: Word): Boolean;
begin
  Result := (AYear >= 1) and (AYear <= 9999) and (AMonth >= 1) and (AMonth <= MonthsPerYear) and
    (ADay >= 1) and (ADay <= DaysInAMonth(AYear, AMonth));
end;

function IsValidTime(const AHour, AMinute, ASecond, AMilliSecond: Word): Boolean;
begin
  Result := (AHour < HoursPerDay) and (AMinute < MinsPerHour) and (ASecond < SecsPerMin) and
    (AMilliSecond < MSecsPerSec);
end;

function IsValidDateTime(const AYear, AMonth, ADay, AHour, AMinute, ASecond,
  AMilliSecond: Word): Boolean;
begin
  Result := IsValidDate(AYear, AMonth, ADay) and IsValidTime(AHour, AMinute, ASecond, AMilliSecond);
end;

function DateOf(const AValue: TDateTime): TDateTime;
begin
  Result := DuDays(DuToMillis(AValue));
end;

function TimeOf(const AValue: TDateTime): TDateTime;
begin
  Result := DuInDay(DuToMillis(AValue)) / MSecsPerDay;
end;

function Today: TDateTime;
begin
  Result := Date;
end;

function Yesterday: TDateTime;
begin
  Result := DuFromMillis((DuDays(DuToMillis(Date)) - 1) * MSecsPerDay);
end;

function Tomorrow: TDateTime;
begin
  Result := DuFromMillis((DuDays(DuToMillis(Date)) + 1) * MSecsPerDay);
end;

function IsSameDay(const AValue, ABasis: TDateTime): Boolean;
begin
  Result := DuDays(DuToMillis(AValue)) = DuDays(DuToMillis(ABasis));
end;

function IsToday(const AValue: TDateTime): Boolean;
begin
  Result := IsSameDay(AValue, Date);
end;

// -1, 0 or 1.
function DuCompare(const A, B: Int64): Integer;
begin
  if A < B then
    Result := -1
  else if A > B then
    Result := 1
  else
    Result := 0;
end;

function CompareDateTime(const A, B: TDateTime): Integer;
begin
  Result := DuCompare(DuToMillis(A), DuToMillis(B));
end;

function CompareDate(const A, B: TDateTime): Integer;
begin
  Result := DuCompare(DuDays(DuToMillis(A)), DuDays(DuToMillis(B)));
end;

function CompareTime(const A, B: TDateTime): Integer;
begin
  Result := DuCompare(DuInDay(DuToMillis(A)), DuInDay(DuToMillis(B)));
end;

function SameDateTime(const A, B: TDateTime): Boolean;
begin
  Result := CompareDateTime(A, B) = 0;
end;

function SameDate(const A, B: TDateTime): Boolean;
begin
  Result := CompareDate(A, B) = 0;
end;

function SameTime(const A, B: TDateTime): Boolean;
begin
  Result := CompareTime(A, B) = 0;
end;
