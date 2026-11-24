unit System.DateUtils;

// Routines on TDateTime (docs/spec/stdlib/System.DateUtils/README.md): the parts of a date and
// time, adding years to milliseconds, the distance between two values, the start and the end of
// a day, a week, a month and a year, comparisons, ISO 8601 text and UNIX time. Written from the
// specification only, not from any other implementation.
//
// Every routine works on the milliseconds from 1899-12-30 that System.SysUtils's DecodeDate and
// DecodeTime give (DuToMillis / DuFromMillis), so that a value is rounded to the millisecond in
// the same way as there, and the calendar is the one of System.SysUtils.
//
// Split as System.Generics.Collections (docs/spec/collections.md 1.2): Name.Interface.inc holds
// the declarations and Name.Implementation.inc the bodies of one group of routines, read only
// when a program names one of them. Parts is the parts of a value, validity and comparisons;
// Arith adding to a value and the distance between two; Bounds the start and the end of a period;
// Convert ISO 8601 text and UNIX time.

interface

uses
  System.SysUtils;

const
  DaysPerWeek = 7;
  WeeksPerFortnight = 2;
  MonthsPerYear = 12;
  YearsPerDecade = 10;
  YearsPerCentury = 100;
  YearsPerMillennium = 1000;

  // The days of the week as DayOfTheWeek gives them (ISO 8601: Monday is 1)
  DayMonday = 1;
  DayTuesday = 2;
  DayWednesday = 3;
  DayThursday = 4;
  DayFriday = 5;
  DaySaturday = 6;
  DaySunday = 7;

  // Lengths of time as TDateTime (days)
  OneHour = 1 / HoursPerDay;
  OneMinute = 1 / MinsPerDay;
  OneSecond = 1 / SecsPerDay;
  OneMillisecond = 1 / MSecsPerDay;

{$I System.DateUtils/Parts.Interface.inc}

{$I System.DateUtils/Arith.Interface.inc}

{$I System.DateUtils/Bounds.Interface.inc}

{$I System.DateUtils/Convert.Interface.inc}

implementation

const
  // The milliseconds from 1899-12-30 of 0001-01-01 and of 10000-01-01
  DuMinMillis = -59926435200000;
  DuEndMillis = 255611462400000;
  DuNanosPerMilli = 1000000;
  DuMillisPerHour = 3600000;
  DuMillisPerMinute = 60000;
  DuMillisPerWeek = 604800000;

// A div B rounded toward minus infinity (B > 0).
function DuFloorDiv(const A, B: Int64): Int64;
begin
  Result := A div B;
  if (A mod B) < 0 then
    Dec(Result);
end;

// N in decimal, with zeros before it up to Width digits (N >= 0).
function DuPad(const N: Int64; Width: Integer): string;
begin
  Result := IntToStr(N);
  while Length(Result) < Width do
    Result := '0' + Result;
end;

type
  // The parts of a date and time
  TDuParts = record
    Year, Month, Day, Hour, Minute, Second, MilliSecond: Word;
  end;

// The parts of a TDateTime, rounded to the millisecond as System.SysUtils rounds it;
// ERangeError outside the years 1 to 9999 (from DecodeDate).
function DuParts(const DateTime: TDateTime): TDuParts;
begin
  Result := Default(TDuParts);
  DecodeDate(DateTime, Result.Year, Result.Month, Result.Day);
  DecodeTime(DateTime, Result.Hour, Result.Minute, Result.Second, Result.MilliSecond);
end;

// The milliseconds from 1899-12-30 of a TDateTime (as DuParts).
function DuToMillis(const DateTime: TDateTime): Int64;
begin
  var P := DuParts(DateTime);
  Result := Trunc(EncodeDate(P.Year, P.Month, P.Day)) * Int64(MSecsPerDay) +
    ((Int64(P.Hour) * 60 + P.Minute) * 60 + P.Second) * 1000 + P.MilliSecond;
end;

// The days from 1899-12-30 of Millis.
function DuDays(const Millis: Int64): Int64;
begin
  Result := DuFloorDiv(Millis, MSecsPerDay);
end;

// The milliseconds of the day of Millis (0 to MSecsPerDay - 1).
function DuInDay(const Millis: Int64): Int64;
begin
  Result := Millis - DuDays(Millis) * MSecsPerDay;
end;

// The TDateTime of milliseconds from 1899-12-30; ERangeError outside the years 1 to 9999.
function DuFromMillis(const Millis: Int64): TDateTime;
begin
  if (Millis < DuMinMillis) or (Millis >= DuEndMillis) then
    raise ERangeError.Create('the resulting date is out of range (the years 1 to 9999)');
  Result := DuDays(Millis);
  // ReplaceTime puts the time on either side of 1899-12-30 as System.SysUtils reads it
  ReplaceTime(Result, DuInDay(Millis) / MSecsPerDay);
end;

// Millis + N * Unit, ERangeError when the result is outside the years 1 to 9999 (or does not
// fit in Int64 on the way).
function DuAddMillis(const Millis, N, Unit_: Int64): TDateTime;
begin
  // The whole range is below 2^49 milliseconds, so a larger step is out of it anyway
  if (N > (Int64(1) shl 50) div Unit_) or (N < -((Int64(1) shl 50) div Unit_)) then
    raise ERangeError.Create('the resulting date is out of range (the years 1 to 9999)');
  Result := DuFromMillis(Millis + N * Unit_);
end;

// The ISO 8601 day of the week of Days from 1899-12-30 (1, Monday, to 7).
function DuIsoDayOfWeek(const Days: Int64): Integer;
begin
  // 1899-12-30 was a Saturday (6)
  Result := (Days mod 7 + 12) mod 7 + 1;
end;

// The milliseconds from 1899-12-30 of the UTC time of the UNIX time Nanos.
function DuMillisOfNanos(const Nanos: Int64): Int64;
begin
  Result := DuFloorDiv(Nanos, DuNanosPerMilli) + Int64(UnixDateDelta) * MSecsPerDay;
end;

// The offset of the local time from UTC (seconds) at the UTC time Millis. The host is asked
// about the instant, clamped to the range of TInstant's nanoseconds (1677 to 2262).
function DuOffsetAtUtc(const Millis: Int64): Int64;
begin
  var Unix := Millis - Int64(UnixDateDelta) * MSecsPerDay;
  var Limit := High(Int64) div DuNanosPerMilli;
  if Unix > Limit then
    Unix := Limit
  else if Unix < -Limit then
    Unix := -Limit;
  Result := LocalOffset(Unix * DuNanosPerMilli);
end;

// The offset of the local time from UTC (seconds) at the local time Millis: the offset at the
// instant the local time is when read with the offset at the same digits as UTC.
function DuOffsetAtLocal(const Millis: Int64): Int64;
begin
  var Guess := DuOffsetAtUtc(Millis);
  Result := DuOffsetAtUtc(Millis - Guess * MSecsPerSec);
end;

{$I System.DateUtils/Parts.Implementation.inc}

{$I System.DateUtils/Arith.Implementation.inc}

{$I System.DateUtils/Bounds.Implementation.inc}

{$I System.DateUtils/Convert.Implementation.inc}

end.
